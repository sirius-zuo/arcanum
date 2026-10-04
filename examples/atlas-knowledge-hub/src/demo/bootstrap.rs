use super::{DemoCtx, DemoError};
use crate::engine_setup::COLLECTION;
use crate::settings::is_local_host_header;
use crate::state::GeneratorMeta;
use axum::extract::State;
use axum::http::HeaderMap;
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

/// Unauthenticated by design (it hands the demo key to the UI). While the server is bound to
/// loopback only, a `Host` that is not local is refused so a DNS-rebinding page cannot read it.
pub async fn bootstrap(
    State(ctx): State<DemoCtx>,
    headers: HeaderMap,
) -> Result<Json<Bootstrap>, DemoError> {
    let s = &ctx.state;
    if s.settings.is_loopback_bind() {
        if let Some(host) = headers.get("host").and_then(|h| h.to_str().ok()) {
            if !is_local_host_header(host) {
                return Err(DemoError::Forbidden(
                    "the demo key is only served to local hosts; set ATLAS_HOST to widen".into(),
                ));
            }
        }
    }
    let e = &s.engine;
    Ok(Json(Bootstrap {
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
    }))
}
