use super::{require_key, DemoCtx, DemoError};
use crate::samples::Manifest;
use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;

pub async fn samples(
    State(ctx): State<DemoCtx>,
    headers: HeaderMap,
) -> Result<Json<Manifest>, DemoError> {
    require_key(&ctx.state, &headers)?;
    Ok(Json((*ctx.manifest).clone()))
}
