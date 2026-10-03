use crate::audit::{AuditEntry, AuditLogger};
use crate::auth::ApiKeyClaims;
use crate::services::context::{ContextError, ContextService};
use arcanum_core::{
    config::GenerateConfig,
    traits::{GenerationEvent, GenerationRequest, GenerationUsage, Generator, StopReason},
    types::*,
    ArcanumError,
};
use arcanum_generate::{build_prompt, parse_citations};
use arcanum_middleware::CircuitBreaker;
use futures::stream::{self, BoxStream, StreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::time::{timeout_at, Instant};

#[derive(Debug)]
pub enum GenerateError {
    Invalid(String),
    Forbidden(String),
    Unavailable(String),
    Upstream(String),
    Timeout,
    Internal(ArcanumError),
}

impl std::fmt::Display for GenerateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GenerateError::Invalid(m)
            | GenerateError::Forbidden(m)
            | GenerateError::Unavailable(m)
            | GenerateError::Upstream(m) => f.write_str(m),
            GenerateError::Timeout => f.write_str("generation timed out"),
            GenerateError::Internal(e) => write!(f, "{e}"),
        }
    }
}

impl From<ContextError> for GenerateError {
    fn from(e: ContextError) -> Self {
        match e {
            ContextError::Invalid(m) => GenerateError::Invalid(m),
            ContextError::Forbidden(m) => GenerateError::Forbidden(m),
            ContextError::Unavailable(m) => GenerateError::Unavailable(m),
            ContextError::Internal(e) => GenerateError::Internal(e),
        }
    }
}

pub struct GeneratorEntry {
    pub generator: Arc<dyn Generator>,
    pub max_output_tokens: u32,
    pub breaker: Arc<CircuitBreaker>,
}

#[derive(Debug)]
pub enum GenerateEvent {
    Delta(String),
    Done(GenerateOutcome),
    Error(GenerateError),
}

pub struct GenerateStream {
    pub context: ContextResponse,
    pub events: BoxStream<'static, GenerateEvent>,
}

pub struct GenerateService {
    context: Arc<ContextService>,
    generators: HashMap<String, GeneratorEntry>,
    config: GenerateConfig,
    audit: Arc<AuditLogger>,
}

impl std::fmt::Debug for GenerateService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GenerateService").finish_non_exhaustive()
    }
}

impl GenerateService {
    pub fn new(
        context: Arc<ContextService>,
        generators: HashMap<String, GeneratorEntry>,
        config: GenerateConfig,
        audit: Arc<AuditLogger>,
    ) -> Self {
        Self {
            context,
            generators,
            config,
            audit,
        }
    }

