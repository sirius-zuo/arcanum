use arcanum_core::{
    traits::SnapshotStore,
    types::{DocumentId, SnapshotLocation},
    ArcanumError, Result,
};
use async_trait::async_trait;
use std::path::PathBuf;
use tracing::instrument;

/// Filesystem-backed SnapshotStore.
///
/// Layout under `root`:
/// ```text
/// <root>/
///   <doc_id>/<version>/
///     raw.bin          — original document bytes
///     canonical.json   — Docling canonical JSON (optional)
/// ```
pub struct LocalSnapshotStore {
    root: PathBuf,
}

impl LocalSnapshotStore {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Ensure the root directory exists.
    async fn ensure_root(&self) -> Result<()> {
        tokio::fs::create_dir_all(&self.root)
            .await
            .map_err(|e| ArcanumError::Storage(format!("create snapshot root: {e}")))
    }

    fn doc_dir(&self, doc_id: &DocumentId, version: u32) -> PathBuf {
        self.root.join(doc_id.0.to_string()).join(version.to_string())
    }

    fn raw_path(&self, doc_id: &DocumentId, version: u32) -> PathBuf {
        self.doc_dir(doc_id, version).join("raw.bin")
    }

    fn canonical_path(&self, doc_id: &DocumentId, version: u32) -> PathBuf {
        self.doc_dir(doc_id, version).join("canonical.json")
    }
}

#[async_trait]
impl SnapshotStore for LocalSnapshotStore {
    #[instrument(skip(self), fields(store = "local_snapshot", doc_id = %doc_id.0, version), err)]
    async fn store(
        &self,
        doc_id:      &DocumentId,
        version:     u32,
        raw:         &[u8],
        canonical:   Option<&serde_json::Value>,
    ) -> Result<SnapshotLocation> {
        self.ensure_root().await?;

        let dir = self.doc_dir(doc_id, version);
        tokio::fs::create_dir_all(&dir)
            .await
            .map_err(|e| ArcanumError::Storage(format!("create snapshot dir: {e}")))?;

        // Write raw bytes.
        tokio::fs::write(self.raw_path(doc_id, version), raw)
            .await
            .map_err(|e| ArcanumError::Storage(format!("write raw snapshot: {e}")))?;

        let raw_uri = format!("file://{}", self.raw_path(doc_id, version).to_string_lossy());

        // Write canonical sidecar if provided.
        let canonical_uri = if let Some(cv) = canonical {
            let path = self.canonical_path(doc_id, version);
            let data = serde_json::to_vec_pretty(&cv)
                .map_err(|e| ArcanumError::Storage(format!("serialize canonical: {e}")))?;
            tokio::fs::write(&path, data)
                .await
                .map_err(|e| ArcanumError::Storage(format!("write canonical snapshot: {e}")))?;
            Some(format!("file://{}", path.to_string_lossy()))
        } else {
            None
        };

        Ok(SnapshotLocation {
            raw_uri,
            canonical_uri,
        })
    }

    #[instrument(skip(self), fields(store = "local_snapshot", uri), err)]
    async fn fetch_raw(&self, uri: &str) -> Result<Vec<u8>> {
        let path = uri
            .strip_prefix("file://")
            .ok_or_else(|| ArcanumError::NotFound(format!("unsupported snapshot URI scheme: {uri}")))?;
        tokio::fs::read(path)
            .await
            .map_err(|e| ArcanumError::NotFound(format!("snapshot not found: {uri}: {e}")))
    }

    #[instrument(skip(self), fields(store = "local_snapshot", uri), err)]
    async fn fetch_canonical(&self, uri: &str) -> Result<Option<serde_json::Value>> {
        let path = uri
            .strip_prefix("file://")
            .ok_or_else(|| ArcanumError::NotFound(format!("unsupported snapshot URI scheme: {uri}")))?;
        let bytes = tokio::fs::read(path)
            .await
            .map_err(|e| ArcanumError::NotFound(format!("canonical snapshot not found: {uri}: {e}")))?;
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|e| ArcanumError::Storage(format!("parse canonical snapshot: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn make_store() -> (LocalSnapshotStore, TempDir) {
        let tmp = TempDir::new().unwrap();
        let store = LocalSnapshotStore::new(tmp.path());
        (store, tmp)
    }

    #[tokio::test]
    async fn test_store_and_fetch_raw() {
        let (store, _tmp) = make_store();
        let doc_id = DocumentId::new();
        let location = store
            .store(&doc_id, 1, b"raw document bytes", None)
            .await
            .unwrap();
        assert!(location.raw_uri.starts_with("file://"));
        let raw = store.fetch_raw(&location.raw_uri).await.unwrap();
        assert_eq!(raw, b"raw document bytes");
    }

    #[tokio::test]
    async fn test_store_and_fetch_canonical() {
        let (store, _tmp) = make_store();
        let doc_id = DocumentId::new();
        let canonical = serde_json::json!({"blocks": [{"id": "b1"}]});
        let location = store
            .store(&doc_id, 1, b"raw", Some(&canonical))
            .await
            .unwrap();
        let fetched = store.fetch_canonical(location.canonical_uri.as_ref().unwrap()).await.unwrap();
        assert_eq!(fetched, Some(canonical));
    }

    #[tokio::test]
    async fn test_store_no_canonical_produces_none() {
        let (store, _tmp) = make_store();
        let doc_id = DocumentId::new();
        let location = store.store(&doc_id, 1, b"raw", None).await.unwrap();
        assert!(location.canonical_uri.is_none());
    }

    #[tokio::test]
    async fn test_fetch_missing_raw_returns_error() {
        let (store, _tmp) = make_store();
        let err = store.fetch_raw("file://nonexistent/path.bin").await.unwrap_err();
        assert!(err.to_string().contains("snapshot not found") || err.to_string().contains("No such file"));
    }

    #[tokio::test]
    async fn test_concurrent_stores_do_not_interfere() {
        let (store, _tmp) = make_store();
        let store = Arc::new(store);
        let doc_id_a = DocumentId::new();
        let doc_id_b = DocumentId::new();

        let s1 = store.clone();
        let s2 = store.clone();
        let da = doc_id_a.clone();
        let db = doc_id_b.clone();

        let (loc_a, loc_b) = tokio::join!(
            async move { s1.store(&da, 1, b"content-a", None).await.unwrap() },
            async move { s2.store(&db, 1, b"content-b", None).await.unwrap() },
        );

        let raw_a = store.fetch_raw(&loc_a.raw_uri).await.unwrap();
        let raw_b = store.fetch_raw(&loc_b.raw_uri).await.unwrap();
        assert_eq!(raw_a, b"content-a");
        assert_eq!(raw_b, b"content-b");
    }
}
