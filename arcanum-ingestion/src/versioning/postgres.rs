use arcanum_core::{
    traits::DocumentVersionStore,
    types::{DocumentId, DocumentVersion, VersionStatus, VersioningPolicy},
    ArcanumError, Result,
};
use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use std::collections::HashMap;
use tracing::instrument;

/// PostgreSQL-backed DocumentVersionStore for production deployments.
///
/// Uses tables defined in `migrations/0002_evidence_foundation.sql`:
/// - `source_documents` — logical document identity
/// - `document_versions` — version history
/// - `collection_config` — per-collection versioning policy
pub struct PostgresDocumentVersionStore {
    pool: PgPool,
}

impl PostgresDocumentVersionStore {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPool::connect(database_url).await
            .map_err(|e| ArcanumError::Storage(format!("PostgresDocumentVersionStore connect error: {}", e)))?;
        let store = Self { pool };
        store.ensure_schema().await?;
        Ok(store)
    }

    async fn ensure_schema(&self) -> Result<()> {
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS source_documents (
                document_id   UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
                source_uri    TEXT        NOT NULL,
                collection_id TEXT        NOT NULL,
                created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                UNIQUE (source_uri, collection_id)
            )
        "#)
        .execute(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("ensure source_documents: {}", e)))?;

        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS document_versions (
                document_id   UUID        NOT NULL REFERENCES source_documents(document_id),
                version_num   INTEGER     NOT NULL,
                content_hash  TEXT        NOT NULL,
                snapshot_uri  TEXT        NOT NULL,
                canonical_uri TEXT,
                mime_type     TEXT        NOT NULL DEFAULT '',
                status        TEXT        NOT NULL DEFAULT 'active'
                              CHECK (status IN ('active', 'superseded', 'deleted')),
                ingested_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                extra         JSONB,
                PRIMARY KEY (document_id, version_num)
            )
        "#)
        .execute(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("ensure document_versions: {}", e)))?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_dv_doc_status ON document_versions (document_id, status)")
            .execute(&self.pool)
            .await
            .map_err(|e| ArcanumError::Storage(format!("ensure dv index: {}", e)))?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_dv_content_hash ON document_versions (content_hash)")
            .execute(&self.pool)
            .await
            .map_err(|e| ArcanumError::Storage(format!("ensure hash index: {}", e)))?;

        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS collection_config (
                collection_id     TEXT        PRIMARY KEY,
                versioning_policy TEXT        NOT NULL DEFAULT 'replace'
                                  CHECK (versioning_policy IN ('replace', 'append_only', 'retention_based')),
                retention_days    INTEGER,
                created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
        "#)
        .execute(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("ensure collection_config: {}", e)))?;

        // arcanum_tree_nodes — used by the tree/vector engine
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS arcanum_tree_nodes (
                id             UUID    PRIMARY KEY,
                collection     TEXT    NOT NULL,
                level          INTEGER NOT NULL,
                text           TEXT    NOT NULL,
                vector         JSONB   NOT NULL,
                centroid       JSONB,
                parent_id      UUID,
                children       JSONB   NOT NULL DEFAULT '[]',
                source_uri     TEXT    NOT NULL DEFAULT '',
                leaf_chunk_ids JSONB   NOT NULL DEFAULT '[]'
            )
        "#)
        .execute(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("ensure arcanum_tree_nodes: {}", e)))?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_tree_nodes_collection_level ON arcanum_tree_nodes (collection, level)")
            .execute(&self.pool)
            .await
            .map_err(|e| ArcanumError::Storage(format!("ensure tree_nodes index: {}", e)))?;

        // Add leaf_chunk_ids to existing deployments that have the old schema without it.
        sqlx::query("ALTER TABLE arcanum_tree_nodes ADD COLUMN IF NOT EXISTS leaf_chunk_ids JSONB NOT NULL DEFAULT '[]'")
            .execute(&self.pool)
            .await
            .map_err(|e| ArcanumError::Storage(format!("alter tree_nodes add leaf_chunk_ids: {}", e)))?;

        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS arcanum_tree_collections (
                name TEXT PRIMARY KEY
            )
        "#)
        .execute(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("ensure arcanum_tree_collections: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl DocumentVersionStore for PostgresDocumentVersionStore {
    #[instrument(skip(self), fields(store = "postgres_version", source_uri, collection), err)]
    async fn get_latest(
        &self,
        source_uri:    &str,
        collection_id: &str,
    ) -> Result<Option<DocumentVersion>> {
        // Find the source document first.
        let doc_id: Option<(uuid::Uuid,)> = sqlx::query_as(
            "SELECT document_id FROM source_documents WHERE source_uri = $1 AND collection_id = $2"
        )
        .bind(source_uri)
        .bind(collection_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("find source document: {}", e)))?;

        let doc_id = match doc_id {
            Some((id,)) => id,
            None => return Ok(None),
        };

        // Get the active version with the highest version_num.
        let row = sqlx::query_as::<_, VersionRow>(
            r#"SELECT document_id, version_num, content_hash, snapshot_uri, canonical_uri, mime_type,
                      status, ingested_at, extra
               FROM document_versions
               WHERE document_id = $1 AND status = 'active'
               ORDER BY version_num DESC LIMIT 1"#
        )
        .bind(doc_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("get latest version: {}", e)))?;

        match row {
            Some(r) => {
                let status = match r.status.as_str() {
                    "active" => VersionStatus::Active,
                    "superseded" => VersionStatus::Superseded,
                    "deleted" => VersionStatus::Deleted,
                    _ => VersionStatus::Active,
                };
                let extra: std::collections::HashMap<String, serde_json::Value> = r.extra
                    .as_ref()
                    .and_then(|v| v.as_object())
                    .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                    .unwrap_or_default();
                Ok(Some(DocumentVersion {
                    document_id:   DocumentId(r.document_id),
                    version_num:   r.version_num as u32,
                    source_uri:    source_uri.to_string(),
                    collection_id: collection_id.to_string(),
                    content_hash:  r.content_hash,
                    snapshot_uri:  r.snapshot_uri,
                    canonical_uri: r.canonical_uri,
                    mime_type:     r.mime_type,
                    status,
                    ingested_at:   r.ingested_at,
                    extra,
                }))
            }
            None => Ok(None),
        }
    }

    #[instrument(skip(self), fields(store = "postgres_version", doc_id = %version.document_id.0, version = version.version_num), err)]
    async fn add_version(&self, version: DocumentVersion) -> Result<()> {
        let doc_id = version.document_id.0;

        // Ensure source document row exists; return its document_id whether new or pre-existing.
        // ON CONFLICT DO UPDATE touches the row so RETURNING always fires (unlike DO NOTHING).
        let (resolved_id,): (uuid::Uuid,) = sqlx::query_as(
            r#"INSERT INTO source_documents (document_id, source_uri, collection_id)
               VALUES ($1, $2, $3)
               ON CONFLICT (source_uri, collection_id) DO UPDATE
                   SET source_uri = EXCLUDED.source_uri
               RETURNING document_id"#
        )
        .bind(doc_id)
        .bind(&version.source_uri)
        .bind(&version.collection_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("ensure source document: {}", e)))?;

        // Insert version row. ON CONFLICT upserts so a concurrent race on the same
        // (document_id, version_num) doesn't crash — last writer wins.
        sqlx::query(
            r#"INSERT INTO document_versions
               (document_id, version_num, content_hash, snapshot_uri, canonical_uri,
                mime_type, status, ingested_at, extra)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
               ON CONFLICT (document_id, version_num) DO UPDATE
                   SET content_hash  = EXCLUDED.content_hash,
                       snapshot_uri  = EXCLUDED.snapshot_uri,
                       canonical_uri = EXCLUDED.canonical_uri,
                       mime_type     = EXCLUDED.mime_type,
                       status        = EXCLUDED.status,
                       ingested_at   = EXCLUDED.ingested_at,
                       extra         = EXCLUDED.extra"#
        )
        .bind(resolved_id)
        .bind(version.version_num as i32)
        .bind(&version.content_hash)
        .bind(&version.snapshot_uri)
        .bind(&version.canonical_uri)
        .bind(&version.mime_type)
        .bind(match version.status {
            VersionStatus::Active => "active",
            VersionStatus::Superseded => "superseded",
            VersionStatus::Deleted => "deleted",
        })
        .bind(version.ingested_at)
        .bind(serde_json::to_value(&version.extra)
            .map_err(|e| ArcanumError::Storage(format!("serialize version extra: {}", e)))?)
        .execute(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("insert version: {}", e)))?;

        Ok(())
    }

    #[instrument(skip(self), fields(store = "postgres_version", doc_id = %document_id.0), err)]
    async fn supersede_active(&self, document_id: &DocumentId) -> Result<()> {
        sqlx::query(
            "UPDATE document_versions SET status = 'superseded' WHERE document_id = $1 AND status = 'active'"
        )
        .bind(document_id.0)
        .execute(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("supersede active: {}", e)))?;
        Ok(())
    }

    #[instrument(skip(self), fields(store = "postgres_version", doc_id = %document_id.0), err)]
    async fn list_versions(&self, document_id: &DocumentId) -> Result<Vec<DocumentVersion>> {
        #[derive(sqlx::FromRow)]
        struct ListRow {
            document_id:   uuid::Uuid,
            version_num:   i32,
            source_uri:    String,
            collection_id: String,
            content_hash:  String,
            snapshot_uri:  String,
            canonical_uri: Option<String>,
            mime_type:     String,
            status:        String,
            ingested_at:   chrono::DateTime<Utc>,
            extra:         Option<serde_json::Value>,
        }

        let rows = sqlx::query_as::<_, ListRow>(
            r#"SELECT dv.document_id, dv.version_num, sd.source_uri, sd.collection_id,
                      dv.content_hash, dv.snapshot_uri, dv.canonical_uri, dv.mime_type,
                      dv.status, dv.ingested_at, dv.extra
               FROM document_versions dv
               JOIN source_documents sd ON sd.document_id = dv.document_id
               WHERE dv.document_id = $1
               ORDER BY dv.version_num ASC"#
        )
        .bind(document_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("list versions: {}", e)))?;

        rows.into_iter().map(|r| {
            let status = match r.status.as_str() {
                "active" => VersionStatus::Active,
                "superseded" => VersionStatus::Superseded,
                "deleted" => VersionStatus::Deleted,
                _ => VersionStatus::Active,
            };
            let extra: HashMap<String, serde_json::Value> = r.extra
                .as_ref()
                .and_then(|v| v.as_object())
                .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default();
            Ok(DocumentVersion {
                document_id:   DocumentId(r.document_id),
                version_num:   r.version_num as u32,
                source_uri:    r.source_uri,
                collection_id: r.collection_id,
                content_hash:  r.content_hash,
                snapshot_uri:  r.snapshot_uri,
                canonical_uri: r.canonical_uri,
                mime_type:     r.mime_type,
                status,
                ingested_at:   r.ingested_at,
                extra,
            })
        }).collect()
    }

    #[instrument(skip(self), fields(store = "postgres_version", collection), err)]
    async fn get_versioning_policy(&self, collection_id: &str) -> Result<VersioningPolicy> {
        let row: Option<(String, Option<i32>,)> = sqlx::query_as(
            "SELECT versioning_policy, retention_days FROM collection_config WHERE collection_id = $1"
        )
        .bind(collection_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("get policy: {}", e)))?;

        match row {
            Some((policy_str, retention_days)) => {
                let policy = match policy_str.as_str() {
                    "append_only" => VersioningPolicy::AppendOnly,
                    "retention_based" => VersioningPolicy::RetentionBased {
                        days: retention_days.unwrap_or(30) as u32,
                    },
                    _ => VersioningPolicy::Replace,
                };
                Ok(policy)
            }
            None => {
                // Default: ensure a row exists with the default policy.
                sqlx::query(
                    "INSERT INTO collection_config (collection_id, versioning_policy) VALUES ($1, 'replace')"
                )
                .bind(collection_id)
                .execute(&self.pool)
                .await
                .ok(); // ignore conflict if row already exists
                Ok(VersioningPolicy::Replace)
            }
        }
    }

    #[instrument(skip(self), fields(store = "postgres_version", collection), err)]
    async fn set_versioning_policy(&self, collection_id: &str, policy: VersioningPolicy) -> Result<()> {
        let policy_str = match &policy {
            VersioningPolicy::Replace => "replace",
            VersioningPolicy::AppendOnly => "append_only",
            VersioningPolicy::RetentionBased { days } => {
                sqlx::query(
                    r#"INSERT INTO collection_config (collection_id, versioning_policy, retention_days)
                       VALUES ($1, 'retention_based', $2)
                       ON CONFLICT (collection_id) DO UPDATE SET
                           versioning_policy = 'retention_based',
                           retention_days = $2"#
                )
                .bind(collection_id)
                .bind(*days as i32)
                .execute(&self.pool)
                .await
                .map_err(|e| ArcanumError::Storage(format!("set retention policy: {}", e)))?;
                return Ok(());
            }
        };

        sqlx::query(
            r#"INSERT INTO collection_config (collection_id, versioning_policy)
               VALUES ($1, $2)
               ON CONFLICT (collection_id) DO UPDATE SET versioning_policy = $2"#
        )
        .bind(collection_id)
        .bind(policy_str)
        .execute(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("set policy: {}", e)))?;
        Ok(())
    }

    async fn delete_by_source_uri(&self, collection_id: &str, source_uri: &str) -> Result<()> {
        sqlx::query(
            r#"DELETE FROM document_versions
               WHERE document_id IN (
                   SELECT document_id FROM source_documents
                   WHERE source_uri = $1 AND collection_id = $2
               )"#
        )
        .bind(source_uri)
        .bind(collection_id)
        .execute(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("delete by source_uri: {}", e)))?;
        Ok(())
    }

    #[instrument(skip(self), fields(store = "postgres_version", doc_id = %document_id.0, version = version_num), err)]
    async fn get_version(
        &self,
        document_id: &DocumentId,
        version_num: u32,
    ) -> Result<Option<DocumentVersion>> {
        let row: Option<VersionRow> = sqlx::query_as::<_, VersionRow>(
            r#"SELECT document_id, version_num, content_hash, snapshot_uri, canonical_uri, mime_type,
                      status, ingested_at, extra
               FROM document_versions
               WHERE document_id = $1 AND version_num = $2"#
        )
        .bind(document_id.0)
        .bind(version_num as i32)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("get_version postgres: {}", e)))?;

        let Some(r) = row else { return Ok(None) };

        let sd_row: Option<(String, String,)> = sqlx::query_as(
            "SELECT source_uri, collection_id FROM source_documents WHERE document_id = $1"
        )
        .bind(document_id.0)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("get_version source_doc: {}", e)))?;

        let (source_uri, collection_id) = sd_row
            .map(|(s, c)| (s, c))
            .unwrap_or_default();

        let status = match r.status.as_str() {
            "active" => VersionStatus::Active,
            "superseded" => VersionStatus::Superseded,
            "deleted" => VersionStatus::Deleted,
            _ => VersionStatus::Active,
        };
        let extra: HashMap<String, serde_json::Value> = r.extra
            .as_ref()
            .and_then(|v| v.as_object())
            .map(|o| o.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default();

        Ok(Some(DocumentVersion {
            document_id:   DocumentId(r.document_id),
            version_num:   r.version_num as u32,
            source_uri,
            collection_id,
            content_hash:  r.content_hash,
            snapshot_uri:  r.snapshot_uri,
            canonical_uri: r.canonical_uri,
            mime_type:     r.mime_type,
            status,
            ingested_at:   r.ingested_at,
            extra,
        }))
    }
}