    /// Validates, assembles context and checks the generator breaker. The
    /// generator itself is called lazily when `events` is first polled.
    pub async fn generate_stream(
        &self,
        req: GenerateRequest,
        claims: &ApiKeyClaims,
    ) -> Result<GenerateStream, GenerateError> {
        req.validate().map_err(GenerateError::Invalid)?;
        let name = req
            .generator
            .clone()
            .or_else(|| self.config.default_generator.clone())
            .unwrap_or_default();
        let entry = self
            .generators
            .get(&name)
            .ok_or_else(|| GenerateError::Invalid(format!("unknown generator '{name}'")))?;
        let cap = entry.max_output_tokens;
        if matches!(req.max_tokens, Some(m) if m > cap) {
            return Err(GenerateError::Invalid(format!(
                "max_tokens must be at most {cap}"
            )));
        }
        let max_tokens = req
            .max_tokens
            .unwrap_or(self.config.default_max_tokens.min(cap));
        let budget = match req.mode {
            GenerateMode::Answer => None,
            GenerateMode::Summarize => Some(self.config.summarize_token_budget),
        };

        let context = self
            .context
            .assemble(req.to_context_request(budget), claims)
            .await?;

        let info = GeneratorInfo {
            name: name.clone(),
            model: entry.generator.model().to_string(),
        };
        let recorder = Recorder {
            audit: self.audit.clone(),
            user_id: claims.user_id.clone(),
            collection_id: req.collection_id.clone(),
            generator: name.clone(),
            mode: mode_name(req.mode),
        };

        if context.passages.is_empty() {
            let outcome = GenerateOutcome {
                status: GenerateStatus::NoContext,
                citations: vec![],
                unknown_refs: vec![],
                stop_reason: StopReason::EndTurn,
                usage: GenerationUsage::default(),
                generator: info,
            };
            let answer = self.config.no_context_answer.clone();
            let events = stream::once(async move { GenerateEvent::Delta(answer) })
                .chain(stream::once(async move {
                    recorder.finish("no_context", None, None).await;
                    GenerateEvent::Done(outcome)
                }))
                .boxed();
            return Ok(GenerateStream { context, events });
        }

        if !entry.breaker.allow_request() {
            return Err(GenerateError::Unavailable(format!(
                "circuit open: generator '{name}' unavailable"
            )));
        }

        let docs = context.rendered.clone().unwrap_or_default();
        let prompt = build_prompt(
            req.mode,
            req.query.as_deref(),
            req.messages.as_deref(),
            &docs,
            req.instructions.as_deref(),
            self.config.history_max_messages,
        );
        let run = Run {
            phase: Phase::Start(GenerationRequest {
                system: prompt.system,
                messages: prompt.messages,
                max_tokens,
                temperature: req.temperature,
            }),
            generator: entry.generator.clone(),
            breaker: entry.breaker.clone(),
            passages: context.passages.clone(),
            info,
            answer: String::new(),
            got_first: false,
            started: Instant::now(),
            first_token_timeout: Duration::from_secs(self.config.first_token_timeout_secs),
            total_timeout: Duration::from_secs(self.config.total_timeout_secs),
            recorder,
        };
        let events = stream::unfold(run, |mut run| async move {
            let event = run.next_event().await?;
            Some((event, run))
        })
        .boxed();
        Ok(GenerateStream { context, events })
    }

    /// Drains `generate_stream`; an `Error` event becomes `Err`.
    pub async fn generate(
        &self,
        req: GenerateRequest,
        claims: &ApiKeyClaims,
    ) -> Result<GenerateResponse, GenerateError> {
        let GenerateStream {
            context,
            mut events,
        } = self.generate_stream(req, claims).await?;
        let mut answer = String::new();
        while let Some(event) = events.next().await {
            match event {
                GenerateEvent::Delta(t) => answer.push_str(&t),
                GenerateEvent::Done(outcome) => {
                    return Ok(GenerateResponse {
                        answer,
                        outcome,
                        context,
                    })
                }
                GenerateEvent::Error(e) => return Err(e),
            }
        }
        Err(GenerateError::Upstream(UPSTREAM_FAILED.into()))
    }
}

fn mode_name(mode: GenerateMode) -> &'static str {
    match mode {
        GenerateMode::Answer => "answer",
        GenerateMode::Summarize => "summarize",
    }
}

/// Records metrics and the audit entry for a terminal event.
struct Recorder {
    audit: Arc<AuditLogger>,
    user_id: String,
    collection_id: String,
    generator: String,
    mode: &'static str,
}

impl Recorder {
    async fn finish(
        &self,
        status: &'static str,
        elapsed: Option<Duration>,
        usage: Option<&GenerationUsage>,
    ) {
        metrics::counter!("arcanum_generation_total",
            "generator" => self.generator.clone(), "mode" => self.mode, "status" => status)
        .increment(1);
        if let Some(d) = elapsed {
            metrics::histogram!("arcanum_generation_duration_seconds",
                "generator" => self.generator.clone())
            .record(d.as_secs_f64());
        }
        if let Some(u) = usage {
            for (kind, n) in [("input", u.input_tokens), ("output", u.output_tokens)] {
                if let Some(n) = n {
                    metrics::counter!("arcanum_generation_tokens_total",
                        "generator" => self.generator.clone(), "kind" => kind)
                    .increment(u64::from(n));
                }
            }
        }
        self.audit
            .log(AuditEntry {
                operation: "generate".into(),
                user_id: self.user_id.clone(),
                collection_id: self.collection_id.clone(),
                result: status.into(),
            })
            .await;
    }
}

