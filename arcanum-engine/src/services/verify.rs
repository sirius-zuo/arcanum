use crate::audit::{AuditEntry, AuditLogger};
use crate::auth::{ApiKeyClaims, AuthMiddleware};
use crate::services::generate::GeneratorEntry;
use arcanum_core::{
    config::VerifyConfig,
    traits::{
        ChunkMetadataStore, DocumentVersionStore, GenerationEvent, GenerationRequest,
        GenerationUsage, StopReason, TokenCounter,
    },
    types::*,
    ArcanumError,
};
use arcanum_verify::{
    attribute, build_sentences, join_chunks, overall, parse_judge_output, plan_batches,
    retry_message, segment, user_message, HydratedPassage, JoinError, JudgeSentence,
    JudgedSentence, JUDGE_SYSTEM_PROMPT,
};
use futures::stream::{self, StreamExt, TryStreamExt};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub const VERIFY_UNAVAILABLE: &str =
    "verification requires a configured judge and a chunk registry";

/// Judge batches that run at once.
const BATCH_CONCURRENCY: usize = 4;

#[derive(Debug)]
pub enum VerifyError {
    Invalid(String),
    Forbidden(String),
    Unavailable(String),
    Upstream,
    InvalidOutput,
    Timeout,
    Internal(ArcanumError),
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerifyError::Invalid(m) | VerifyError::Forbidden(m) | VerifyError::Unavailable(m) => {
                f.write_str(m)
            }
            VerifyError::Upstream => f.write_str("judge request failed"),
            VerifyError::InvalidOutput => f.write_str("judge returned invalid output"),
            VerifyError::Timeout => f.write_str("judge timed out"),
            VerifyError::Internal(e) => write!(f, "{e}"),
        }
    }
}

impl VerifyError {
    pub fn code(&self) -> &'static str {
        match self {
            VerifyError::Invalid(_) => "invalid",
            VerifyError::Forbidden(_) => "forbidden",
            VerifyError::Unavailable(_) => "judge_unavailable",
            VerifyError::Upstream => "judge_upstream",
            VerifyError::InvalidOutput => "judge_invalid_output",
            VerifyError::Timeout => "judge_timeout",
            VerifyError::Internal(_) => "internal",
        }
    }
}

fn version_status(v: Option<&DocumentVersion>) -> &'static str {
    match v.map(|v| &v.status) {
        Some(VersionStatus::Active) => "active",
        Some(VersionStatus::Superseded) => "superseded",
        Some(VersionStatus::Deleted) => "deleted",
        None => "unknown",
    }
}

fn verdict_name(v: SentenceVerdict) -> &'static str {
    match v {
        SentenceVerdict::Supported => "supported",
        SentenceVerdict::Miscited => "miscited",
        SentenceVerdict::UncitedSupported => "uncited_supported",
        SentenceVerdict::Partial => "partial",
        SentenceVerdict::Unsupported => "unsupported",
        SentenceVerdict::NoClaim => "no_claim",
    }
}

fn add_tokens(a: Option<u32>, b: Option<u32>) -> Option<u32> {
    match (a, b) {
        (None, None) => None,
        _ => Some(a.unwrap_or(0).saturating_add(b.unwrap_or(0))),
    }
}

/// Result of judging one batch, retry included.
struct BatchOutcome {
    judged: Vec<JudgedSentence>,
    usage: GenerationUsage,
    calls: u32,
}

struct JudgeReply {
    raw: String,
    truncated: bool,
    usage: GenerationUsage,
}

struct Hydrated {
    hydrated: Vec<HydratedPassage>,
    unavailable: Vec<String>,
}

pub struct VerifyService {
    registry: Arc<dyn ChunkMetadataStore>,
    version_store: Arc<dyn DocumentVersionStore>,
    generators: Arc<HashMap<String, GeneratorEntry>>,
    counter: Arc<dyn TokenCounter>,
    config: VerifyConfig,
    auth: Arc<AuthMiddleware>,
    audit: Arc<AuditLogger>,
}

impl std::fmt::Debug for VerifyService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("VerifyService").finish_non_exhaustive()
    }
}

