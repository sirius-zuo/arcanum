use arcanum_core::{
    traits::{ChunkMetadataStore, DocumentVersionStore, GraphStore, TreeStore},
    types::{
        ChunkId, EntityId, EvidenceKind, ProofChain, ProofNode, RawSourceRef, TreeNodeId,
    },
    ArcanumError, Result,
};
use async_trait::async_trait;
use std::sync::Arc;
use tracing::instrument;

pub struct DefaultEvidenceResolver {
    pub chunk_metadata: Arc<dyn ChunkMetadataStore>,
    pub version_store:  Arc<dyn DocumentVersionStore>,
    pub tree_store:     Arc<dyn TreeStore>,
    pub graph_store:    Arc<dyn GraphStore>,
}

impl DefaultEvidenceResolver {
    pub fn new(
        chunk_metadata: Arc<dyn ChunkMetadataStore>,
        version_store:  Arc<dyn DocumentVersionStore>,
        tree_store:     Arc<dyn TreeStore>,
        graph_store:    Arc<dyn GraphStore>,
    ) -> Self {
        Self { chunk_metadata, version_store, tree_store, graph_store }
    }

    async fn resolve_chunk_inner(&self, chunk_id: &ChunkId) -> Result<(ProofNode, RawSourceRef)> {
        let meta = self.chunk_metadata.get(chunk_id).await?
            .ok_or_else(|| ArcanumError::NotFound(format!("chunk metadata not found: {}", chunk_id.0)))?;

        // Cross-check the version still exists (confirms no GC race).
        // If the version store returns None (e.g. NoOp stub), skip the check.
        if let Some(version) = self.version_store.get_version(&meta.document_id, meta.version_num).await? {
            if version.status != arcanum_core::types::VersionStatus::Active {
                tracing::warn!(
                    chunk_id = %chunk_id.0,
                    version = %meta.version_num,
                    status = ?version.status,
                    "chunk metadata references a non-active version"
                );
            }
        } else {
            tracing::warn!(
                chunk_id = %chunk_id.0,
                document_id = %meta.document_id.0,
                version = %meta.version_num,
                "chunk metadata references a version not found in the version store"
            );
        }

        let label = format!(
            "{} p.{} §{}",
            meta.source_uri,
            meta.page.map(|p| p.to_string()).unwrap_or_else(|| "-".into()),
            meta.section.as_deref().unwrap_or("-"),
        );

        let node = ProofNode {
            id:       chunk_id.0.to_string(),
            kind:     EvidenceKind::Chunk,
            label,
            metadata: serde_json::json!({
                "page":    meta.page,
                "section": meta.section,
            }),
            children: vec![],
        };

        let raw = RawSourceRef {
            document_id:   meta.document_id,
            version_num:   meta.version_num,
            source_uri:    meta.source_uri,
            snapshot_uri:  meta.snapshot_uri,
            canonical_uri: meta.canonical_uri,
            page:          meta.page,
            section:       meta.section,
            block_ids:     meta.block_ids,
            offset_start:  meta.offset_start,
            offset_end:    meta.offset_end,
        };

        Ok((node, raw))
    }
}

#[async_trait]
impl arcanum_core::traits::EvidenceResolver for DefaultEvidenceResolver {
    #[instrument(skip(self), fields(chunk_id = %chunk_id.0), err)]
    async fn resolve_chunk(&self, chunk_id: &ChunkId) -> Result<ProofChain> {
        let (root, raw) = self.resolve_chunk_inner(chunk_id).await?;
        Ok(ProofChain { root, raw_sources: vec![raw] })
    }

