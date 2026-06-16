use arcanum_core::{
    traits::DocumentVersionStore,
    types::{DocumentEntry, DocumentId, DocumentVersion, VersionStatus, VersioningPolicy},
    ArcanumError, Result,
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::SqlitePool;
use std::collections::HashMap;
use tracing::instrument;

/// SQLite-backed DocumentVersionStore for local development and testing.
/// Use PostgresDocumentVersionStore in production.
pub struct SqliteDocumentVersionStore {
    pool: SqlitePool,
}

impl SqliteDocumentVersionStore {
    /// Open (or create) a SQLite version store at `path`.
    /// Use `":memory:"` for in-process tests.
    pub async fn open(path: &str) -> Result<Self> {
        if path != ":memory:" {
            if let Some(parent) = std::path::Path::new(path).parent() {
                if !parent.as_os_str().is_empty() {
                    tokio::fs::create_dir_all(parent).await
                        .map_err(|e| ArcanumError::Storage(format!("create db dir: {}", e)))?;
                }
            }
        }
        let url = if path == ":memory:" {
            "sqlite::memory:".to_string()
        } else if path.starts_with("sqlite:") {
            path.to_string()
        } else {
            format!("sqlite://{}?mode=rwc", path)
        };
        let pool = SqlitePool::connect(&url).await
            .map_err(|e| ArcanumError::Storage(format!("SqliteDocumentVersionStore open: {}", e)))?;
        let store = Self { pool };
        store.ensure_schema().await?;
        Ok(store)
    }

    async fn ensure_schema(&self) -> Result<()> {
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS source_documents (
                document_id   TEXT NOT NULL PRIMARY KEY,
                source_uri    TEXT NOT NULL,
                collection_id TEXT NOT NULL,
                created_at    TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE (source_uri, collection_id)
            )
        "#).execute(&self.pool).await
            .map_err(|e| ArcanumError::Storage(format!("ensure source_documents: {}", e)))?;

        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS document_versions (
                document_id   TEXT    NOT NULL REFERENCES source_documents(document_id),
                version_num   INTEGER NOT NULL,
                content_hash  TEXT    NOT NULL,
                snapshot_uri  TEXT    NOT NULL,
                canonical_uri TEXT,
                mime_type     TEXT    NOT NULL DEFAULT '',
                status        TEXT    NOT NULL DEFAULT 'active'
                              CHECK (status IN ('active', 'superseded', 'deleted')),
                ingested_at   TEXT    NOT NULL DEFAULT (datetime('now')),
                extra         TEXT,
                PRIMARY KEY (document_id, version_num)
            )
        "#).execute(&self.pool).await
            .map_err(|e| ArcanumError::Storage(format!("ensure document_versions: {}", e)))?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_dv_doc_status ON document_versions (document_id, status)")
            .execute(&self.pool).await
            .map_err(|e| ArcanumError::Storage(format!("ensure dv index: {}", e)))?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_dv_content_hash ON document_versions (content_hash)")
            .execute(&self.pool).await
            .map_err(|e| ArcanumError::Storage(format!("ensure hash index: {}", e)))?;

        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS collection_config (
                collection_id     TEXT    NOT NULL PRIMARY KEY,
                versioning_policy TEXT    NOT NULL DEFAULT 'replace'
                                  CHECK (versioning_policy IN ('replace', 'append_only', 'retention_based')),
                retention_days    INTEGER,
                created_at        TEXT    NOT NULL DEFAULT (datetime('now'))
            )
        "#).execute(&self.pool).await
            .map_err(|e| ArcanumError::Storage(format!("ensure collection_config: {}", e)))?;

        Ok(())
    }
}

#[derive(sqlx::FromRow)]
struct SqVersionRow {
    document_id:   String,
    version_num:   i64,
    content_hash:  String,
    snapshot_uri:  String,
    canonical_uri: Option<String>,
    mime_type:     String,
    status:        String,
    ingested_at:   DateTime<Utc>,
    extra:         Option<String>,
}

fn parse_status(s: &str) -> VersionStatus {
    match s {
        "superseded" => VersionStatus::Superseded,
        "deleted"    => VersionStatus::Deleted,
        _            => VersionStatus::Active,
    }
}

fn status_str(s: &VersionStatus) -> &'static str {
    match s {
        VersionStatus::Active     => "active",
        VersionStatus::Superseded => "superseded",
        VersionStatus::Deleted    => "deleted",
    }
}

