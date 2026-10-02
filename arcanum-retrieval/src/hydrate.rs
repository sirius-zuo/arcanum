use arcanum_core::{
    traits::ChunkMetadataStore,
    types::{
        ChunkId, ChunkKind, ChunkMetadataRecord, IndexedChunk, RetrievalStrategy, RetrievedChunk,
        Vector,
    },
    Result,
};
use std::collections::HashMap;
use tracing::warn;

fn strategy_label(strategy: &RetrievalStrategy) -> &'static str {
    match strategy {
        RetrievalStrategy::Vector => "vector",
        RetrievalStrategy::Bm25 => "bm25",
        RetrievalStrategy::ColBert => "colbert",
        RetrievalStrategy::Raptor => "raptor",
        RetrievalStrategy::Graph => "graph",
    }
}

/// Fetches registry records for `ids` with a single `get_many`. Ids absent from
/// the registry are logged and counted, then left out of the result.
pub async fn fetch_records(
    store: &dyn ChunkMetadataStore,
    ids: &[ChunkId],
    collection: &str,
    strategy: &RetrievalStrategy,
) -> Result<HashMap<ChunkId, ChunkMetadataRecord>> {
    let records: HashMap<ChunkId, ChunkMetadataRecord> = store
        .get_many(ids)
        .await?
        .into_iter()
        .map(|r| (r.chunk_id.clone(), r))
        .collect();
    let label = strategy_label(strategy);
    for id in ids {
        if !records.contains_key(id) {
            warn!(
                collection,
                strategy = label,
                chunk_id = %id.0,
                "chunk not in registry"
            );
            metrics::counter!("arcanum_retrieval_unresolved_chunks_total", "strategy" => label)
                .increment(1);
        }
    }
    Ok(records)
}

/// Resolves scored chunk ids into `RetrievedChunk`s through the registry,
/// preserving `hits` order and skipping ids the registry does not know.
pub async fn hydrate(
    store: &dyn ChunkMetadataStore,
    hits: &[(ChunkId, f32)],
    collection: &str,
    strategy: RetrievalStrategy,
) -> Result<Vec<RetrievedChunk>> {
    let ids: Vec<ChunkId> = hits.iter().map(|(id, _)| id.clone()).collect();
    let records = fetch_records(store, &ids, collection, &strategy).await?;
    Ok(hits
        .iter()
        .filter_map(|(id, score)| {
            let chunk = records.get(id)?.to_chunk();
            Some(RetrievedChunk {
                indexed_chunk: IndexedChunk {
                    chunk,
                    vector: Vector(vec![]),
                    token_vectors: None,
                    store_id: id.0.to_string(),
                },
                score: *score,
                strategy: strategy.clone(),
                kind: ChunkKind::Source,
            })
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::traits::InMemoryChunkMetadataStore;
    use arcanum_core::types::{ChunkBackend, DocumentId};
    use async_trait::async_trait;
    use chrono::Utc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn rec(text: &str, idx: usize) -> ChunkMetadataRecord {
        ChunkMetadataRecord {
            chunk_id: ChunkId::new(),
            document_id: DocumentId::new(),
            collection_id: "col".into(),
            version_num: 1,
            backend: ChunkBackend::Lexical,
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

    struct CountingStore {
        inner: InMemoryChunkMetadataStore,
        get_many_calls: AtomicUsize,
    }

    #[async_trait]
    impl ChunkMetadataStore for CountingStore {
        async fn put(&self, record: &ChunkMetadataRecord) -> Result<()> {
            self.inner.put(record).await
        }
        async fn get(&self, id: &ChunkId) -> Result<Option<ChunkMetadataRecord>> {
            self.inner.get(id).await
        }
        async fn get_many(&self, ids: &[ChunkId]) -> Result<Vec<ChunkMetadataRecord>> {
            self.get_many_calls.fetch_add(1, Ordering::SeqCst);
            self.inner.get_many(ids).await
        }
        async fn delete_by_source_uri(&self, c: &str, s: &str) -> Result<()> {
            self.inner.delete_by_source_uri(c, s).await
        }
        async fn delete_by_document_version(&self, d: &DocumentId, v: u32) -> Result<Vec<ChunkId>> {
            self.inner.delete_by_document_version(d, v).await
        }
    }

    #[tokio::test]
    async fn hydrate_preserves_order_and_scores() {
        let store = InMemoryChunkMetadataStore::new();
        let (a, b, c) = (rec("aaa", 0), rec("bbb", 1), rec("ccc", 2));
        for r in [&a, &b, &c] {
            store.put(r).await.unwrap();
        }
        let hits = vec![
            (c.chunk_id.clone(), 0.9),
            (a.chunk_id.clone(), 0.5),
            (b.chunk_id.clone(), 0.1),
        ];
        let out = hydrate(&store, &hits, "col", RetrievalStrategy::Bm25)
            .await
            .unwrap();
        let ids: Vec<_> = out
            .iter()
            .map(|r| r.indexed_chunk.chunk.id.clone())
            .collect();
        assert_eq!(ids, vec![c.chunk_id, a.chunk_id, b.chunk_id]);
        let scores: Vec<_> = out.iter().map(|r| r.score).collect();
        assert_eq!(scores, vec![0.9, 0.5, 0.1]);
        assert_eq!(out[0].indexed_chunk.chunk.text, "ccc");
        assert_eq!(out[0].indexed_chunk.chunk.position.index, 2);
        assert_eq!(out[0].strategy, RetrievalStrategy::Bm25);
        assert_eq!(out[0].kind, ChunkKind::Source);
        assert!(out[0].indexed_chunk.vector.0.is_empty());
        assert_eq!(
            out[0].indexed_chunk.store_id,
            out[0].indexed_chunk.chunk.id.0.to_string()
        );
    }

    #[tokio::test]
    async fn hydrate_skips_missing_ids_without_error() {
        let store = InMemoryChunkMetadataStore::new();
        let a = rec("aaa", 0);
        store.put(&a).await.unwrap();
        let hits = vec![
            (ChunkId::new(), 0.9),
            (a.chunk_id.clone(), 0.5),
            (ChunkId::new(), 0.1),
        ];
        let out = hydrate(&store, &hits, "col", RetrievalStrategy::Graph)
            .await
            .unwrap();
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].indexed_chunk.chunk.id, a.chunk_id);
    }

    #[tokio::test]
    async fn fetch_records_calls_get_many_once() {
        let store = CountingStore {
            inner: InMemoryChunkMetadataStore::new(),
            get_many_calls: AtomicUsize::new(0),
        };
        let (a, b) = (rec("aaa", 0), rec("bbb", 1));
        store.put(&a).await.unwrap();
        store.put(&b).await.unwrap();
        let ids = vec![a.chunk_id.clone(), ChunkId::new(), b.chunk_id.clone()];
        let got = fetch_records(&store, &ids, "col", &RetrievalStrategy::Raptor)
            .await
            .unwrap();
        assert_eq!(store.get_many_calls.load(Ordering::SeqCst), 1);
        assert_eq!(got.len(), 2);
        assert!(got.contains_key(&a.chunk_id) && got.contains_key(&b.chunk_id));
    }
}
