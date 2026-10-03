//! The `/demo` layer: what the REST API lacks, for the showcase UI.

pub mod auth;
pub mod bootstrap;
pub mod documents;
pub mod health;
pub mod ingest;
pub mod library;
pub mod samples;

use crate::samples::Manifest;
use crate::state::AtlasState;
use axum::routing::{get, post};
use axum::Router;
use std::sync::Arc;

pub use auth::{require_key, DemoError};
pub use health::{HttpOllamaProbe, OllamaProbe};

/// Everything a demo route needs.
#[derive(Clone)]
pub struct DemoCtx {
    pub state: Arc<AtlasState>,
    pub manifest: Arc<Manifest>,
    pub probe: Arc<dyn OllamaProbe>,
}

/// Mounts every `/demo` route. Later tasks add one `.route(..)` line per endpoint.
pub fn demo_router(
    state: Arc<AtlasState>,
    manifest: Arc<Manifest>,
    probe: Arc<dyn OllamaProbe>,
) -> Router {
    let ctx = DemoCtx {
        state,
        manifest,
        probe,
    };
    Router::new()
        .route("/demo/bootstrap", get(bootstrap::bootstrap))
        .route("/demo/health", get(health::health))
        .route("/demo/samples", get(samples::samples))
        .route("/demo/library", get(library::library))
        .route(
            "/demo/documents/:document_id/versions/:n/text",
            get(documents::document_text),
        )
        .route("/demo/samples/load", post(ingest::load))
        .route("/demo/samples/apply-update", post(ingest::apply_update))
        .with_state(ctx)
}
