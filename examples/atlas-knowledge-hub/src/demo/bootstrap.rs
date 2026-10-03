use super::DemoCtx;
use crate::engine_setup::COLLECTION;
use crate::state::GeneratorMeta;
use axum::extract::State;
use axum::Json;
use serde::Serialize;

#[derive(Serialize)]
pub struct Features {
    pub context: bool,
    pub generate: bool,
    pub verify: bool,
    pub evidence: bool,
    pub experiments: bool,
    pub gc: bool,
}

#[derive(Serialize)]
pub struct Bootstrap {
    pub api_key: String,
    pub collection: &'static str,
    pub orchestration_mode: String,
    pub generators: Vec<GeneratorMeta>,
    pub judge: Option<String>,
    pub anthropic_enabled: bool,
    pub ollama_url: String,
    pub mcp_port: u16,
    pub features: Features,
}

pub async fn bootstrap(State(ctx): State<DemoCtx>) -> Json<Bootstrap> {
    let s = &ctx.state;
    let e = &s.engine;
    Json(Bootstrap {
        api_key: s.admin_key.clone(),
        collection: COLLECTION,
        orchestration_mode: format!("{:?}", e.config.retrieval.orchestration_mode),
        generators: s.generators.clone(),
        judge: s.judge.clone(),
        anthropic_enabled: s.settings.anthropic_key.is_some(),
        ollama_url: s.settings.ollama_url.clone(),
        mcp_port: s.settings.mcp_port,
        features: Features {
            context: e.context.is_some(),
            generate: e.generate.is_some(),
            verify: e.verify.is_some(),
            evidence: e.evidence.is_some(),
            experiments: true,
            gc: false,
        },
    })
}
