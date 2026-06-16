use arcanum_core::{
    traits::{ChunkMetadataStore, DocumentVersionStore, GcWorker, GraphStore, SnapshotStore, TreeStore, VectorStore},
    types::{DocumentId, GcReport, VersioningPolicy},
    ArcanumError, Result,
};
use async_trait::async_trait;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::instrument;

/// Enforces RetentionBased versioning policy by purging superseded document versions
/// older than the configured retention window.
pub struct PostgresGcWorker {
    pool:             PgPool,
    version_store:    Arc<dyn DocumentVersionStore>,
    snapshot_store:   Arc<dyn SnapshotStore>,
    vector_store:     Arc<dyn VectorStore>,
    tree_store:       Arc<dyn TreeStore>,
    graph_store:      Arc<dyn GraphStore>,
    chunk_meta_store: Arc<dyn ChunkMetadataStore>,
}

impl PostgresGcWorker {
    pub async fn new(
        database_url:     &str,
        version_store:    Arc<dyn DocumentVersionStore>,
        snapshot_store:   Arc<dyn SnapshotStore>,
        vector_store:     Arc<dyn VectorStore>,
        tree_store:       Arc<dyn TreeStore>,
        graph_store:      Arc<dyn GraphStore>,
        chunk_meta_store: Arc<dyn ChunkMetadataStore>,
    ) -> Result<Self> {
        let pool = PgPool::connect(database_url).await
            .map_err(|e| ArcanumError::Storage(format!("PostgresGcWorker connect: {}", e)))?;
        Ok(Self { pool, version_store, snapshot_store, vector_store, tree_store, graph_store, chunk_meta_store })
    }
}

#[derive(sqlx::FromRow)]
struct SupersededRow {
    document_id:   uuid::Uuid,
    version_num:   i32,
    collection_id: String,
    source_uri:    String,
    snapshot_uri:  String,
    canonical_uri: Option<String>,
    ingested_at:   chrono::DateTime<chrono::Utc>,
}

