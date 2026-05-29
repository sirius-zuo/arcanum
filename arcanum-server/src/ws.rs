use axum::{
    extract::{ws::{WebSocket, WebSocketUpgrade, Message}, State, Query},
    http::StatusCode,
    response::IntoResponse,
};
use std::sync::Arc;
use serde::Deserialize;
use arcanum_engine::{ArcanumEngine, auth::ApiKeyClaims};

#[derive(Deserialize)]
pub struct WsParams {
    /// Short-lived token for WebSocket auth (passed as ?token=<bearer>).
    token: Option<String>,
}

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(params): Query<WsParams>,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
) -> impl IntoResponse {
    let claims = match validate_ws_token(&params.token, &engine) {
        Ok(c) => c,
        Err(status) => return status.into_response(),
    };
    ws.on_upgrade(move |socket| handle_socket(socket, claims))
}

fn validate_ws_token(token: &Option<String>, engine: &Option<Arc<ArcanumEngine>>)
    -> Result<ApiKeyClaims, StatusCode>
{
    let Some(engine) = engine else {
        return Err(StatusCode::UNAUTHORIZED);
    };
    let token = token.as_deref().ok_or(StatusCode::UNAUTHORIZED)?;
    engine.auth.validate_api_key(token).map_err(|_| StatusCode::UNAUTHORIZED)
}

async fn handle_socket(mut socket: WebSocket, claims: ApiKeyClaims) {
    while let Some(msg) = socket.recv().await {
        if let Ok(Message::Text(text)) = msg {
            if let Ok(cmd) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(topics) = cmd["subscribe"].as_array() {
                    // Filter topics to only those the caller is allowed to see.
                    let allowed: Vec<_> = topics.iter()
                        .filter(|t| {
                            let t = t.as_str().unwrap_or("");
                            // System topics (e.g. "ingestion:progress") allowed for admins;
                            // collection-scoped topics checked against claims.
                            claims.is_admin || t.starts_with("system:") ||
                            claims.allowed_collections.iter().any(|c| t.contains(c.as_str()))
                        })
                        .collect();
                    let _ = socket.send(Message::Text(
                        serde_json::json!({ "type": "subscribed", "topics": allowed }).to_string()
                    )).await;
                }
            }
        }
    }
}