    #[instrument(skip(self), fields(node_id = %node_id.0), err)]
    async fn resolve_tree_node(&self, node_id: &TreeNodeId) -> Result<ProofChain> {
        let node = self.tree_store.get_by_id(node_id).await?
            .ok_or_else(|| ArcanumError::NotFound(format!("tree node not found: {}", node_id.0)))?;

        let mut chunk_nodes = Vec::new();
        let mut raw_sources = Vec::new();
        for cid in &node.leaf_chunk_ids {
            match self.resolve_chunk_inner(cid).await {
                Ok((cn, rs)) => { chunk_nodes.push(cn); raw_sources.push(rs); }
                Err(e) => tracing::warn!(chunk_id = %cid.0, "chunk resolution failed: {}", e),
            }
        }

        raw_sources.dedup_by_key(|r| r.snapshot_uri.clone());

        Ok(ProofChain {
            root: ProofNode {
                id:       node_id.0.to_string(),
                kind:     EvidenceKind::TreeNode,
                label:    node.text.chars().take(100).collect(),
                metadata: serde_json::json!({ "level": node.level, "source_uri": node.source_uri }),
                children: chunk_nodes,
            },
            raw_sources,
        })
    }

    #[instrument(skip(self), fields(entity_id = %entity_id.0), err)]
    async fn resolve_entity(&self, entity_id: &EntityId) -> Result<ProofChain> {
        let entity = self.graph_store.get_entity_by_id(entity_id).await?
            .ok_or_else(|| ArcanumError::NotFound(format!("entity not found: {}", entity_id.0)))?;

        let mut chunk_nodes = Vec::new();
        let mut raw_sources = Vec::new();
        for cid in &entity.source_chunks {
            match self.resolve_chunk_inner(cid).await {
                Ok((cn, rs)) => { chunk_nodes.push(cn); raw_sources.push(rs); }
                Err(e) => tracing::warn!(chunk_id = %cid.0, "chunk resolution failed: {}", e),
            }
        }
        raw_sources.dedup_by_key(|r| r.snapshot_uri.clone());

        Ok(ProofChain {
            root: ProofNode {
                id:       entity_id.0.to_string(),
                kind:     EvidenceKind::Entity,
                label:    entity.name.clone(),
                metadata: serde_json::json!({
                    "entity_type":  entity.entity_type,
                    "collection_id": entity.collection_id,
                }),
                children: chunk_nodes,
            },
            raw_sources,
        })
    }

