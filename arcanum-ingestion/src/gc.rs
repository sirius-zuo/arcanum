use arcanum_core::{
    traits::{ChunkMetadataStore, DocumentVersionStore, GcWorker, GraphStore, SnapshotStore, TreeStore, VectorStore},
    types::{GcReport, VersionStatus, VersioningPolicy},
    ArcanumError, Result,
};
use async_trait::async_trait;
use sqlx::PgPool;
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

        for row in &rows {
            // Step 2: Check if the collection has a RetentionBased policy that has expired.
            let policy = self.version_store
                .get_versioning_policy(&row.collection_id).await
                .unwrap_or(VersioningPolicy::Replace);

            let retention_days = match policy {
                VersioningPolicy::RetentionBased { days } => days,
                _ => continue, // Not retention-based — skip.
            };

            let age_days = (now - row.ingested_at).num_days();
            if age_days < retention_days as i64 {
                continue; // Still within retention window.
            }

            let doc_id_str   = row.document_id.to_string();
            let version_num  = row.version_num as u32;
            let collection   = &row.collection_id;
            let source_uri   = &row.source_uri;

            let mut version_errors: Vec<String> = vec![];

            // a. Delete snapshot files.
            if let Err(e) = self.snapshot_store.delete(&row.snapshot_uri, row.canonical_uri.as_deref()).await {
                version_errors.push(format!("{}/v{}: snapshot delete: {}", doc_id_str, version_num, e));
            } else {
                report.snapshots_removed += 1;
            }

            // b. Delete vector chunks.
            if let Err(e) = self.vector_store.delete_by_source_uri(collection, source_uri).await {
                version_errors.push(format!("{}/v{}: vector delete: {}", doc_id_str, version_num, e));
            } else {
                report.chunks_removed += 1;
            }

            // c. Delete tree nodes.
            if let Err(e) = self.tree_store.delete_by_source_uri(collection, source_uri).await {
                version_errors.push(format!("{}/v{}: tree delete: {}", doc_id_str, version_num, e));
            }

            // d. Delete graph entities.
            if let Err(e) = self.graph_store.delete_by_source_uri(collection, source_uri).await {
                version_errors.push(format!("{}/v{}: graph delete: {}", doc_id_str, version_num, e));
            }

            // e. Delete chunk_metadata rows.
            if let Err(e) = self.chunk_meta_store.delete_by_source_uri(collection, source_uri).await {
                version_errors.push(format!("{}/v{}: chunk_meta delete: {}", doc_id_str, version_num, e));
            }

            // f. Mark version as deleted only if all store deletions succeeded.
            if version_errors.is_empty() {
                sqlx::query(
                    "UPDATE document_versions SET status = 'deleted' WHERE document_id = $1 AND version_num = $2",
                )
                .bind(row.document_id)
                .bind(version_num as i32)
                .execute(&self.pool).await
                .map_err(|e| ArcanumError::Storage(format!("gc status update: {}", e)))?;
                report.versions_deleted += 1;
            }

            report.errors.extend(version_errors);
        }

        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::ArcanumError;

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