#[derive(sqlx::FromRow)]
struct VersionRow {
    document_id:   uuid::Uuid,
    version_num:   i32,
    content_hash:  String,
    snapshot_uri:  String,
    canonical_uri: Option<String>,
    mime_type:     String,
    status:        String,
    ingested_at:   chrono::DateTime<Utc>,
    extra:         Option<serde_json::Value>,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke test — verifies the module compiles and types are correct.
    #[test]
    fn test_postgres_version_store_module_compiles() {
        fn _assert_send_sync<T: Send + Sync>() {}
        _assert_send_sync::<PostgresDocumentVersionStore>();
    }

    /// Integration test — requires a live PostgreSQL instance.
    #[tokio::test]
    #[ignore]
    async fn test_postgres_version_store_integration() {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/arcanum_test".to_string());
        let store = PostgresDocumentVersionStore::new(&db_url).await.expect("connect");

        let doc_id = DocumentId::new();
        let version = DocumentVersion {
            document_id: doc_id.clone(),
            version_num: 1,
            source_uri: "file://test.md".into(),
            collection_id: "test-col".into(),
            content_hash: "abc123".into(),
            snapshot_uri: "file:///snap/1.raw".into(),
            canonical_uri: Some("file:///snap/1.json".into()),
            mime_type: "text/markdown".into(),
            status: VersionStatus::Active,
            ingested_at: Utc::now(),
            extra: HashMap::new(),
        };
        store.add_version(version).await.expect("add_version");

        let latest = store.get_latest("file://test.md", "test-col").await.expect("get_latest");
        assert!(latest.is_some());
        let v = latest.unwrap();
        assert_eq!(v.version_num, 1);
        assert_eq!(v.document_id, doc_id);
    }