enum Phase {
    Start(GenerationRequest),
    Streaming(BoxStream<'static, arcanum_core::Result<GenerationEvent>>),
    Finished,
}

fn error_detail(e: arcanum_core::ArcanumError) -> String {
    match e {
        arcanum_core::ArcanumError::Generation(m) => m,
        other => other.to_string(),
    }
}

/// Fixed client-facing text for any upstream failure; the detail is logged.
const UPSTREAM_FAILED: &str = "generation failed";

/// State of one generation, driven by `stream::unfold` without a spawned
/// task, so dropping the stream drops the upstream request.
struct Run {
    phase: Phase,
    generator: Arc<dyn Generator>,
    breaker: Arc<CircuitBreaker>,
    passages: Vec<Passage>,
    info: GeneratorInfo,
    answer: String,
    got_first: bool,
    started: Instant,
    first_token_timeout: Duration,
    total_timeout: Duration,
    recorder: Recorder,
}

impl Run {
    fn deadline(&self) -> Instant {
        let total = self.started + self.total_timeout;
        if self.got_first {
            total
        } else {
            total.min(self.started + self.first_token_timeout)
        }
    }

    async fn next_event(&mut self) -> Option<GenerateEvent> {
        loop {
            match std::mem::replace(&mut self.phase, Phase::Finished) {
                Phase::Finished => return None,
                Phase::Start(req) => {
                    self.started = Instant::now();
                    let generator = self.generator.clone();
                    match timeout_at(self.deadline(), generator.stream(req)).await {
                        Err(_) => return Some(self.timeout().await),
                        Ok(Err(e)) => return Some(self.fail(error_detail(e)).await),
                        Ok(Ok(s)) => self.phase = Phase::Streaming(s),
                    }
                }
                Phase::Streaming(mut s) => match timeout_at(self.deadline(), s.next()).await {
                    Err(_) => return Some(self.timeout().await),
                    Ok(None) => {
                        return Some(self.fail("stream ended before completion".into()).await)
                    }
                    Ok(Some(Err(e))) => return Some(self.fail(error_detail(e)).await),
                    Ok(Some(Ok(GenerationEvent::TextDelta(t)))) => {
                        self.got_first = true;
                        self.answer.push_str(&t);
                        self.phase = Phase::Streaming(s);
                        return Some(GenerateEvent::Delta(t));
                    }
                    Ok(Some(Ok(GenerationEvent::Done { usage, stop_reason }))) => {
                        self.breaker.record_success();
                        let parsed = parse_citations(&self.answer, &self.passages);
                        self.recorder
                            .finish("ok", Some(self.started.elapsed()), Some(&usage))
                            .await;
                        return Some(GenerateEvent::Done(GenerateOutcome {
                            status: GenerateStatus::Ok,
                            citations: parsed.citations,
                            unknown_refs: parsed.unknown_refs,
                            stop_reason,
                            usage,
                            generator: self.info.clone(),
                        }));
                    }
                },
            }
        }
    }

    async fn fail(&mut self, msg: String) -> GenerateEvent {
        tracing::warn!(generator = %self.info.name, error = %msg, "generation failed");
        self.breaker.record_failure();
        self.recorder
            .finish("error", Some(self.started.elapsed()), None)
            .await;
        GenerateEvent::Error(GenerateError::Upstream(UPSTREAM_FAILED.into()))
    }