fn parse_extra(raw: Option<&str>) -> HashMap<String, serde_json::Value> {
    raw.and_then(|s| serde_json::from_str(s).ok()).unwrap_or_default()
}

fn row_to_version(r: SqVersionRow, source_uri: &str, collection_id: &str) -> Result<DocumentVersion> {
    let id = uuid::Uuid::parse_str(&r.document_id)
        .map_err(|e| ArcanumError::Storage(format!("invalid document_id uuid: {}", e)))?;
    Ok(DocumentVersion {
        document_id:   DocumentId(id),
        version_num:   r.version_num as u32,
        source_uri:    source_uri.to_string(),
        collection_id: collection_id.to_string(),
        content_hash:  r.content_hash,
        snapshot_uri:  r.snapshot_uri,
        canonical_uri: r.canonical_uri,
        mime_type:     r.mime_type,
        status:        parse_status(&r.status),
        ingested_at:   r.ingested_at,
        extra:         parse_extra(r.extra.as_deref()),
    })
}

#[async_trait]
impl DocumentVersionStore for SqliteDocumentVersionStore {
    #[instrument(skip(self), fields(store = "sqlite_version"), err)]
    async fn get_latest(&self, source_uri: &str, collection_id: &str) -> Result<Option<DocumentVersion>> {
        let doc_id: Option<(String,)> = sqlx::query_as(
            "SELECT document_id FROM source_documents WHERE source_uri = $1 AND collection_id = $2"
        )
        .bind(source_uri).bind(collection_id)
        .fetch_optional(&self.pool).await
        .map_err(|e| ArcanumError::Storage(format!("find source doc: {}", e)))?;

        let doc_id = match doc_id { Some((id,)) => id, None => return Ok(None) };

        let row = sqlx::query_as::<_, SqVersionRow>(
            r#"SELECT document_id, version_num, content_hash, snapshot_uri, canonical_uri,
                      mime_type, status, ingested_at, extra
               FROM document_versions
               WHERE document_id = $1 AND status = 'active'
               ORDER BY version_num DESC LIMIT 1"#
        )
        .bind(&doc_id)
        .fetch_optional(&self.pool).await
        .map_err(|e| ArcanumError::Storage(format!("get latest: {}", e)))?;

        match row {
            Some(r) => Ok(Some(row_to_version(r, source_uri, collection_id)?)),
            None    => Ok(None),
        }
    }

    #[instrument(skip(self), fields(store = "sqlite_version", doc_id = %version.document_id.0), err)]
    async fn add_version(&self, version: DocumentVersion) -> Result<()> {
        let doc_id_str = version.document_id.0.to_string();
        let extra_json = serde_json::to_string(&version.extra)
            .map_err(|e| ArcanumError::Storage(format!("serialize extra: {}", e)))?;

        // Ensure source_documents row; ON CONFLICT DO UPDATE touches the row so RETURNING fires.
        sqlx::query(
            r#"INSERT INTO source_documents (document_id, source_uri, collection_id)
               VALUES ($1, $2, $3)
               ON CONFLICT (source_uri, collection_id) DO UPDATE SET source_uri = excluded.source_uri"#
        )
        .bind(&doc_id_str).bind(&version.source_uri).bind(&version.collection_id)
        .execute(&self.pool).await
        .map_err(|e| ArcanumError::Storage(format!("ensure source doc: {}", e)))?;

        // Upsert version row — last writer wins on (document_id, version_num) conflict.
        sqlx::query(
            r#"INSERT INTO document_versions
               (document_id, version_num, content_hash, snapshot_uri, canonical_uri,
                mime_type, status, ingested_at, extra)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
               ON CONFLICT (document_id, version_num) DO UPDATE
                   SET content_hash  = excluded.content_hash,
                       snapshot_uri  = excluded.snapshot_uri,
                       canonical_uri = excluded.canonical_uri,
                       mime_type     = excluded.mime_type,
                       status        = excluded.status,
                       ingested_at   = excluded.ingested_at,
                       extra         = excluded.extra"#
        )
        .bind(&doc_id_str)
        .bind(version.version_num as i64)
        .bind(&version.content_hash)
        .bind(&version.snapshot_uri)
        .bind(&version.canonical_uri)
        .bind(&version.mime_type)
        .bind(status_str(&version.status))
        .bind(version.ingested_at)
        .bind(&extra_json)
        .execute(&self.pool).await
        .map_err(|e| ArcanumError::Storage(format!("insert version: {}", e)))?;

        Ok(())
    }