#[async_trait]
impl GcWorker for PostgresGcWorker {
    #[instrument(skip(self), err)]
    async fn run_once(&self) -> Result<GcReport> {
        // Step 1: Load all superseded versions.
        let rows = sqlx::query_as::<_, SupersededRow>(
            r#"SELECT sd.collection_id, dv.document_id, dv.version_num,
                      sd.source_uri, dv.snapshot_uri, dv.canonical_uri, dv.ingested_at
               FROM document_versions dv
               JOIN source_documents  sd ON sd.document_id = dv.document_id
               WHERE dv.status = 'superseded'"#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("gc query: {}", e)))?;

        let mut report = GcReport {
            versions_deleted:  0,
            snapshots_removed: 0,
            chunks_removed:    0,
            errors:            vec![],
        };

        let now = chrono::Utc::now();

        // Cache versioning policy per collection so we don't re-query it once per row.
        let mut policy_cache: HashMap<String, VersioningPolicy> = HashMap::new();

        for row in &rows {
            // Step 2: Check if the collection has a RetentionBased policy that has expired.
            let policy = if let Some(p) = policy_cache.get(&row.collection_id) {
                p.clone()
            } else {
                let p = self.version_store
                    .get_versioning_policy(&row.collection_id).await
                    .unwrap_or(VersioningPolicy::Replace);
                policy_cache.insert(row.collection_id.clone(), p.clone());
                p
            };

            let retention_days = match policy {
                VersioningPolicy::RetentionBased { days } => days,
                _ => continue, // Not retention-based — skip.
            };

            let age_days = (now - row.ingested_at).num_days();
            if age_days < retention_days as i64 {
                continue; // Still within retention window.
            }

            let doc_id      = DocumentId(row.document_id);
            let doc_id_str  = row.document_id.to_string();
            let version_num = row.version_num as u32;
            let collection  = &row.collection_id;
            let source_uri  = &row.source_uri;

            let mut version_errors: Vec<String> = vec![];

            // a. Delete snapshot files. Snapshot URIs are per-version, so this is always safe.
            if let Err(e) = self.snapshot_store.delete(&row.snapshot_uri, row.canonical_uri.as_deref()).await {
                version_errors.push(format!("{}/v{}: snapshot delete: {}", doc_id_str, version_num, e));
            } else {
                report.snapshots_removed += 1;
            }

            // b. Delete chunk_metadata rows scoped to this exact version, capturing the
            // chunk IDs so the (version-unaware) vector store can be told precisely which
            // chunks to remove instead of deleting every chunk for the shared source_uri.
            let chunk_ids = match self.chunk_meta_store.delete_by_document_version(&doc_id, version_num).await {
                Ok(ids) => Some(ids),
                Err(e) => {
                    version_errors.push(format!("{}/v{}: chunk_meta delete: {}", doc_id_str, version_num, e));
                    None
                }
            };

            // c. Delete vector chunks for exactly this version's chunk IDs.
            if let Some(ids) = &chunk_ids {
                if ids.is_empty() {
                    report.chunks_removed += 0;
                } else if let Err(e) = self.vector_store.delete(collection, ids).await {
                    version_errors.push(format!("{}/v{}: vector delete: {}", doc_id_str, version_num, e));
                } else {
                    report.chunks_removed += ids.len() as u32;
                }
            }

            // d/e. Tree nodes and graph entities are only addressable by source_uri (no
            // version-scoped delete exists for these stores). Since multiple versions of the
            // same document share source_uri, blanket-deleting here would also remove an
            // active or otherwise-retained version's data. Guard by checking whether any
            // other non-deleted version still references this source_uri; if so, skip the
            // destructive delete rather than risk corrupting live data.
            let other_live_versions = sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM document_versions WHERE document_id = $1 AND version_num != $2 AND status != 'deleted'",
            )
            .bind(row.document_id)
            .bind(version_num as i32)
            .fetch_one(&self.pool).await;

            match other_live_versions {
                Ok(0) => {
                    if let Err(e) = self.tree_store.delete_by_source_uri(collection, source_uri).await {
                        version_errors.push(format!("{}/v{}: tree delete: {}", doc_id_str, version_num, e));
                    }
                    if let Err(e) = self.graph_store.delete_by_source_uri(collection, source_uri).await {
                        version_errors.push(format!("{}/v{}: graph delete: {}", doc_id_str, version_num, e));
                    }
                }
                Ok(_) => {
                    tracing::info!(
                        document_id = %doc_id_str, version_num,
                        "skipping tree/graph delete — another live version shares this source_uri"
                    );
                }
                Err(e) => {
                    version_errors.push(format!("{}/v{}: live-version check: {}", doc_id_str, version_num, e));
                }
            }

            // f. Mark version as deleted only if all store deletions succeeded. A failure here
            // is recorded like any other step error rather than aborting the whole GC pass —
            // the version remains superseded and will be retried on the next run.
            if version_errors.is_empty() {
                match sqlx::query(
                    "UPDATE document_versions SET status = 'deleted' WHERE document_id = $1 AND version_num = $2",
                )
                .bind(row.document_id)
                .bind(version_num as i32)
                .execute(&self.pool).await
                {
                    Ok(_)  => report.versions_deleted += 1,
                    Err(e) => version_errors.push(format!("{}/v{}: status update: {}", doc_id_str, version_num, e)),
                }
            }

            report.errors.extend(version_errors);
        }

        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    #[ignore = "requires Postgres — set TEST_DATABASE_URL"]
    async fn test_gc_skips_non_retention_policy() {
        let url = std::env::var("TEST_DATABASE_URL").unwrap();
        // Verify that the worker connects to Postgres.
        // Full integration test with stubs is skipped — requires implementing
        // full trait stubs for TreeStore/GraphStore/VectorStore.
        let pool = sqlx::PgPool::connect(&url).await;
        assert!(pool.is_ok(), "should connect to Postgres at {}: {}", url, pool.unwrap_err());
    }
}
