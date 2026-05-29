use axum::{
    extract::{ws::{WebSocket, WebSocketUpgrade, Message}, State},
    response::IntoResponse,
};
use std::sync::Arc;
use arcanum_engine::ArcanumEngine;

pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(_engine): State<Option<Arc<ArcanumEngine>>>,
) -> impl IntoResponse {
    ws.on_upgrade(handle_socket)
}

async fn handle_socket(mut socket: WebSocket) {
    while let Some(msg) = socket.recv().await {
        if let Ok(Message::Text(text)) = msg {
            if let Ok(cmd) = serde_json::from_str::<serde_json::Value>(&text) {
                if let Some(topics) = cmd["subscribe"].as_array() {
                    let _ = socket.send(Message::Text(
                        serde_json::json!({ "type": "subscribed", "topics": topics }).to_string()
                    )).await;
                }
            }
        }
    }
}
