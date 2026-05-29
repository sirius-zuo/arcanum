use axum::{
    extract::{ws::{WebSocket, WebSocketUpgrade, Message}, State},
    http::{StatusCode, HeaderMap},
    response::IntoResponse,
};
use std::sync::Arc;
use arcanum_engine::{ArcanumEngine, auth::ApiKeyClaims};

/// Token delivery: client sends `Sec-WebSocket-Protocol: arcanum-v1, <jwt>`.
/// This is the browser-compatible pattern — the browser WebSocket API does not
/// allow arbitrary headers, but does allow subprotocol negotiation.
/// Non-browser clients may also use `Authorization: Bearer <jwt>` in the
/// HTTP upgrade request headers directly.
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    headers: HeaderMap,
    State(engine): State<Option<Arc<ArcanumEngine>>>,
) -> impl IntoResponse {
    let claims = match extract_and_validate_ws_token(&headers, &engine) {
        Ok(c) => c,
        Err(status) => return status.into_response(),
    };
    ws.on_upgrade(move |socket| handle_socket(socket, claims))
}

fn extract_and_validate_ws_token(
    headers: &HeaderMap,
    engine: &Option<Arc<ArcanumEngine>>,
) -> Result<ApiKeyClaims, StatusCode> {
    let Some(engine) = engine else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    // Prefer `Authorization: Bearer <token>` (non-browser clients).
    // Fall back to `Sec-WebSocket-Protocol: arcanum-v1, <token>` (browser clients).
    let token = if let Some(auth) = headers.get("Authorization") {
        auth.to_str()
            .unwrap_or("")
            .strip_prefix("Bearer ")
            .unwrap_or("")
            .to_string()
    } else if let Some(proto) = headers.get("Sec-WebSocket-Protocol") {
        // Format: "arcanum-v1, <jwt>"
        proto.to_str()
            .unwrap_or("")
            .splitn(2, ',')
            .nth(1)
            .unwrap_or("")
            .trim()
            .to_string()
    } else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    if token.is_empty() {
        return Err(StatusCode::UNAUTHORIZED);
    }

    engine.auth.validate_api_key(&token)
        .map_err(|_| StatusCode::UNAUTHORIZED)
}

/// Topic format: `<prefix>:<collection_id>` e.g. `ingestion:my-docs`, `system:health`.
/// Returns true if `claims` grants access to the given topic.
fn topic_allowed(topic: &str, claims: &ApiKeyClaims) -> bool {
    let Some((prefix, collection_id)) = topic.split_once(':') else {
        // Reject malformed topics.
        return false;
    };
    if prefix == "system" {
        // System-level topics are admin-only.
        return claims.is_admin;
    }
    // Collection-scoped topics: exact match against allowed_collections.
    claims.is_admin
        || claims.allowed_collections.iter().any(|c| c == collection_id)
}

async fn handle_socket(mut socket: WebSocket, claims: ApiKeyClaims) {
    while let Some(msg) = socket.recv().await {
        if let Ok(Message::Text(text)) = msg {
            if let Ok(cmd) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(topics) = cmd["subscribe"].as_array() {
                    let allowed: Vec<_> = topics.iter()
                        .filter_map(|t| t.as_str())
                        .filter(|t| topic_allowed(t, &claims))
                        .collect();
                    let _ = socket.send(Message::Text(
                        serde_json::json!({ "type": "subscribed", "topics": allowed }).to_string()
                    )).await;
                }
            }
        }
    }
}
