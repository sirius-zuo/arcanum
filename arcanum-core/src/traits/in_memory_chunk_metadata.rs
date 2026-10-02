use crate::{
    traits::ChunkMetadataStore,
    types::{ChunkId, ChunkMetadataRecord, DocumentId},
    Result,
};
use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::Mutex;

pub struct InMemoryChunkMetadataStore {
    data: Mutex<HashMap<ChunkId, ChunkMetadataRecord>>,
}

impl InMemoryChunkMetadataStore {
    pub fn new() -> Self {
        Self {
            data: Mutex::new(HashMap::new()),
        }
    }

    pub async fn get_all(&self) -> Vec<ChunkMetadataRecord> {
        self.data.lock().await.values().cloned().collect()
    }
}

impl Default for InMemoryChunkMetadataStore {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ChunkMetadataStore for InMemoryChunkMetadataStore {
    async fn put(&self, record: &ChunkMetadataRecord) -> Result<()> {
        self.data
            .lock()
            .await
            .insert(record.chunk_id.clone(), record.clone());
        Ok(())
    }

    async fn get(&self, chunk_id: &ChunkId) -> Result<Option<ChunkMetadataRecord>> {
        Ok(self.data.lock().await.get(chunk_id).cloned())
    }

    async fn get_many(&self, ids: &[ChunkId]) -> Result<Vec<ChunkMetadataRecord>> {
        let data = self.data.lock().await;
        Ok(ids.iter().filter_map(|id| data.get(id).cloned()).collect())
    }

    async fn delete_by_source_uri(&self, _collection_id: &str, _source_uri: &str) -> Result<()> {
        let mut data = self.data.lock().await;
        let ids_to_remove: Vec<ChunkId> = data
            .iter()
            .filter(|(_, r)| r.source_uri == _source_uri && r.collection_id == _collection_id)
            .map(|(k, _)| k.clone())
            .collect();
        for id in ids_to_remove {
            data.remove(&id);
        }
        Ok(())
    }