    /// Bug #1 — re-ingesting an existing document used to crash with RowNotFound.
    #[tokio::test]
    #[ignore]
    async fn test_add_version_is_idempotent_for_existing_source_doc() {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/arcanum_test".to_string());
        let store = PostgresDocumentVersionStore::new(&db_url).await.unwrap();

        let doc = DocumentVersion {
            document_id:   DocumentId::new(),
            version_num:   1,
            source_uri:    "file://bug1-test.md".into(),
            collection_id: "test-col-bug1".into(),
            content_hash:  "hash-v1".into(),
            snapshot_uri:  "file:///snap/v1.raw".into(),
            canonical_uri: None,
            mime_type:     "text/markdown".into(),
            status:        VersionStatus::Active,
            ingested_at:   Utc::now(),
            extra:         HashMap::new(),
        };
        store.add_version(doc.clone()).await.unwrap();    // first ingestion

        let mut doc_v2 = doc.clone();
        doc_v2.version_num  = 2;
        doc_v2.content_hash = "hash-v2".into();
        doc_v2.snapshot_uri = "file:///snap/v2.raw".into();
        store.add_version(doc_v2).await.unwrap();         // second ingestion — must not crash
    }

    /// Bug #3 — get_latest must return the real content_hash.
    #[tokio::test]
    #[ignore]
    async fn test_get_latest_returns_content_hash() {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/arcanum_test".to_string());
        let store = PostgresDocumentVersionStore::new(&db_url).await.unwrap();

        let doc = DocumentVersion {
            document_id:   DocumentId::new(),
            version_num:   1,
            source_uri:    "file://bug3-test.md".into(),
            collection_id: "test-col-bug3".into(),
            content_hash:  "sha256-abc123".into(),
            snapshot_uri:  "file:///snap/v1.raw".into(),
            canonical_uri: None,
            mime_type:     "text/plain".into(),
            status:        VersionStatus::Active,
            ingested_at:   Utc::now(),
            extra:         HashMap::new(),
        };
        store.add_version(doc).await.unwrap();

        let latest = store.get_latest("file://bug3-test.md", "test-col-bug3").await.unwrap().unwrap();
        assert_eq!(latest.content_hash, "sha256-abc123");
        assert_eq!(latest.source_uri, "file://bug3-test.md");
        assert_eq!(latest.collection_id, "test-col-bug3");
    }