impl VerifyService {
    pub fn new(
        registry: Arc<dyn ChunkMetadataStore>,
        version_store: Arc<dyn DocumentVersionStore>,
        generators: Arc<HashMap<String, GeneratorEntry>>,
        counter: Arc<dyn TokenCounter>,
        config: VerifyConfig,
        auth: Arc<AuthMiddleware>,
        audit: Arc<AuditLogger>,
    ) -> Self {
        Self {
            registry,
            version_store,
            generators,
            counter,
            config,
            auth,
            audit,
        }
    }

    pub async fn verify(
        &self,
        req: VerifyRequest,
        claims: &ApiKeyClaims,
    ) -> Result<VerifyResponse, VerifyError> {
        let started = Instant::now();
        let collection_id = req.collection_id.clone();
        let result = self.run(req, claims).await;
        let (outcome, audit_result) = match &result {
            Ok(r) => {
                for s in &r.sentences {
                    metrics::counter!("arcanum_verify_sentences_total",
                        "verdict" => verdict_name(s.verdict))
                    .increment(1);
                }
                let c = &r.counts;
                let verdict = match r.verdict {
                    OverallVerdict::Pass => "pass",
                    OverallVerdict::Fail => "fail",
                };
                (
                    "ok",
                    format!(
                        "{verdict} supported={} miscited={} uncited_supported={} partial={} \
                         unsupported={} no_claim={} judge={} calls={}",
                        c.supported,
                        c.miscited,
                        c.uncited_supported,
                        c.partial,
                        c.unsupported,
                        c.no_claim,
                        r.judge.name,
                        r.usage.judge_calls
                    ),
                )
            }
            Err(e) => (e.code(), e.code().to_string()),
        };
        metrics::counter!("arcanum_verify_requests_total", "outcome" => outcome).increment(1);
        metrics::histogram!("arcanum_verify_duration_seconds")
            .record(started.elapsed().as_secs_f64());
        self.audit
            .log(AuditEntry {
                operation: "verify".into(),
                user_id: claims.user_id.clone(),
                collection_id,
                result: audit_result,
            })
            .await;
        result
    }

    async fn run(
        &self,
        req: VerifyRequest,
        claims: &ApiKeyClaims,
    ) -> Result<VerifyResponse, VerifyError> {
        req.validate(self.config.max_answer_chars, self.config.max_passages)
            .map_err(VerifyError::Invalid)?;
        if !self.auth.can_access_collection(claims, &req.collection_id) {
            return Err(VerifyError::Forbidden(format!(
                "not authorised to access collection '{}'",
                req.collection_id
            )));
        }
        let name = req
            .judge
            .clone()
            .or_else(|| self.config.judge.clone())
            .unwrap_or_default();
        let entry = self
            .generators
            .get(&name)
            .ok_or_else(|| VerifyError::Invalid(format!("unknown judge: {name}")))?;

        let passages = self.hydrate(&req).await?;
        let available: Vec<String> = passages.hydrated.iter().map(|p| p.ref_id.clone()).collect();
        let units = segment(&req.answer);
        let attributions = attribute(&req.answer, &units, &available);
        let sentences: Vec<JudgeSentence> = units
            .iter()
            .zip(&attributions)
            .enumerate()
            .filter(|(_, (u, _))| !u.code)
            .map(|(i, (u, a))| JudgeSentence {
                id: i + 1,
                text: req.answer[u.span.0..u.span.1].to_string(),
                cited: a.cited.clone(),
            })
            .collect();

        let mut judged: HashMap<usize, JudgedSentence> = HashMap::new();
        let mut usage = GenerationUsage {
            input_tokens: Some(0),
            output_tokens: Some(0),
        };
        let mut calls = 0u32;
        if !available.is_empty() && !sentences.is_empty() {
            let prompt_passages: Vec<Passage> =
                passages.hydrated.iter().map(|p| p.to_passage()).collect();
            let batches = plan_batches(
                &prompt_passages,
                sentences,
                self.counter.as_ref(),
                self.config.max_judge_input_tokens,
                self.config.max_sentences_per_batch,
            )
            .map_err(|_| VerifyError::Invalid("passages exceed the judge input budget".into()))?;
            if !entry.breaker.allow_request() {
                return Err(VerifyError::Unavailable(format!(
                    "circuit open: judge '{name}' unavailable"
                )));
            }
            let outcomes: Vec<BatchOutcome> = stream::iter(batches)
                .map(|batch| {
                    let user = user_message(&prompt_passages, &batch);
                    let ids: Vec<usize> = batch.iter().map(|s| s.id).collect();
                    let available = &available;
                    async move { self.judge_batch(entry, user, &ids, available).await }
                })
                .buffered(BATCH_CONCURRENCY)
                .try_collect()
                .await?;
            usage = GenerationUsage::default();
            for o in outcomes {
                calls += o.calls;
                usage.input_tokens = add_tokens(usage.input_tokens, o.usage.input_tokens);
                usage.output_tokens = add_tokens(usage.output_tokens, o.usage.output_tokens);
                judged.extend(o.judged.into_iter().map(|j| (j.id, j)));
            }
        }

        let results = build_sentences(
            &req.answer,
            &units,
            &attributions,
            &judged,
            &passages.hydrated,
        );
        let mut counts = VerdictCounts::default();
        for s in &results {
            counts.add(s.verdict);
        }
        Ok(VerifyResponse {
            verdict: overall(&counts, req.strict_citations),
            strict_citations: req.strict_citations,
            counts,
            sentences: results,
            passages_unavailable: passages.unavailable,
            judge: GeneratorInfo {
                name,
                model: entry.generator.model().to_string(),
            },
            usage: VerifyUsage {
                input_tokens: usage.input_tokens,
                output_tokens: usage.output_tokens,
                judge_calls: calls,
            },
        })
    }