    async fn timeout(&mut self) -> GenerateEvent {
        self.breaker.record_failure();
        self.recorder
            .finish("timeout", Some(self.started.elapsed()), None)
            .await;
        GenerateEvent::Error(GenerateError::Timeout)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::AuthMiddleware;
    use crate::services::test_support::{seeded_registry, RegistryRetriever};
    use arcanum_core::config::ContextConfig;
    use arcanum_core::traits::{
        ApproxCl100kCounter, GenerationUsage, Retriever, ScriptStep, ScriptedGenerator, StopReason,
    };
    use arcanum_retrieval::{OrchestratorMode, RetrievalOrchestrator};
    use futures::StreamExt;
    use std::time::Duration;

    struct EmptyRetriever;
    #[async_trait::async_trait]
    impl Retriever for EmptyRetriever {
        async fn retrieve(&self, _q: &Query) -> arcanum_core::Result<Vec<RetrievedChunk>> {
            Ok(vec![])
        }
        fn strategy(&self) -> RetrievalStrategy {
            RetrievalStrategy::Vector
        }
    }

    struct Fixture {
        svc: GenerateService,
        gen: Arc<ScriptedGenerator>,
        breaker: Arc<CircuitBreaker>,
        audit: Arc<AuditLogger>,
        claims: ApiKeyClaims,
        config: GenerateConfig,
    }

    struct Opts {
        steps: Vec<ScriptStep>,
        empty: bool,
        threshold: u32,
        config: GenerateConfig,
    }

    fn opts(steps: Vec<ScriptStep>) -> Opts {
        Opts {
            steps,
            empty: false,
            threshold: 5,
            config: GenerateConfig {
                default_generator: Some("fake".into()),
                ..GenerateConfig::default()
            },
        }
    }

    fn ok_steps() -> Vec<ScriptStep> {
        vec![
            ScriptStep::Delta("Acme builds rockets ".into()),
            ScriptStep::Delta("[P1].".into()),
            ScriptStep::Done(StopReason::EndTurn),
        ]
    }

    async fn fixture(o: Opts) -> Fixture {
        let (registry, chunk) = seeded_registry().await;
        let retriever: Arc<dyn Retriever> = if o.empty {
            Arc::new(EmptyRetriever)
        } else {
            Arc::new(RegistryRetriever {
                chunk,
                strategy: RetrievalStrategy::Vector,
            })
        };
        let orchestrator =
            RetrievalOrchestrator::new(OrchestratorMode::ParallelFusion).add_retriever(retriever);
        let auth = Arc::new(AuthMiddleware::new("a-32-char-secret-for-testing-ok!"));
        let audit = Arc::new(AuditLogger::new());
        let context = Arc::new(ContextService::new(
            Arc::new(orchestrator),
            registry,
            None,
            Arc::new(ApproxCl100kCounter::new()),
            ContextConfig::default(),
            auth.clone(),
            audit.clone(),
            Arc::new(CircuitBreaker::new(
                "vector_store",
                5,
                Duration::from_secs(30),
            )),
        ));
        let gen = Arc::new(ScriptedGenerator::new("m", o.steps));
        let breaker = Arc::new(CircuitBreaker::new(
            "generator:fake",
            o.threshold,
            Duration::from_secs(30),
        ));
        let mut generators = HashMap::new();
        generators.insert(
            "fake".to_string(),
            GeneratorEntry {
                generator: gen.clone(),
                max_output_tokens: 100,
                breaker: breaker.clone(),
            },
        );
        let svc = GenerateService::new(context, generators, o.config.clone(), audit.clone());
        let claims = auth
            .validate_api_key(&auth.generate_admin_key("tester"))
            .unwrap();
        Fixture {
            svc,
            gen,
            breaker,
            audit,
            claims,
            config: o.config,
        }
    }

    fn req(query: &str) -> GenerateRequest {
        GenerateRequest {
            collection_id: "col1".into(),
            mode: GenerateMode::Answer,
            query: Some(query.into()),
            messages: None,
            generator: None,
            max_tokens: None,
            temperature: None,
            instructions: None,
            context: Default::default(),
            stream: false,
        }
    }

    fn open(cb: &CircuitBreaker) {
        for _ in 0..5 {
            cb.record_failure();
        }
    }

    #[tokio::test]
    async fn no_context_skips_generator_even_with_open_breaker() {
        let mut o = opts(ok_steps());
        o.empty = true;
        let f = fixture(o).await;
        open(&f.breaker);
        let resp = f.svc.generate(req("fox"), &f.claims).await.unwrap();
        assert_eq!(resp.outcome.status, GenerateStatus::NoContext);
        assert_eq!(resp.answer, f.config.no_context_answer);
        assert_eq!(resp.outcome.usage, GenerationUsage::default());
        assert_eq!(resp.outcome.generator.name, "fake");
        assert_eq!(resp.outcome.stop_reason, StopReason::EndTurn);
        assert!(resp.outcome.citations.is_empty() && resp.outcome.unknown_refs.is_empty());
        assert_eq!(f.gen.calls(), 0);
    }

    #[tokio::test]
    async fn unknown_generator_is_invalid() {
        let f = fixture(opts(ok_steps())).await;
        let mut r = req("fox");
        r.generator = Some("nope".into());
        let err = f.svc.generate(r, &f.claims).await.unwrap_err();
        assert!(
            matches!(&err, GenerateError::Invalid(m) if m == "unknown generator 'nope'"),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn max_tokens_over_cap_is_invalid() {
        let f = fixture(opts(ok_steps())).await;
        let mut r = req("fox");
        r.max_tokens = Some(101);
        let err = f.svc.generate(r, &f.claims).await.unwrap_err();
        assert!(
            matches!(&err, GenerateError::Invalid(m) if m == "max_tokens must be at most 100"),
            "{err:?}"
        );
        assert_eq!(f.gen.calls(), 0);
    }

    #[tokio::test]
    async fn default_max_tokens_clamped_to_cap() {
        let f = fixture(opts(ok_steps())).await;
        f.svc.generate(req("fox"), &f.claims).await.unwrap();
        assert_eq!(f.gen.last_request().unwrap().max_tokens, 100);
    }

    #[tokio::test]
    async fn prompt_contains_rendered_context() {
        let f = fixture(opts(ok_steps())).await;
        f.svc.generate(req("fox"), &f.claims).await.unwrap();
        let last = f.gen.last_request().unwrap();
        assert!(last.system.starts_with("You answer questions"));
        let msg = &last.messages.last().unwrap().content;
        assert!(msg.contains("<documents>"), "{msg}");
        assert!(msg.ends_with("Question: fox"), "{msg}");
    }

    #[tokio::test]
    async fn open_breaker_is_unavailable() {
        let f = fixture(opts(ok_steps())).await;
        open(&f.breaker);
        let err = f.svc.generate(req("fox"), &f.claims).await.unwrap_err();
        assert!(
            matches!(&err, GenerateError::Unavailable(m)
                if m == "circuit open: generator 'fake' unavailable"),
            "{err:?}"
        );
        assert_eq!(f.gen.calls(), 0);
    }

    #[tokio::test]
    async fn first_token_timeout() {
        let mut o = opts(vec![ScriptStep::Hang]);
        o.threshold = 1;
        o.config.first_token_timeout_secs = 1;
        let f = fixture(o).await;
        let err = f.svc.generate(req("fox"), &f.claims).await.unwrap_err();
        assert!(matches!(err, GenerateError::Timeout), "{err:?}");
        assert_eq!(err.to_string(), "generation timed out");
        assert!(!f.breaker.allow_request(), "timeout must record a failure");
        let records = f.audit.query(10).await;
        assert_eq!(records[0].entry.operation, "generate");
        assert_eq!(records[0].entry.result, "timeout");
    }

    #[tokio::test]
    async fn stream_ended_without_done_is_upstream() {
        let steps = vec![
            ScriptStep::Delta("a".into()),
            ScriptStep::Fail("stream ended before completion".into()),
        ];
        let f = fixture(opts(steps)).await;
        let s = f.svc.generate_stream(req("fox"), &f.claims).await.unwrap();
        let events: Vec<GenerateEvent> = s.events.collect().await;
        assert_eq!(events.len(), 2, "{events:?}");
        assert!(matches!(&events[0], GenerateEvent::Delta(t) if t == "a"));
        assert!(
            matches!(&events[1], GenerateEvent::Error(GenerateError::Upstream(m))
                if m == "generation failed"),
            "{events:?}"
        );
        let err = f.svc.generate(req("fox"), &f.claims).await.unwrap_err();
        assert!(matches!(err, GenerateError::Upstream(_)), "{err:?}");
        let records = f.audit.query(1).await;
        assert_eq!(records[0].entry.result, "error");
    }

    #[tokio::test]
    async fn upstream_detail_is_not_exposed_to_clients() {
        let steps = vec![ScriptStep::Fail(
            "openai_compatible returned 401: Incorrect API key sk-proj-ab****wxyz at http://10.0.3.17:8000"
                .into(),
        )];
        let f = fixture(opts(steps)).await;
        let err = f.svc.generate(req("fox"), &f.claims).await.unwrap_err();
        assert_eq!(err.to_string(), "generation failed");
        let s = f.svc.generate_stream(req("fox"), &f.claims).await.unwrap();
        let events: Vec<GenerateEvent> = s.events.collect().await;
        match events.last().unwrap() {
            GenerateEvent::Error(e) => assert_eq!(e.to_string(), "generation failed"),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[tokio::test]
    async fn json_and_stream_agree() {
        let f = fixture(opts(ok_steps())).await;
        let s = f.svc.generate_stream(req("fox"), &f.claims).await.unwrap();
        let events: Vec<GenerateEvent> = s.events.collect().await;
        let mut streamed = String::new();
        let mut done = None;
        for e in events {
            match e {
                GenerateEvent::Delta(t) => streamed.push_str(&t),
                GenerateEvent::Done(o) => done = Some(o),
                GenerateEvent::Error(e) => panic!("unexpected error {e:?}"),
            }
        }
        let stream_outcome = done.expect("done event");
        let resp = f.svc.generate(req("fox"), &f.claims).await.unwrap();
        assert_eq!(resp.answer, "Acme builds rockets [P1].");
        assert_eq!(streamed, resp.answer);
        assert_eq!(stream_outcome, resp.outcome);
        assert_eq!(resp.outcome.status, GenerateStatus::Ok);
        assert_eq!(resp.outcome.citations.len(), 1);
        assert_eq!(resp.outcome.citations[0].ref_id, "P1");
        assert_eq!(resp.outcome.citations[0].answer_spans, vec![(20, 24)]);
        assert_eq!(
            resp.outcome.usage,
            GenerationUsage {
                input_tokens: Some(10),
                output_tokens: Some(5)
            }
        );
        assert!(f.breaker.allow_request());
    }

    #[tokio::test]
    async fn summarize_uses_summarize_budget() {
        let f = fixture(opts(ok_steps())).await;
        let mut r = req("fox");
        r.mode = GenerateMode::Summarize;
        let resp = f.svc.generate(r, &f.claims).await.unwrap();
        assert_eq!(resp.context.usage.budget, 8000);
        let resp = f.svc.generate(req("fox"), &f.claims).await.unwrap();
        assert_eq!(resp.context.usage.budget, 4000);
    }

    #[tokio::test]
    async fn writes_generate_and_context_audit_entries() {
        let f = fixture(opts(ok_steps())).await;
        f.svc.generate(req("fox"), &f.claims).await.unwrap();
        let records = f.audit.query(10).await;
        let mut ops: Vec<&str> = records.iter().map(|r| r.entry.operation.as_str()).collect();
        ops.sort();
        assert_eq!(ops, vec!["context", "generate"]);
        let g = records
            .iter()
            .find(|r| r.entry.operation == "generate")
            .unwrap();
        assert_eq!(g.entry.result, "ok");
        assert_eq!(g.entry.user_id, "tester");
        assert_eq!(g.entry.collection_id, "col1");
    }
}