    /// Bug #8 — list_versions must return populated source_uri and collection_id.
    #[tokio::test]
    #[ignore]
    async fn test_list_versions_returns_full_document_version() {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/arcanum_test".to_string());
        let store = PostgresDocumentVersionStore::new(&db_url).await.unwrap();

        let doc_id = DocumentId::new();
        for v in 1u32..=2 {
            store.add_version(DocumentVersion {
                document_id:   doc_id.clone(),
                version_num:   v,
                source_uri:    "file://bug8-test.md".into(),
                collection_id: "test-col-bug8".into(),
                content_hash:  format!("hash-v{}", v),
                snapshot_uri:  format!("file:///snap/v{}.raw", v),
                canonical_uri: None,
                mime_type:     "text/plain".into(),
                status:        VersionStatus::Active,
                ingested_at:   Utc::now(),
                extra:         HashMap::new(),
            }).await.unwrap();
        }

        let versions = store.list_versions(&doc_id).await.unwrap();
        assert_eq!(versions.len(), 2);
        for v in &versions {
            assert_eq!(v.source_uri, "file://bug8-test.md", "source_uri must be populated");
            assert_eq!(v.collection_id, "test-col-bug8", "collection_id must be populated");
            assert!(!v.content_hash.is_empty(), "content_hash must be populated");
        }
    }

    /// Bug #2 — delete_by_source_uri must not crash with column-not-found.
    #[tokio::test]
    #[ignore]
    async fn test_delete_by_source_uri_works() {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://postgres:postgres@localhost/arcanum_test".to_string());
        let store = PostgresDocumentVersionStore::new(&db_url).await.unwrap();

        store.add_version(DocumentVersion {
            document_id:   DocumentId::new(),
            version_num:   1,
            source_uri:    "file://bug2-test.md".into(),
            collection_id: "test-col-bug2".into(),
            content_hash:  "hash".into(),
            snapshot_uri:  "file:///snap.raw".into(),
            canonical_uri: None,
            mime_type:     "text/plain".into(),
            status:        VersionStatus::Active,
            ingested_at:   Utc::now(),
            extra:         HashMap::new(),
        }).await.unwrap();

        store.delete_by_source_uri("test-col-bug2", "file://bug2-test.md").await.unwrap();

        let latest = store.get_latest("file://bug2-test.md", "test-col-bug2").await.unwrap();
        assert!(latest.is_none(), "deleted doc must not appear in get_latest");
    }
}
