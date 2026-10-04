//! Atlas Knowledge Hub: a showcase of every Arcanum capability.

pub mod demo;
pub mod engine_setup;
pub mod ollama_generator;
pub mod samples;
pub mod settings;
pub mod state;

pub use engine_setup::{build_state, ModelDeps};
pub use settings::Settings;
pub use state::AtlasState;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Router;
use std::path::PathBuf;
use std::sync::Arc;
use tower::ServiceExt;
use tower_http::services::{ServeDir, ServeFile};

/// Origin of the Vite dev server, allowed in addition to the same-origin proxy.
const DEV_ORIGIN: &str = "http://localhost:5173";

/// Built UI, relative to the working directory.
pub const UI_DIST: &str = "ui/dist";

/// Path prefixes owned by the API. Unknown paths below them answer JSON 404, never `index.html`.
const API_PREFIXES: [&str; 4] = ["/api/", "/demo/", "/evidence/", "/ws/"];

/// The full HTTP app: the real Arcanum API, the `/demo` layer, and `ui/dist` as an SPA
/// fallback when it exists.
pub fn assemble_app(
    state: Arc<AtlasState>,
    manifest: Arc<samples::Manifest>,
    probe: Arc<dyn demo::OllamaProbe>,
) -> Router {
    let dist = PathBuf::from(UI_DIST);
    assemble_app_with_dist(state, manifest, probe, dist.is_dir().then_some(dist))
}

/// Like [`assemble_app`] with an explicit dist directory (`None` serves no UI).
pub fn assemble_app_with_dist(
    state: Arc<AtlasState>,
    manifest: Arc<samples::Manifest>,
    probe: Arc<dyn demo::OllamaProbe>,
    dist: Option<PathBuf>,
) -> Router {
    // The engine already holds the config loaded from config.toml plus Atlas's overrides.
    let mut config = state.engine.config.clone();
    config.server.cors_allowed_origins = vec![DEV_ORIGIN.to_string()];
    let api = arcanum_server::server::build_app_with_config(Some(state.engine.clone()), config);
    let router = api.merge(demo::demo_router(state, manifest, probe));

    let spa = dist.map(|d| ServeDir::new(&d).fallback(ServeFile::new(d.join("index.html"))));
    router.fallback(move |req: Request<Body>| {
        let spa = spa.clone();
        async move {
            let api_path = API_PREFIXES.iter().any(|p| req.uri().path().starts_with(p));
            match spa {
                Some(spa) if !api_path => spa.oneshot(req).await.into_response(),
                _ => not_found(),
            }
        }
    })
}

fn not_found() -> Response {
    (
        StatusCode::NOT_FOUND,
        axum::Json(serde_json::json!({ "error": "not found" })),
    )
        .into_response()
}