    /// Loads every requested chunk in one query and joins each passage's chunks.
    async fn hydrate(&self, req: &VerifyRequest) -> Result<Hydrated, VerifyError> {
        let all_ids: Vec<ChunkId> = req
            .passages
            .iter()
            .flat_map(|p| p.chunk_ids.iter().cloned())
            .collect();
        let records = self
            .registry
            .get_many(&all_ids)
            .await
            .map_err(VerifyError::Internal)?;
        let by_id: HashMap<&ChunkId, &ChunkMetadataRecord> =
            records.iter().map(|r| (&r.chunk_id, r)).collect();
        for id in &all_ids {
            if let Some(r) = by_id.get(id) {
                if r.collection_id != req.collection_id {
                    return Err(VerifyError::Invalid(format!(
                        "chunk {} does not belong to collection {}",
                        id.0, req.collection_id
                    )));
                }
            }
        }
        let mut hydrated = Vec::new();
        let mut unavailable = Vec::new();
        for p in &req.passages {
            let found: Option<Vec<ChunkMetadataRecord>> = p
                .chunk_ids
                .iter()
                .map(|id| by_id.get(id).map(|r| (*r).clone()))
                .collect();
            let Some(found) = found else {
                unavailable.push(p.ref_id.clone());
                continue;
            };
            let mut joined = join_chunks(&p.ref_id, found).map_err(|e| match e {
                JoinError::MixedVersions => VerifyError::Invalid(format!(
                    "passage {} spans multiple document versions",
                    p.ref_id
                )),
                JoinError::NotContiguous => {
                    VerifyError::Invalid(format!("passage {} chunks are not contiguous", p.ref_id))
                }
                JoinError::TextMismatch => VerifyError::Internal(ArcanumError::Storage(format!(
                    "chunk text for passage {} does not match its offsets",
                    p.ref_id
                ))),
            })?;
            let version = self
                .version_store
                .get_version(&joined.document_id, joined.version_num)
                .await
                .map_err(VerifyError::Internal)?;
            joined.version_status = version_status(version.as_ref()).to_string();
            hydrated.push(joined);
        }
        Ok(Hydrated {
            hydrated,
            unavailable,
        })
    }

    /// Judges one batch; invalid output gets exactly one retry.
    async fn judge_batch(
        &self,
        entry: &GeneratorEntry,
        user: String,
        batch_ids: &[usize],
        available: &[String],
    ) -> Result<BatchOutcome, VerifyError> {
        let mut messages = vec![Message {
            role: Role::User,
            content: user,
        }];
        let mut usage = GenerationUsage::default();
        let mut calls = 0;
        loop {
            let reply = self.call_judge(entry, messages.clone()).await?;
            calls += 1;
            usage.input_tokens = add_tokens(usage.input_tokens, reply.usage.input_tokens);
            usage.output_tokens = add_tokens(usage.output_tokens, reply.usage.output_tokens);
            match parse_judge_output(&reply.raw, reply.truncated, batch_ids, available) {
                Ok(judged) => {
                    metrics::counter!("arcanum_verify_judge_calls_total", "result" => "ok")
                        .increment(1);
                    return Ok(BatchOutcome {
                        judged,
                        usage,
                        calls,
                    });
                }
                Err(err) => {
                    metrics::counter!("arcanum_verify_judge_calls_total", "result" => "invalid")
                        .increment(1);
                    if calls >= 2 {
                        tracing::warn!(error = %err, "judge returned invalid output twice");
                        return Err(VerifyError::InvalidOutput);
                    }
                    messages.push(Message {
                        role: Role::Assistant,
                        content: reply.raw,
                    });
                    messages.push(Message {
                        role: Role::User,
                        content: retry_message(&err),
                    });
                }
            }
        }
    }

