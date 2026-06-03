use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;
use chrono::Utc;
use tracing::instrument;

#[derive(Debug, Clone)]
pub struct McpSession {
    pub id: String,
    pub client_info: String,
    pub created_at: String,
    pub closed: bool,
}

impl McpSession {
    pub fn new(client_info: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            client_info: client_info.into(),
            created_at: Utc::now().to_rfc3339(),
            closed: false,
        }
    }
}

#[derive(Debug)]
pub struct SessionManager {
    sessions: Arc<RwLock<HashMap<String, McpSession>>>,
}

impl SessionManager {
    pub fn new() -> Self {
        Self { sessions: Arc::new(RwLock::new(HashMap::new())) }
    }

    #[instrument(skip(self, client_info), fields(client = tracing::field::Empty))]
    pub async fn create(&self, client_info: impl Into<String>) -> McpSession {
        let info = client_info.into();
        let session = McpSession::new(info);
        self.sessions.write().await.insert(session.id.clone(), session.clone());
        tracing::Span::current().record("client", &session.client_info);
        session
    }

    #[instrument(skip(self), fields(session_id = id))]
    pub async fn get(&self, id: &str) -> Option<McpSession> {
        self.sessions.read().await.get(id).cloned()
    }

    #[instrument(skip(self), fields(session_id = id))]
    pub async fn close(&self, id: &str) -> bool {
        let mut sessions = self.sessions.write().await;
        if let Some(s) = sessions.get_mut(id) {
            s.closed = true;
            true
        } else {
            false
        }
    }

    #[instrument(skip(self))]
    pub async fn active_count(&self) -> usize {
        self.sessions.read().await.values().filter(|s| !s.closed).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_create_and_retrieve() {
        let mgr = SessionManager::new();
        let session = mgr.create("test-client").await;
        let retrieved = mgr.get(&session.id).await;
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().client_info, "test-client");
    }

    #[tokio::test]
    async fn test_session_close() {
        let mgr = SessionManager::new();
        let session = mgr.create("client").await;
        assert_eq!(mgr.active_count().await, 1);
        let closed = mgr.close(&session.id).await;
        assert!(closed);
        assert_eq!(mgr.active_count().await, 0);
        let s = mgr.get(&session.id).await.unwrap();
        assert!(s.closed);
    }

    #[tokio::test]
    async fn test_session_get_nonexistent() {
        let mgr = SessionManager::new();
        assert!(mgr.get("no-such-id").await.is_none());
    }
}