    #[instrument(skip(self), fields(store = "sqlite_version", doc_id = %document_id.0), err)]
    async fn supersede_active(&self, document_id: &DocumentId) -> Result<()> {
        sqlx::query(
            "UPDATE document_versions SET status = 'superseded' WHERE document_id = $1 AND status = 'active'"
        )
        .bind(document_id.0.to_string())
        .execute(&self.pool).await
        .map_err(|e| ArcanumError::Storage(format!("supersede active: {}", e)))?;
        Ok(())
    }

    #[instrument(skip(self), fields(store = "sqlite_version", doc_id = %document_id.0), err)]
    async fn list_versions(&self, document_id: &DocumentId) -> Result<Vec<DocumentVersion>> {
        #[derive(sqlx::FromRow)]
        struct ListRow {
            document_id:   String,
            version_num:   i64,
            content_hash:  String,
            snapshot_uri:  String,
            canonical_uri: Option<String>,
            mime_type:     String,
            status:        String,
            ingested_at:   DateTime<Utc>,
            extra:         Option<String>,
            source_uri:    String,
            collection_id: String,
        }

        let rows = sqlx::query_as::<_, ListRow>(
            r#"SELECT dv.document_id, dv.version_num, dv.content_hash, dv.snapshot_uri,
                      dv.canonical_uri, dv.mime_type, dv.status, dv.ingested_at, dv.extra,
                      sd.source_uri, sd.collection_id
               FROM document_versions dv
               JOIN source_documents sd ON sd.document_id = dv.document_id
               WHERE dv.document_id = $1
               ORDER BY dv.version_num ASC"#
        )
        .bind(document_id.0.to_string())
        .fetch_all(&self.pool).await
        .map_err(|e| ArcanumError::Storage(format!("list versions: {}", e)))?;

        rows.into_iter().map(|r| {
            let id = uuid::Uuid::parse_str(&r.document_id)
                .map_err(|e| ArcanumError::Storage(format!("invalid uuid: {}", e)))?;
            Ok(DocumentVersion {
                document_id:   DocumentId(id),
                version_num:   r.version_num as u32,
                source_uri:    r.source_uri,
                collection_id: r.collection_id,
                content_hash:  r.content_hash,
                snapshot_uri:  r.snapshot_uri,
                canonical_uri: r.canonical_uri,
                mime_type:     r.mime_type,
                status:        parse_status(&r.status),
                ingested_at:   r.ingested_at,
                extra:         parse_extra(r.extra.as_deref()),
            })
        }).collect()
    }

    #[instrument(skip(self), fields(store = "sqlite_version"), err)]
    async fn get_versioning_policy(&self, collection_id: &str) -> Result<VersioningPolicy> {
        let row: Option<(String, Option<i64>)> = sqlx::query_as(
            "SELECT versioning_policy, retention_days FROM collection_config WHERE collection_id = $1"
        )
        .bind(collection_id)
        .fetch_optional(&self.pool).await
        .map_err(|e| ArcanumError::Storage(format!("get policy: {}", e)))?;

        match row {
            Some((policy_str, retention_days)) => {
                Ok(match policy_str.as_str() {
                    "append_only"     => VersioningPolicy::AppendOnly,
                    "retention_based" => VersioningPolicy::RetentionBased {
                        days: retention_days.unwrap_or(30) as u32,
                    },
                    _ => VersioningPolicy::Replace,
                })
            }
            None => {
                sqlx::query(
                    "INSERT OR IGNORE INTO collection_config (collection_id, versioning_policy) VALUES ($1, 'replace')"
                )
                .bind(collection_id)
                .execute(&self.pool).await.ok();
                Ok(VersioningPolicy::Replace)
            }
        }
    }