    /// One judge call, drained to completion under the judge timeout.
    async fn call_judge(
        &self,
        entry: &GeneratorEntry,
        messages: Vec<Message>,
    ) -> Result<JudgeReply, VerifyError> {
        let request = GenerationRequest {
            system: JUDGE_SYSTEM_PROMPT.to_string(),
            messages,
            max_tokens: self
                .config
                .judge_max_output_tokens
                .min(entry.max_output_tokens),
            temperature: Some(0.0),
        };
        let drained = async {
            let mut events = entry
                .generator
                .stream(request)
                .await
                .map_err(|e| e.to_string())?;
            let mut raw = String::new();
            while let Some(event) = events.next().await {
                match event.map_err(|e| e.to_string())? {
                    GenerationEvent::TextDelta(t) => raw.push_str(&t),
                    GenerationEvent::Done { usage, stop_reason } => {
                        return Ok(JudgeReply {
                            raw,
                            truncated: stop_reason == StopReason::MaxTokens,
                            usage,
                        })
                    }
                }
            }
            Err("stream ended before completion".to_string())
        };
        let timeout = Duration::from_secs(self.config.judge_timeout_secs);
        match tokio::time::timeout(timeout, drained).await {
            Err(_) => {
                entry.breaker.record_failure();
                metrics::counter!("arcanum_verify_judge_calls_total", "result" => "timeout")
                    .increment(1);
                Err(VerifyError::Timeout)
            }
            Ok(Err(detail)) => {
                tracing::warn!(error = %detail, "judge request failed");
                entry.breaker.record_failure();
                metrics::counter!("arcanum_verify_judge_calls_total", "result" => "upstream")
                    .increment(1);
                Err(VerifyError::Upstream)
            }
            Ok(Ok(reply)) => {
                entry.breaker.record_success();
                Ok(reply)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::test_support::DOC_TEXT;
    use arcanum_core::traits::{
        ApproxCl100kCounter, InMemoryChunkMetadataStore, NoOpDocumentVersionStore, ScriptStep,
        ScriptedGenerator, StopReason,
    };
    use arcanum_middleware::CircuitBreaker;
    use serde_json::json;

    struct Fixture {
        svc: VerifyService,
        store: Arc<InMemoryChunkMetadataStore>,
        gen: Arc<ScriptedGenerator>,
        breaker: Arc<CircuitBreaker>,
        audit: Arc<AuditLogger>,
        claims: ApiKeyClaims,
        auth: Arc<AuthMiddleware>,
    }

    fn ok(json: &str) -> Vec<ScriptStep> {
        vec![
            ScriptStep::Delta(json.to_string()),
            ScriptStep::Done(StopReason::EndTurn),
        ]
    }

    fn fixture(scripts: Vec<Vec<ScriptStep>>) -> Fixture {
        fixture_with(scripts, 5, VerifyConfig::default())
    }

    fn fixture_with(
        scripts: Vec<Vec<ScriptStep>>,
        threshold: u32,
        mut config: VerifyConfig,
    ) -> Fixture {
        config.judge = Some("fake".into());
        let store = Arc::new(InMemoryChunkMetadataStore::new());
        let auth = Arc::new(AuthMiddleware::new("a-32-char-secret-for-testing-ok!"));
        let audit = Arc::new(AuditLogger::new());
        let gen = Arc::new(ScriptedGenerator::with_scripts("judge-model", scripts));
        let breaker = Arc::new(CircuitBreaker::new(
            "judge:fake",
            threshold,
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
        let svc = VerifyService::new(
            store.clone(),
            Arc::new(NoOpDocumentVersionStore),
            Arc::new(generators),
            Arc::new(ApproxCl100kCounter::new()),
            config,
            auth.clone(),
            audit.clone(),
        );
        let claims = auth
            .validate_api_key(&auth.generate_admin_key("tester"))
            .unwrap();
        Fixture {
            svc,
            store,
            gen,
            breaker,
            audit,
            claims,
            auth,
        }
    }

    /// Writes one record per range, each holding `text[start..end]`.
    async fn seed(
        store: &InMemoryChunkMetadataStore,
        collection: &str,
        version: u32,
        text: &str,
        ranges: &[(usize, usize)],
    ) -> (DocumentId, Vec<ChunkId>) {
        let doc = DocumentId::new();
        let mut ids = Vec::new();
        for (i, (start, end)) in ranges.iter().enumerate() {
            let rec = ChunkMetadataRecord {
                chunk_id: ChunkId::new(),
                document_id: doc.clone(),
                collection_id: collection.into(),
                version_num: version,
                backend: ChunkBackend::Vector,
                text: text[*start..*end].to_string(),
                chunk_index: i,
                source_uri: "raw://doc".into(),
                snapshot_uri: "file:///snap/doc/1.raw".into(),
                canonical_uri: None,
                page: None,
                section: None,
                block_ids: vec![],
                offset_start: *start,
                offset_end: *end,
                ingested_at: chrono::Utc::now(),
            };
            store.put(&rec).await.unwrap();
            ids.push(rec.chunk_id);
        }
        (doc, ids)
    }

    fn claim(id: usize, text: &str, ref_id: &str, quote: &str) -> serde_json::Value {
        json!({"id": id, "kind": "claim", "claims": [
            {"text": text, "support": [{"ref": ref_id, "quote": quote}]}]})
    }

    fn unsupported_claim(id: usize, text: &str) -> serde_json::Value {
        json!({"id": id, "kind": "claim", "claims": [{"text": text, "support": []}]})
    }

    fn no_claim(id: usize) -> serde_json::Value {
        json!({"id": id, "kind": "no_claim", "claims": []})
    }

    fn judge_json(sentences: Vec<serde_json::Value>) -> String {
        json!({ "sentences": sentences }).to_string()
    }

    fn req(answer: &str, passages: Vec<(&str, Vec<ChunkId>)>) -> VerifyRequest {
        VerifyRequest {
            collection_id: "col1".into(),
            answer: answer.into(),
            passages: passages
                .into_iter()
                .map(|(r, c)| PassageRef {
                    ref_id: r.into(),
                    chunk_ids: c,
                })
                .collect(),
            judge: None,
            strict_citations: false,
        }
    }

    async fn simple(f: &Fixture, answer: &str) -> VerifyRequest {
        let (_, ids) = seed(&f.store, "col1", 1, DOC_TEXT, &[(0, 44)]).await;
        req(answer, vec![("P1", ids)])
    }

    #[tokio::test]
    async fn happy_path_with_evidence_offsets() {
        let json = judge_json(vec![
            claim(1, "The fox jumps", "P1", "fox jumps"),
            no_claim(2),
        ]);
        let f = fixture(vec![ok(&json)]);
        let (_, ids) = seed(&f.store, "col1", 1, DOC_TEXT, &[(0, 20), (15, 44)]).await;
        let r = req("The fox jumps [P1]. Hello!", vec![("P1", ids)]);
        let resp = f.svc.verify(r, &f.claims).await.unwrap();
        assert_eq!(resp.verdict, OverallVerdict::Pass);
        assert_eq!(resp.counts.supported, 1);
        assert_eq!(resp.counts.no_claim, 1);
        assert_eq!(resp.sentences.len(), 2);
        let e = &resp.sentences[0].claims[0].evidence[0];
        assert_eq!(&DOC_TEXT[e.offset_start..e.offset_end], "fox jumps");
        assert!(e.quote_matched);
        assert_eq!(e.version_status, "unknown");
        assert_eq!(resp.usage.judge_calls, 1);
        assert_eq!(resp.usage.input_tokens, Some(10));
        assert_eq!(resp.usage.output_tokens, Some(5));
        assert_eq!(resp.judge.name, "fake");
        assert_eq!(resp.judge.model, "judge-model");
        assert_eq!(f.gen.last_request().unwrap().temperature, Some(0.0));
        let records = f.audit.query(10).await;
        assert_eq!(records[0].entry.operation, "verify");
        assert_eq!(
            records[0].entry.result,
            "pass supported=1 miscited=0 uncited_supported=0 partial=0 unsupported=0 no_claim=1 judge=fake calls=1"
        );
    }

    #[tokio::test]
    async fn gc_chunk_makes_passage_unavailable() {
        let json = judge_json(vec![
            claim(1, "Fox", "P1", "fox"),
            unsupported_claim(2, "Dogs bark"),
        ]);
        let f = fixture(vec![ok(&json)]);
        let (_, ids) = seed(&f.store, "col1", 1, DOC_TEXT, &[(0, 44)]).await;
        let r = req(
            "The fox jumps [P1]. Dogs bark [P2].",
            vec![("P1", ids), ("P2", vec![ChunkId::new()])],
        );
        let resp = f.svc.verify(r, &f.claims).await.unwrap();
        assert_eq!(resp.passages_unavailable, vec!["P2".to_string()]);
        let msg = &f.gen.last_request().unwrap().messages[0].content;
        assert!(msg.contains("ref=\"P1\""), "{msg}");
        assert!(!msg.contains("ref=\"P2\""), "{msg}");
        assert_eq!(resp.sentences[1].invalid_refs, vec!["P2".to_string()]);
        assert!(resp.sentences[1].cited.is_empty());
    }

    #[tokio::test]
    async fn no_available_passage_skips_judge() {
        let f = fixture(vec![ok("{}")]);
        // The breaker is open, yet the request still succeeds: it is not checked.
        for _ in 0..5 {
            f.breaker.record_failure();
        }
        let r = req(
            "Alpha beta. Gamma delta.\n```\ncode\n```",
            vec![("P1", vec![ChunkId::new()])],
        );
        let resp = f.svc.verify(r, &f.claims).await.unwrap();
        assert_eq!(f.gen.calls(), 0);
        assert_eq!(resp.verdict, OverallVerdict::Fail);
        assert_eq!(resp.usage.judge_calls, 0);
        assert_eq!(resp.usage.input_tokens, Some(0));
        assert_eq!(resp.usage.output_tokens, Some(0));
        assert_eq!(resp.counts.unsupported, 2);
        assert_eq!(resp.counts.no_claim, 1);
        assert_eq!(resp.sentences[0].verdict, SentenceVerdict::Unsupported);
        assert_eq!(resp.sentences[1].verdict, SentenceVerdict::Unsupported);
        assert_eq!(resp.sentences[2].verdict, SentenceVerdict::NoClaim);
    }

    #[tokio::test]
    async fn cross_collection_chunk_is_400() {
        let f = fixture(vec![ok("{}")]);
        let (_, ids) = seed(&f.store, "other", 1, DOC_TEXT, &[(0, 44)]).await;
        let msg = format!("chunk {} does not belong to collection col1", ids[0].0);
        let err = f
            .svc
            .verify(req("Alpha.", vec![("P1", ids)]), &f.claims)
            .await
            .unwrap_err();
        assert!(
            matches!(&err, VerifyError::Invalid(m) if *m == msg),
            "{err:?}"
        );
        assert_eq!(err.code(), "invalid");
        assert_eq!(f.gen.calls(), 0);
    }

    #[tokio::test]
    async fn mixed_version_passage_is_400() {
        let f = fixture(vec![ok("{}")]);
        let (_, mut a) = seed(&f.store, "col1", 1, DOC_TEXT, &[(0, 20)]).await;
        let (_, b) = seed(&f.store, "col1", 2, DOC_TEXT, &[(20, 44)]).await;
        a.extend(b);
        let err = f
            .svc
            .verify(req("Alpha.", vec![("P1", a)]), &f.claims)
            .await
            .unwrap_err();
        assert!(
            matches!(&err, VerifyError::Invalid(m)
                if m == "passage P1 spans multiple document versions"),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn gap_is_400() {
        let f = fixture(vec![ok("{}")]);
        let (_, ids) = seed(&f.store, "col1", 1, DOC_TEXT, &[(0, 10), (20, 30)]).await;
        let err = f
            .svc
            .verify(req("Alpha.", vec![("P1", ids)]), &f.claims)
            .await
            .unwrap_err();
        assert!(
            matches!(&err, VerifyError::Invalid(m)
                if m == "passage P1 chunks are not contiguous"),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn invalid_request_is_400() {
        let f = fixture(vec![ok("{}")]);
        let err = f
            .svc
            .verify(req("  ", vec![("P1", vec![ChunkId::new()])]), &f.claims)
            .await
            .unwrap_err();
        assert!(matches!(&err, VerifyError::Invalid(_)), "{err:?}");
    }

    #[tokio::test]
    async fn unknown_judge_is_400() {
        let f = fixture(vec![ok("{}")]);
        let mut r = simple(&f, "Alpha.").await;
        r.judge = Some("nope".into());
        let err = f.svc.verify(r, &f.claims).await.unwrap_err();
        assert!(
            matches!(&err, VerifyError::Invalid(m) if m == "unknown judge: nope"),
            "{err:?}"
        );
    }

    #[tokio::test]
    async fn forbidden_is_403() {
        let f = fixture(vec![ok("{}")]);
        let r = simple(&f, "Alpha.").await;
        let token = f.auth.generate_api_key("u", vec!["other".into()]);
        let claims = f.auth.validate_api_key(&token).unwrap();
        let err = f.svc.verify(r, &claims).await.unwrap_err();
        assert!(
            matches!(&err, VerifyError::Forbidden(m)
                if m == "not authorised to access collection 'col1'"),
            "{err:?}"
        );
        assert_eq!(err.code(), "forbidden");
        assert_eq!(f.gen.calls(), 0);
    }

    #[tokio::test]
    async fn invalid_then_valid_retries_once() {
        let json = judge_json(vec![claim(1, "Alpha", "P1", "fox")]);
        let f = fixture(vec![ok("not json"), ok(&json)]);
        let r = simple(&f, "Alpha [P1].").await;
        let resp = f.svc.verify(r, &f.claims).await.unwrap();
        assert_eq!(resp.usage.judge_calls, 2);
        assert_eq!(resp.usage.input_tokens, Some(20));
        assert_eq!(resp.usage.output_tokens, Some(10));
        let last = f.gen.last_request().unwrap();
        assert_eq!(last.messages.len(), 3);
        assert!(
            last.messages[2]
                .content
                .contains("Your previous reply was invalid: output is not valid JSON"),
            "{}",
            last.messages[2].content
        );
        assert_eq!(last.messages[1].content, "not json");
    }

    #[tokio::test]
    async fn invalid_twice_is_invalid_output() {
        let f = fixture(vec![ok("not json")]);
        for _ in 0..5 {
            let r = simple(&f, "Alpha.").await;
            let err = f.svc.verify(r, &f.claims).await.unwrap_err();
            assert!(matches!(err, VerifyError::InvalidOutput), "{err:?}");
            assert_eq!(err.to_string(), "judge returned invalid output");
        }
        assert!(f.breaker.allow_request());
    }

    #[tokio::test]
    async fn truncated_output_retries() {
        let json = judge_json(vec![claim(1, "Alpha", "P1", "fox")]);
        let truncated = vec![
            ScriptStep::Delta(json.clone()),
            ScriptStep::Done(StopReason::MaxTokens),
        ];
        let f = fixture(vec![truncated, ok(&json)]);
        let r = simple(&f, "Alpha [P1].").await;
        let resp = f.svc.verify(r, &f.claims).await.unwrap();
        assert_eq!(resp.usage.judge_calls, 2);
        assert_eq!(f.gen.calls(), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn timeout_is_504() {
        let config = VerifyConfig {
            judge_timeout_secs: 1,
            ..VerifyConfig::default()
        };
        let f = fixture_with(vec![vec![ScriptStep::Hang]], 1, config);
        let r = simple(&f, "Alpha.").await;
        let err = f.svc.verify(r, &f.claims).await.unwrap_err();
        assert!(matches!(err, VerifyError::Timeout), "{err:?}");
        assert_eq!(err.to_string(), "judge timed out");
        assert_eq!(err.code(), "judge_timeout");
        assert!(!f.breaker.allow_request(), "timeout must record a failure");
        assert_eq!(f.audit.query(1).await[0].entry.result, "judge_timeout");
    }

    #[tokio::test]
    async fn upstream_failure_is_502_and_counts_against_breaker() {
        let f = fixture_with(
            vec![vec![ScriptStep::Fail("key sk-secret at 10.0.0.1".into())]],
            1,
            VerifyConfig::default(),
        );
        let r = simple(&f, "Alpha.").await;
        let err = f.svc.verify(r, &f.claims).await.unwrap_err();
        assert!(matches!(err, VerifyError::Upstream), "{err:?}");
        assert_eq!(err.to_string(), "judge request failed");
        assert!(!f.breaker.allow_request());
    }

    #[tokio::test]
    async fn stream_without_done_is_upstream() {
        let f = fixture_with(
            vec![vec![ScriptStep::Delta("{".into())]],
            1,
            VerifyConfig::default(),
        );
        let r = simple(&f, "Alpha.").await;
        let err = f.svc.verify(r, &f.claims).await.unwrap_err();
        assert!(matches!(err, VerifyError::Upstream), "{err:?}");
        assert!(!f.breaker.allow_request());
    }

    #[tokio::test]
    async fn open_breaker_is_503_without_call() {
        let f = fixture(vec![ok("{}")]);
        for _ in 0..5 {
            f.breaker.record_failure();
        }
        let r = simple(&f, "Alpha.").await;
        let err = f.svc.verify(r, &f.claims).await.unwrap_err();
        assert!(
            matches!(&err, VerifyError::Unavailable(m)
                if m == "circuit open: judge 'fake' unavailable"),
            "{err:?}"
        );
        assert_eq!(err.code(), "judge_unavailable");
        assert_eq!(f.gen.calls(), 0);
    }

    #[tokio::test]
    async fn one_failing_batch_fails_request() {
        let json = judge_json(vec![claim(1, "Alpha", "P1", "fox")]);
        let config = VerifyConfig {
            max_sentences_per_batch: 1,
            ..VerifyConfig::default()
        };
        let f = fixture_with(
            vec![ok(&json), vec![ScriptStep::Fail("boom".into())]],
            5,
            config,
        );
        let r = simple(&f, "Alpha one. Beta two.").await;
        let err = f.svc.verify(r, &f.claims).await.unwrap_err();
        assert!(matches!(err, VerifyError::Upstream), "{err:?}");
    }

    #[tokio::test]
    async fn batch_missing_a_sentence_is_invalid_output() {
        let config = VerifyConfig {
            max_sentences_per_batch: 1,
            ..VerifyConfig::default()
        };
        let f = fixture_with(
            vec![ok(&judge_json(vec![claim(1, "Alpha", "P1", "fox")]))],
            5,
            config,
        );
        // Both batches replay the same script, so batch 2 lacks sentence 2.
        let r = simple(&f, "Alpha one. Beta two.").await;
        let err = f.svc.verify(r, &f.claims).await.unwrap_err();
        assert!(matches!(err, VerifyError::InvalidOutput), "{err:?}");
    }

    #[test]
    fn version_status_maps_each_state() {
        let v = |status| DocumentVersion {
            document_id: DocumentId::new(),
            version_num: 1,
            source_uri: "raw://doc".into(),
            collection_id: "col1".into(),
            content_hash: "h".into(),
            snapshot_uri: "s".into(),
            canonical_uri: None,
            mime_type: "text/plain".into(),
            status,
            ingested_at: chrono::Utc::now(),
            extra: HashMap::new(),
        };
        assert_eq!(version_status(Some(&v(VersionStatus::Active))), "active");
        assert_eq!(
            version_status(Some(&v(VersionStatus::Superseded))),
            "superseded"
        );
        assert_eq!(version_status(Some(&v(VersionStatus::Deleted))), "deleted");
        assert_eq!(version_status(None), "unknown");
    }

    #[test]
    fn error_codes_follow_variant_order() {
        let codes = [
            VerifyError::Invalid(String::new()).code(),
            VerifyError::Forbidden(String::new()).code(),
            VerifyError::Unavailable(String::new()).code(),
            VerifyError::Upstream.code(),
            VerifyError::InvalidOutput.code(),
            VerifyError::Timeout.code(),
            VerifyError::Internal(ArcanumError::QueueFull).code(),
        ];
        assert_eq!(
            codes,
            [
                "invalid",
                "forbidden",
                "judge_unavailable",
                "judge_upstream",
                "judge_invalid_output",
                "judge_timeout",
                "internal"
            ]
        );
    }
}
