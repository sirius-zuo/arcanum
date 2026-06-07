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
    let (claims, eng) = match extract_and_validate_ws_token(&headers, &engine) {
        Ok(pair) => pair,
        Err(status) => return status.into_response(),
    };
    ws.protocols(["arcanum-v1"])
        .on_upgrade(move |socket| handle_socket(socket, claims, eng))
}

fn extract_and_validate_ws_token(
    headers: &HeaderMap,
    engine: &Option<Arc<ArcanumEngine>>,
) -> Result<(ApiKeyClaims, Arc<ArcanumEngine>), StatusCode> {
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

    let claims = engine.auth.validate_api_key(&token)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
    Ok((claims, engine.clone()))
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

async fn handle_socket(mut socket: WebSocket, claims: ApiKeyClaims, engine: Arc<ArcanumEngine>) {
    // Subscribe to all EventBus events via a wildcard topic subscription.
    // We use a single broadcast receiver per connection.
    let mut event_rx = engine.events.subscribe("ingestion:progress").await;

    loop {
        tokio::select! {
            // Forward engine events to the client.
            event = event_rx.recv() => {
                match event {
                    Ok(payload) => {
                        let msg = serde_json::json!({ "type": "event", "payload": payload });
                        if socket.send(Message::Text(msg.to_string())).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => {
                        // Broadcast channel lagged or closed — reconnect or exit.
                        break;
                    }
                }
            }
            // Handle incoming client messages.
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(cmd) = serde_json::from_str::<serde_json::Value>(&text) {
                            if let Some(topics) = cmd["subscribe"].as_array() {
                                let allowed: Vec<_> = topics.iter()
                                    .filter_map(|t| t.as_str())
                                    .filter(|t| topic_allowed(t, &claims))
                                    .collect();
                                let response = serde_json::json!({ "type": "subscribed", "topics": allowed });
                                if socket.send(Message::Text(response.to_string())).await.is_err() {
                                    break;
                                }
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}