    #[instrument(skip(self), fields(store = "sqlite_version", collection), err)]
    async fn set_versioning_policy(&self, collection_id: &str, policy: VersioningPolicy) -> Result<()> {
        match &policy {
            VersioningPolicy::RetentionBased { days } => {
                sqlx::query(
                    r#"INSERT INTO collection_config (collection_id, versioning_policy, retention_days)
                       VALUES ($1, 'retention_based', $2)
                       ON CONFLICT (collection_id) DO UPDATE
                           SET versioning_policy = 'retention_based', retention_days = excluded.retention_days"#
                )
                .bind(collection_id).bind(*days as i64)
                .execute(&self.pool).await
                .map_err(|e| ArcanumError::Storage(format!("set retention policy: {}", e)))?;
            }
            other => {
                let policy_str = match other {
                    VersioningPolicy::Replace    => "replace",
                    VersioningPolicy::AppendOnly => "append_only",
                    VersioningPolicy::RetentionBased { .. } => unreachable!(),
                };
                sqlx::query(
                    r#"INSERT INTO collection_config (collection_id, versioning_policy)
                       VALUES ($1, $2)
                       ON CONFLICT (collection_id) DO UPDATE SET versioning_policy = excluded.versioning_policy"#
                )
                .bind(collection_id).bind(policy_str)
                .execute(&self.pool).await
                .map_err(|e| ArcanumError::Storage(format!("set policy: {}", e)))?;
            }
        }
        Ok(())
    }

    #[instrument(skip(self), fields(store = "sqlite_version"), err)]
    async fn delete_by_source_uri(&self, collection_id: &str, source_uri: &str) -> Result<()> {
        sqlx::query(
            r#"DELETE FROM document_versions
               WHERE document_id IN (
                   SELECT document_id FROM source_documents
                   WHERE source_uri = $1 AND collection_id = $2
               )"#
        )
        .bind(source_uri).bind(collection_id)
        .execute(&self.pool).await
        .map_err(|e| ArcanumError::Storage(format!("delete by source_uri: {}", e)))?;
        Ok(())
    }

    #[instrument(skip(self), fields(store = "sqlite_version"), err)]
    async fn list_collections(&self) -> Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT collection_id FROM source_documents ORDER BY collection_id"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("list_collections: {}", e)))?;

