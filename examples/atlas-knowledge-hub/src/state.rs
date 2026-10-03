use crate::settings::Settings;
use arcanum_core::traits::InMemoryChunkMetadataStore;
use arcanum_engine::auth::ApiKeyClaims;
use arcanum_engine::ArcanumEngine;
use std::sync::Arc;

/// Shared state for the demo layer and the UI-facing routes.
#[derive(Clone)]
pub struct AtlasState {
    pub engine: Arc<ArcanumEngine>,
    pub registry: Arc<InMemoryChunkMetadataStore>,
    pub settings: Settings,
    pub admin_key: String,
    pub metrics_token: String,
    pub claims: ApiKeyClaims,
}
