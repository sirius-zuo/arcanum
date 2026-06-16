use crate::{
    traits::ChunkMetadataStore,
    types::{ChunkId, ChunkMetadataRecord, DocumentId},
    ArcanumError, Result,
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
        self.data.lock().await.insert(record.chunk_id.clone(), record.clone());
        Ok(())
    }

    async fn get(&self, chunk_id: &ChunkId) -> Result<Option<ChunkMetadataRecord>> {
        Ok(self.data.lock().await.get(chunk_id).cloned())
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    #[tokio::test]
    async fn test_put_and_get() {
        let store = InMemoryChunkMetadataStore::new();
        let record = ChunkMetadataRecord {
            chunk_id:      ChunkId::new(),
            document_id:   DocumentId::new(),
            collection_id: "col".into(),
            version_num:   1,
            source_uri:    "file://test.txt".into(),
            snapshot_uri:  "file:///snap/test/1.raw".into(),
            canonical_uri: None,
            page:          Some(1),
            section:       Some("§1".into()),
            block_ids:     vec!["b1".into()],
            offset_start:  0,
            offset_end:    100,
            ingested_at:   Utc::now(),
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
            chunk_id:      ChunkId::new(),
            document_id:   DocumentId::new(),
            collection_id: "col".into(),
            version_num:   1,
            source_uri:    "file://delete_me.txt".into(),
            snapshot_uri:  "file:///snap/d/1.raw".into(),
            canonical_uri: None,
            page:          None,
            section:       None,
            block_ids:     vec![],
            offset_start:  0,
            offset_end:    50,
            ingested_at:   Utc::now(),
        };
        let id = record.chunk_id.clone();
        store.put(&record).await.unwrap();
        store.delete_by_source_uri("col", "file://delete_me.txt").await.unwrap();
        assert!(store.get(&id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_get_missing() {
        let store = InMemoryChunkMetadataStore::new();
        assert!(store.get(&ChunkId::new()).await.unwrap().is_none());
    }
}