        Ok(rows.into_iter().map(|(col,)| col).collect())
    }

    #[instrument(skip(self), fields(store = "sqlite_version", doc_id = %document_id.0, version = version_num), err)]
    async fn get_version(
        &self,
        document_id: &DocumentId,
        version_num: u32,
    ) -> Result<Option<DocumentVersion>> {
        let doc_id_str = document_id.0.to_string();
        let vnum = version_num as i64;

        let row = sqlx::query_as::<_, SqVersionRow>(
            "SELECT document_id, version_num, content_hash, snapshot_uri, canonical_uri, mime_type, status, ingested_at, extra FROM document_versions WHERE document_id = $1 AND version_num = $2"
        )
        .bind(&doc_id_str).bind(vnum)
        .fetch_optional(&self.pool).await
        .map_err(|e| ArcanumError::Storage(format!("get_version sqlite: {}", e)))?;

        let Some(r) = row else { return Ok(None) };

        let sd: Option<(String, String,)> = sqlx::query_as(
            "SELECT source_uri, collection_id FROM source_documents WHERE document_id = $1"
        )
        .bind(&doc_id_str)
        .fetch_optional(&self.pool).await
        .map_err(|e| ArcanumError::Storage(format!("get_version source_doc: {}", e)))?;

        let (source_uri, collection_id) = sd.unwrap_or_default();
        row_to_version(r, &source_uri, &collection_id).map(Some)
    }

    #[instrument(skip(self), fields(store = "sqlite_version", collection = collection_id), err)]
    async fn list_documents(
        &self,
        collection_id: &str,
    ) -> Result<Vec<DocumentEntry>> {
        #[derive(sqlx::FromRow)]
        struct ListRow {
            source_uri:  String,
            ingested_at: DateTime<Utc>,
        }

        let rows = sqlx::query_as::<_, ListRow>(
            r#"SELECT sd.source_uri, dv.ingested_at
               FROM source_documents sd
               JOIN (
                   SELECT document_id, MAX(ingested_at) AS ingested_at
                   FROM document_versions
                   WHERE status = 'active'
                   GROUP BY document_id
               ) dv ON dv.document_id = sd.document_id
               WHERE sd.collection_id = $1
               ORDER BY dv.ingested_at DESC"#,
        )
        .bind(collection_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("list_documents: {}", e)))?;

        let entries = rows
            .into_iter()
            .map(|r| DocumentEntry {
                source_uri: r.source_uri,
                registered_at: r.ingested_at.timestamp(),
            })
            .collect();

        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arcanum_core::types::VersionStatus;
    use std::collections::HashMap;

    async fn make_store() -> SqliteDocumentVersionStore {
        SqliteDocumentVersionStore::open(":memory:").await.unwrap()
    }

    #[tokio::test]
    async fn test_add_and_get_latest() {
        let store = make_store().await;
        let doc_id = DocumentId::new();
        let version = DocumentVersion {
            document_id:   doc_id.clone(),
            version_num:   1,
            source_uri:    "file://test.md".into(),
            collection_id: "col-a".into(),
            content_hash:  "sha256-abc".into(),
            snapshot_uri:  "file:///snap/1.raw".into(),
            canonical_uri: None,
            mime_type:     "text/markdown".into(),
            status:        VersionStatus::Active,
            ingested_at:   chrono::Utc::now(),
            extra:         HashMap::new(),
        };
        store.add_version(version).await.unwrap();

        let latest = store.get_latest("file://test.md", "col-a").await.unwrap().unwrap();
        assert_eq!(latest.version_num, 1);
        assert_eq!(latest.content_hash, "sha256-abc");
        assert_eq!(latest.source_uri, "file://test.md");
        assert_eq!(latest.collection_id, "col-a");
    }

    #[tokio::test]
    async fn test_re_ingestion_does_not_crash() {
        let store = make_store().await;
        let doc_id = DocumentId::new();
        let base = DocumentVersion {
            document_id:   doc_id.clone(),
            version_num:   1,
            source_uri:    "file://re-ingest.md".into(),
            collection_id: "col-b".into(),
            content_hash:  "hash-v1".into(),
            snapshot_uri:  "file:///snap/v1.raw".into(),
            canonical_uri: None,
            mime_type:     "text/plain".into(),
            status:        VersionStatus::Active,
            ingested_at:   chrono::Utc::now(),
            extra:         HashMap::new(),
        };
        store.add_version(base).await.unwrap();

        let v2 = DocumentVersion {
            version_num:  2,
            content_hash: "hash-v2".into(),
            snapshot_uri: "file:///snap/v2.raw".into(),
            ..store.get_latest("file://re-ingest.md", "col-b").await.unwrap().unwrap()
        };
        store.add_version(v2).await.unwrap();

        let latest = store.get_latest("file://re-ingest.md", "col-b").await.unwrap().unwrap();
        assert_eq!(latest.version_num, 2);
    }

    #[tokio::test]
    async fn test_supersede_and_list_versions() {
        let store = make_store().await;
        let doc_id = DocumentId::new();
        for v in 1u32..=2 {
            store.add_version(DocumentVersion {
                document_id:   doc_id.clone(),
                version_num:   v,
                source_uri:    "file://supersede.md".into(),
                collection_id: "col-c".into(),
                content_hash:  format!("hash-v{}", v),
                snapshot_uri:  format!("file:///snap/v{}.raw", v),
                canonical_uri: None,
                mime_type:     "text/plain".into(),
                status:        VersionStatus::Active,
                ingested_at:   chrono::Utc::now(),
                extra:         HashMap::new(),
            }).await.unwrap();
        }

        store.supersede_active(&doc_id).await.unwrap();
        let versions = store.list_versions(&doc_id).await.unwrap();
        assert_eq!(versions.len(), 2);
        assert!(versions.iter().all(|v| v.status == VersionStatus::Superseded));
    }

    #[tokio::test]
    async fn test_delete_by_source_uri() {
        let store = make_store().await;
        store.add_version(DocumentVersion {
            document_id:   DocumentId::new(),
            version_num:   1,
            source_uri:    "file://delete-me.md".into(),
            collection_id: "col-d".into(),
            content_hash:  "hash".into(),
            snapshot_uri:  "file:///snap.raw".into(),
            canonical_uri: None,
            mime_type:     "text/plain".into(),
            status:        VersionStatus::Active,
            ingested_at:   chrono::Utc::now(),
            extra:         HashMap::new(),
        }).await.unwrap();

        store.delete_by_source_uri("col-d", "file://delete-me.md").await.unwrap();
        let result = store.get_latest("file://delete-me.md", "col-d").await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_get_version_returns_added_version() {
        let store = make_store().await;
        let doc_id = DocumentId::new();
        let version = DocumentVersion {
            document_id:   doc_id.clone(),
            version_num:   1,
            source_uri:    "file://doc.pdf".into(),
            collection_id: "col".into(),
            content_hash:  "abc123".into(),
            snapshot_uri:  "file:///snapshots/doc/1.raw".into(),
            canonical_uri: None,
            mime_type:     "application/pdf".into(),
            status:        VersionStatus::Active,
            ingested_at:   chrono::Utc::now(),
            extra:         Default::default(),
        };
        store.add_version(version).await.unwrap();
        let found = store.get_version(&doc_id, 1).await.unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().content_hash, "abc123");
    }

    #[tokio::test]
    async fn test_get_version_returns_none_for_missing() {
        let store = make_store().await;
        let found = store.get_version(&DocumentId::new(), 99).await.unwrap();
        assert!(found.is_none());
    }

    #[tokio::test]
    async fn test_list_documents_returns_active_documents() {
        let store = make_store().await;

        // Add 3 documents
        for (i, uri) in ["file://a.md", "file://b.md", "file://c.md"].iter().enumerate() {
            store.add_version(DocumentVersion {
                document_id:   DocumentId::new(),
                version_num:   1,
                source_uri:    uri.clone().into(),
                collection_id: "test-list".into(),
                content_hash:  format!("hash-{}", i),
                snapshot_uri:  format!("file:///snap/{}.raw", i),
                canonical_uri: None,
                mime_type:     "text/markdown".into(),
                status:        VersionStatus::Active,
                ingested_at:   chrono::Utc::now(),
                extra:         HashMap::new(),
            }).await.unwrap();
        }

        let docs = store.list_documents("test-list").await.unwrap();
        assert_eq!(docs.len(), 3, "should list 3 active documents");
        let uris: Vec<_> = docs.iter().map(|d| d.source_uri.as_str()).collect();
        assert!(uris.contains(&"file://a.md"));
        assert!(uris.contains(&"file://b.md"));
        assert!(uris.contains(&"file://c.md"));

        // Supersede one document — it should no longer appear in list
        let doc_id: Option<(String,)> = sqlx::query_as(
            "SELECT document_id FROM source_documents WHERE source_uri = $1 AND collection_id = $2"
        )
        .bind("file://a.md").bind("test-list")
        .fetch_optional(&store.pool).await.unwrap();
        if let Some((id,)) = doc_id {
            store.supersede_active(&DocumentId(uuid::Uuid::parse_str(&id).unwrap())).await.unwrap();
        }

        let docs = store.list_documents("test-list").await.unwrap();
        assert_eq!(docs.len(), 2, "should list 2 after superseding one");
    }

    #[tokio::test]
    async fn test_list_documents_empty_collection() {
        let store = make_store().await;
        let docs = store.list_documents("nonexistent").await.unwrap();
        assert!(docs.is_empty(), "empty collection should return empty list");
    }
}