    async fn delete_by_document_version(
        &self,
        document_id: &DocumentId,
        version_num: u32,
    ) -> Result<Vec<ChunkId>> {
        let mut data = self.data.lock().await;
        let ids_to_remove: Vec<ChunkId> = data
            .iter()
            .filter(|(_, r)| r.document_id == *document_id && r.version_num == version_num)
            .map(|(k, _)| k.clone())
            .collect();
        for id in &ids_to_remove {
            data.remove(id);
        }
        Ok(ids_to_remove)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ChunkBackend;
    use chrono::Utc;

    #[tokio::test]
    async fn test_put_and_get() {
        let store = InMemoryChunkMetadataStore::new();
        let record = ChunkMetadataRecord {
            chunk_id: ChunkId::new(),
            document_id: DocumentId::new(),
            collection_id: "col".into(),
            version_num: 1,
            backend: ChunkBackend::Vector,
            text: String::new(),
            chunk_index: 0,
            source_uri: "file://test.txt".into(),
            snapshot_uri: "file:///snap/test/1.raw".into(),
            canonical_uri: None,
            page: Some(1),
            section: Some("§1".into()),
            block_ids: vec!["b1".into()],
            offset_start: 0,
            offset_end: 100,
            ingested_at: Utc::now(),
        };
        let id = record.chunk_id.clone();
        store.put(&record).await.unwrap();
        let found = store.get(&id).await.unwrap().unwrap();
        assert_eq!(found.source_uri, "file://test.txt");
        assert_eq!(found.page, Some(1));
        assert_eq!(found.block_ids, vec!["b1"]);
    }

    #[tokio::test]
    async fn test_delete_by_source_uri() {
        let store = InMemoryChunkMetadataStore::new();
        let record = ChunkMetadataRecord {
            chunk_id: ChunkId::new(),
            document_id: DocumentId::new(),
            collection_id: "col".into(),
            version_num: 1,
            backend: ChunkBackend::Vector,
            text: String::new(),
            chunk_index: 0,
            source_uri: "file://delete_me.txt".into(),
            snapshot_uri: "file:///snap/d/1.raw".into(),
            canonical_uri: None,
            page: None,
            section: None,
            block_ids: vec![],
            offset_start: 0,
            offset_end: 50,
            ingested_at: Utc::now(),
        };
        let id = record.chunk_id.clone();
        store.put(&record).await.unwrap();
        store
            .delete_by_source_uri("col", "file://delete_me.txt")
            .await
            .unwrap();
        assert!(store.get(&id).await.unwrap().is_none());
    }

    fn rec(backend: ChunkBackend, text: &str, idx: usize) -> ChunkMetadataRecord {
        ChunkMetadataRecord {
            chunk_id: ChunkId::new(),
            document_id: DocumentId::new(),
            collection_id: "col".into(),
            version_num: 1,
            backend,
            text: text.into(),
            chunk_index: idx,
            source_uri: "file://x.txt".into(),
            snapshot_uri: "file:///snap/x/1.raw".into(),
            canonical_uri: None,
            page: None,
            section: None,
            block_ids: vec![],
            offset_start: 0,
            offset_end: text.len(),
            ingested_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn get_many_returns_present_ids_and_skips_missing() {
        let store = InMemoryChunkMetadataStore::new();
        let a = rec(ChunkBackend::Vector, "aaa", 0);
        let b = rec(ChunkBackend::Lexical, "bbb", 1);
        store.put(&a).await.unwrap();
        store.put(&b).await.unwrap();
        let got = store
            .get_many(&[a.chunk_id.clone(), ChunkId::new(), b.chunk_id.clone()])
            .await
            .unwrap();
        let mut ids: Vec<_> = got.iter().map(|r| r.chunk_id.0).collect();
        ids.sort();
        let mut want = vec![a.chunk_id.0, b.chunk_id.0];
        want.sort();
        assert_eq!(ids, want);
    }

    #[tokio::test]
    async fn record_round_trips_backend_text_and_index() {
        let store = InMemoryChunkMetadataStore::new();
        let r = rec(ChunkBackend::Graph, "graph text", 5);
        store.put(&r).await.unwrap();
        let found = store.get(&r.chunk_id).await.unwrap().unwrap();
        assert_eq!(found.backend, ChunkBackend::Graph);
        assert_eq!(found.text, "graph text");
        assert_eq!(found.chunk_index, 5);
    }

    #[tokio::test]
    async fn test_get_missing() {
        let store = InMemoryChunkMetadataStore::new();
        assert!(store.get(&ChunkId::new()).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_delete_by_document_version_only_removes_matching_version() {
        let store = InMemoryChunkMetadataStore::new();
        let doc_id = DocumentId::new();

        let mut v1 = ChunkMetadataRecord {
            chunk_id: ChunkId::new(),
            document_id: doc_id.clone(),
            collection_id: "col".into(),
            version_num: 1,
            backend: ChunkBackend::Vector,
            text: String::new(),
            chunk_index: 0,
            source_uri: "file://doc.pdf".into(),
            snapshot_uri: "file:///snap/d/1.raw".into(),
            canonical_uri: None,
            page: None,
            section: None,
            block_ids: vec![],
            offset_start: 0,
            offset_end: 10,
            ingested_at: Utc::now(),
        };
        let mut v2 = v1.clone();
        v2.chunk_id = ChunkId::new();
        v2.version_num = 2;
        v2.snapshot_uri = "file:///snap/d/2.raw".into();
        v1.chunk_id = ChunkId::new();

        store.put(&v1).await.unwrap();
        store.put(&v2).await.unwrap();

        let removed = store.delete_by_document_version(&doc_id, 1).await.unwrap();
        assert_eq!(removed, vec![v1.chunk_id.clone()]);
        assert!(store.get(&v1.chunk_id).await.unwrap().is_none());
        assert!(store.get(&v2.chunk_id).await.unwrap().is_some());
    }
}
