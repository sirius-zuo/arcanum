use crate::settings::Settings;
use arcanum_core::traits::InMemoryChunkMetadataStore;
use arcanum_engine::auth::ApiKeyClaims;
use arcanum_engine::ArcanumEngine;
use serde::Serialize;
use std::sync::Arc;

/// What the UI shows about a registered generator.
#[derive(Debug, Clone, Serialize)]
pub struct GeneratorMeta {
    pub name: String,
    pub protocol: String,
    pub model: String,
    pub is_default: bool,
}

/// Shared state for the demo layer and the UI-facing routes.
#[derive(Clone)]
pub struct AtlasState {
    pub engine: Arc<ArcanumEngine>,
    pub registry: Arc<InMemoryChunkMetadataStore>,
    pub settings: Settings,
    pub admin_key: String,
    pub metrics_token: String,
    pub claims: ApiKeyClaims,
    pub generators: Vec<GeneratorMeta>,
    pub judge: Option<String>,
    pub mcp: Arc<arcanum_mcp::McpJsonRpcHandler>,
}
