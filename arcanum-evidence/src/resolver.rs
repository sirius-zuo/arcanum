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

        // Cross-check the version still exists (confirms no GC race) and surface its status
        // in the returned node so callers can tell stale/GC'd evidence apart from live evidence
        // instead of that information only reaching a log line.
        let version_status = match self.version_store.get_version(&meta.document_id, meta.version_num).await? {
            Some(version) => {
                if version.status != arcanum_core::types::VersionStatus::Active {
                    tracing::warn!(
                        chunk_id = %chunk_id.0,
                        version = %meta.version_num,
                        status = ?version.status,
                        "chunk metadata references a non-active version"
                    );
                }
                match version.status {
                    arcanum_core::types::VersionStatus::Active     => "active",
                    arcanum_core::types::VersionStatus::Superseded => "superseded",
                    arcanum_core::types::VersionStatus::Deleted    => "deleted",
                }
            }
            None => {
                tracing::warn!(
                    chunk_id = %chunk_id.0,
                    document_id = %meta.document_id.0,
                    version = %meta.version_num,
                    "chunk metadata references a version not found in the version store"
                );
                "unknown"
            }
        };

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
                "page":            meta.page,
                "section":         meta.section,
                "version_status":  version_status,
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

/// Removes exact-duplicate citations (same snapshot, same span) from a raw_sources list.
/// `Vec::dedup_by_key` only collapses *consecutive* duplicates and was previously keyed on
/// `snapshot_uri` alone, which silently dropped distinct passages (different offsets) from
/// the same document whenever they happened to land next to each other in iteration order.
/// Keying on the full (snapshot_uri, offset_start, offset_end) tuple via a HashSet instead
/// only removes true duplicates, regardless of position in the list.
fn dedup_raw_sources(raw_sources: &mut Vec<RawSourceRef>) {
    let mut seen = std::collections::HashSet::new();
    raw_sources.retain(|r| seen.insert((r.snapshot_uri.clone(), r.offset_start, r.offset_end)));
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

        dedup_raw_sources(&mut raw_sources);

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
        dedup_raw_sources(&mut raw_sources);

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
        dedup_raw_sources(&mut raw_sources);

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
        async fn delete_by_document_version(&self, _: &DocumentId, _: u32) -> arcanum_core::Result<Vec<ChunkId>> { Ok(vec![]) }
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

    /// A version store that returns a fixed `Some(DocumentVersion)` for every lookup, so
    /// the "version found" branch of resolve_chunk_inner's cross-check is actually exercised
    /// (NoOpDocumentVersionStore always returns None, which only covers the "not found" path).
    struct FakeVersionStore(VersionStatus);

    #[async_trait::async_trait]
    impl DocumentVersionStore for FakeVersionStore {
        async fn get_latest(&self, _: &str, _: &str) -> arcanum_core::Result<Option<DocumentVersion>> { Ok(None) }
        async fn add_version(&self, _: DocumentVersion) -> arcanum_core::Result<()> { Ok(()) }
        async fn supersede_active(&self, _: &DocumentId) -> arcanum_core::Result<()> { Ok(()) }
        async fn list_versions(&self, _: &DocumentId) -> arcanum_core::Result<Vec<DocumentVersion>> { Ok(vec![]) }
        async fn get_versioning_policy(&self, _: &str) -> arcanum_core::Result<VersioningPolicy> { Ok(VersioningPolicy::Replace) }
        async fn set_versioning_policy(&self, _: &str, _: VersioningPolicy) -> arcanum_core::Result<()> { Ok(()) }
        async fn delete_by_source_uri(&self, _: &str, _: &str) -> arcanum_core::Result<()> { Ok(()) }
        async fn get_version(&self, document_id: &DocumentId, version_num: u32) -> arcanum_core::Result<Option<DocumentVersion>> {
            Ok(Some(DocumentVersion {
                document_id:   document_id.clone(),
                version_num,
                source_uri:    "confluence://page/42".into(),
                collection_id: "col".into(),
                content_hash:  "hash".into(),
                snapshot_uri:  "file:///snapshots/d/1.raw".into(),
                canonical_uri: None,
                mime_type:     "text/plain".into(),
                status:        self.0.clone(),
                ingested_at:   Utc::now(),
                extra:         HashMap::new(),
            }))
        }
        async fn list_collections(&self) -> arcanum_core::Result<Vec<String>> { Ok(vec![]) }
        async fn list_documents(&self, _: &str) -> arcanum_core::Result<Vec<DocumentEntry>> { Ok(vec![]) }
    }

    #[tokio::test]
    async fn test_resolve_chunk_reports_active_version_status() {
        let meta = Arc::new(FakeMetaStore::default());
        let vs   = Arc::new(FakeVersionStore(VersionStatus::Active));
        let ts   = Arc::new(arcanum_tree::InMemoryTreeStore::new());
        let gs   = Arc::new(arcanum_graph::InMemoryGraphStore::new());
        let resolver = DefaultEvidenceResolver::new(meta.clone(), vs, ts, gs);

        let chunk_id = ChunkId::new();
        meta.put(&sample_record(chunk_id.clone(), DocumentId::new())).await.unwrap();

        let chain = resolver.resolve_chunk(&chunk_id).await.unwrap();
        assert_eq!(chain.root.metadata["version_status"], "active");
    }

    #[tokio::test]
    async fn test_resolve_chunk_reports_non_active_version_status() {
        let meta = Arc::new(FakeMetaStore::default());
        let vs   = Arc::new(FakeVersionStore(VersionStatus::Superseded));
        let ts   = Arc::new(arcanum_tree::InMemoryTreeStore::new());
        let gs   = Arc::new(arcanum_graph::InMemoryGraphStore::new());
        let resolver = DefaultEvidenceResolver::new(meta.clone(), vs, ts, gs);

        let chunk_id = ChunkId::new();
        meta.put(&sample_record(chunk_id.clone(), DocumentId::new())).await.unwrap();

        let chain = resolver.resolve_chunk(&chunk_id).await.unwrap();
        assert_eq!(chain.root.metadata["version_status"], "superseded");
    }

    #[tokio::test]
    async fn test_dedup_raw_sources_keeps_distinct_spans_same_snapshot() {
        let mut sources = vec![
            RawSourceRef {
                document_id: DocumentId::new(), version_num: 1,
                source_uri: "s".into(), snapshot_uri: "snap".into(), canonical_uri: None,
                page: Some(1), section: None, block_ids: vec![],
                offset_start: 0, offset_end: 10,
            },
            RawSourceRef {
                document_id: DocumentId::new(), version_num: 1,
                source_uri: "s".into(), snapshot_uri: "snap".into(), canonical_uri: None,
                page: Some(2), section: None, block_ids: vec![],
                offset_start: 20, offset_end: 30,
            },
        ];
        dedup_raw_sources(&mut sources);
        assert_eq!(sources.len(), 2, "distinct spans within the same snapshot must both survive dedup");
    }
}
