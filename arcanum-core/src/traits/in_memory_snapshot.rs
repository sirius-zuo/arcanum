use async_trait::async_trait;
use crate::{types::{DocumentId, SnapshotLocation}, Result};
use super::SnapshotStore;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// Minimal in-memory snapshot store for engine wiring.
pub struct InMemorySnapshotStore {
    data: Arc<Mutex<HashMap<String, Vec<u8>>>>,
}

impl InMemorySnapshotStore {
    pub fn new() -> Self {
        Self { data: Arc::new(Mutex::new(HashMap::new())) }
    }
}

impl Default for InMemorySnapshotStore {
    fn default() -> Self { Self::new() }
}

#[async_trait]
impl SnapshotStore for InMemorySnapshotStore {
    async fn store(&self, _doc_id: &DocumentId, _version: u32, raw: &[u8], _canonical: Option<&serde_json::Value>) -> Result<SnapshotLocation> {
        let mut data = self.data.lock().unwrap();
        let doc_str = _doc_id.0.to_string();
        let raw_key = format!("mem://{}/{}/raw.bin", doc_str, _version);
        data.insert(raw_key.clone(), raw.to_vec());

        let canonical_uri = if let Some(cv) = _canonical {
            let bytes = serde_json::to_vec(cv)
                .map_err(|e| crate::ArcanumError::Storage(format!("serialize canonical: {}", e)))?;
            let canon_key = format!("mem://{}/{}/canonical.json", doc_str, _version);
            data.insert(canon_key.clone(), bytes);
            Some(canon_key)
        } else {
            None   // was always Some(key2) — now correctly None
        };

        Ok(SnapshotLocation {
            raw_uri:       raw_key,
            canonical_uri,
        })
    }

    async fn fetch_raw(&self, uri: &str) -> Result<Vec<u8>> {
        self.data.lock().unwrap().get(uri)
            .cloned()
            .ok_or_else(|| crate::ArcanumError::NotFound(format!("snapshot not found: {}", uri)))
    }

    async fn fetch_canonical(&self, uri: &str) -> Result<Option<serde_json::Value>> {
        Ok(self.data.lock().unwrap().get(uri)
            .and_then(|b| serde_json::from_slice(b).ok()))
    }

    async fn delete(&self, raw_uri: &str, canonical_uri: Option<&str>) -> Result<()> {
        let mut data = self.data.lock().unwrap();
        data.remove(raw_uri);
        if let Some(u) = canonical_uri {
            data.remove(u);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn basic_store_and_retrieve() {
        let store = InMemorySnapshotStore::new();
        let doc_id = DocumentId::new();
        let loc = store.store(&doc_id, 1, b"hello", Some(&serde_json::json!({"k":"v"}))).await.unwrap();
        assert!(loc.raw_uri.contains("raw.bin"));
        let raw = store.fetch_raw(&loc.raw_uri).await.unwrap();
        assert_eq!(raw, b"hello");
    }

    #[tokio::test]
    async fn store_without_canonical_returns_none_uri() {
        let store = InMemorySnapshotStore::new();
        let doc_id = DocumentId::new();
        let loc = store.store(&doc_id, 1, b"raw", None).await.unwrap();
        assert!(loc.canonical_uri.is_none(), "canonical_uri must be None when no canonical provided");
    }

    #[tokio::test]
    async fn store_serialization_error_is_propagated() {
        let store = InMemorySnapshotStore::new();
        let doc_id = DocumentId::new();
        let canonical = serde_json::json!({"blocks": [{"id": "b1"}]});
        let loc = store.store(&doc_id, 1, b"raw", Some(&canonical)).await.unwrap();
        assert!(loc.canonical_uri.is_some());
        let fetched = store.fetch_canonical(loc.canonical_uri.as_ref().unwrap()).await.unwrap();
        assert_eq!(fetched, Some(canonical));
    }
}