    #[instrument(skip(self, relation_type), fields(source_id = %source_id.0, relation_type, target_id = %target_id.0), err)]
    async fn resolve_relation(
        &self,
        source_id:     &EntityId,
        relation_type: &str,
        target_id:     &EntityId,
    ) -> Result<ProofChain> {
        let relation = self.graph_store.get_relation(source_id, relation_type, target_id).await?
            .ok_or_else(|| ArcanumError::NotFound(
                format!("relation {}/{}/{} not found", source_id.0, relation_type, target_id.0)
            ))?;

        let mut chunk_nodes = Vec::new();
        let mut raw_sources = Vec::new();
        for cid in &relation.source_chunks {
            match self.resolve_chunk_inner(cid).await {
                Ok((cn, rs)) => { chunk_nodes.push(cn); raw_sources.push(rs); }
                Err(e) => tracing::warn!(chunk_id = %cid.0, "chunk resolution failed: {}", e),
            }
        }
        raw_sources.dedup_by_key(|r| r.snapshot_uri.clone());

        Ok(ProofChain {
            root: ProofNode {
                id:       format!("{}/{}/{}", source_id.0, relation_type, target_id.0),
                kind:     EvidenceKind::Relation,
                label:    format!("{} --[{}]--> {}", source_id.0, relation_type, target_id.0),
                metadata: serde_json::json!({ "confidence": relation.confidence }),
                children: chunk_nodes,
            },
            raw_sources,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::{
        traits::{ChunkMetadataStore, EvidenceResolver, NoOpDocumentVersionStore},
        types::*,
    };
    use chrono::Utc;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex as StdMutex};
    use arcanum_core::traits::TreeStore;

    #[derive(Default)]
    struct FakeMetaStore(StdMutex<HashMap<String, ChunkMetadataRecord>>);

    #[async_trait::async_trait]
    impl ChunkMetadataStore for FakeMetaStore {
        async fn put(&self, r: &ChunkMetadataRecord) -> arcanum_core::Result<()> {
            self.0.lock().unwrap().insert(r.chunk_id.0.to_string(), r.clone());
            Ok(())
        }
        async fn get(&self, id: &ChunkId) -> arcanum_core::Result<Option<ChunkMetadataRecord>> {
            Ok(self.0.lock().unwrap().get(&id.0.to_string()).cloned())
        }
        async fn delete_by_source_uri(&self, _: &str, _: &str) -> arcanum_core::Result<()> { Ok(()) }
    }

    fn sample_record(chunk_id: ChunkId, doc_id: DocumentId) -> ChunkMetadataRecord {
        ChunkMetadataRecord {
            chunk_id,
            document_id:   doc_id,
            collection_id: "col".into(),
            version_num:   1,
            source_uri:    "confluence://page/42".into(),
            snapshot_uri:  "file:///snapshots/d/1.raw".into(),
            canonical_uri: None,
            page:          Some(3),
            section:       Some("§3.2".into()),
            block_ids:     vec!["b1".into()],
            offset_start:  100,
            offset_end:    200,
            ingested_at:   Utc::now(),
        }
    }

    fn make_resolver() -> (DefaultEvidenceResolver, Arc<FakeMetaStore>) {
        let meta = Arc::new(FakeMetaStore::default());
        let vs   = Arc::new(NoOpDocumentVersionStore);
        let ts   = Arc::new(arcanum_tree::InMemoryTreeStore::new());
        let gs   = Arc::new(arcanum_graph::InMemoryGraphStore::new());
        (DefaultEvidenceResolver::new(meta.clone(), vs, ts, gs), meta)
    }

    #[tokio::test]
    async fn test_resolve_chunk_returns_proof_chain() {
        let (resolver, meta) = make_resolver();
        let chunk_id = ChunkId::new();
        let doc_id   = DocumentId::new();
        meta.put(&sample_record(chunk_id.clone(), doc_id)).await.unwrap();
        let chain = resolver.resolve_chunk(&chunk_id).await.unwrap();
        assert_eq!(chain.root.kind, EvidenceKind::Chunk);
        assert_eq!(chain.raw_sources.len(), 1);
        assert_eq!(chain.raw_sources[0].page, Some(3));
    }

    #[tokio::test]
    async fn test_resolve_chunk_not_found_returns_error() {
        let (resolver, _) = make_resolver();
        let err = resolver.resolve_chunk(&ChunkId::new()).await.unwrap_err();
        assert!(err.to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_resolve_tree_node_fans_out_to_chunks() {
        use arcanum_tree::InMemoryTreeStore;
        let meta = Arc::new(FakeMetaStore::default());
        let vs   = Arc::new(NoOpDocumentVersionStore);
        let ts   = Arc::new(InMemoryTreeStore::new());
        let gs   = Arc::new(arcanum_graph::InMemoryGraphStore::new());

        let cid1 = ChunkId::new();
        let cid2 = ChunkId::new();
        meta.put(&sample_record(cid1.clone(), DocumentId::new())).await.unwrap();
        meta.put(&sample_record(cid2.clone(), DocumentId::new())).await.unwrap();

        let nid = TreeNodeId::new();
        ts.insert_node("col", TreeNode {
            id:               nid.clone(),
            level:            0,
            text:             "summary node".into(),
            vector:           Vector(vec![]),
            parent:           None,
            children:         vec![],
            cluster_centroid: None,
            source_uri:       "file://doc.pdf".into(),
            leaf_chunk_ids:   vec![cid1, cid2],
        }).await.unwrap();

        let resolver = DefaultEvidenceResolver::new(meta, vs, ts, gs);
        let chain = resolver.resolve_tree_node(&nid).await.unwrap();
        assert_eq!(chain.root.kind, EvidenceKind::TreeNode);
        assert_eq!(chain.root.children.len(), 2);
        assert!(!chain.raw_sources.is_empty());
    }
}
