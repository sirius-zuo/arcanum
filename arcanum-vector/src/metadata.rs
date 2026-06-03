use sqlx::{SqlitePool, Row};
use arcanum_core::{types::*, Result, ArcanumError};
use tracing::instrument;

pub struct SqliteMetadataStore {
    pool: SqlitePool,
}

impl SqliteMetadataStore {
    pub async fn new(db_url: &str) -> Result<Self> {
        let pool = SqlitePool::connect(db_url).await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        let store = Self { pool };
        store.migrate().await?;
        Ok(store)
    }

    pub async fn new_in_memory() -> Result<Self> {
        Self::new("sqlite::memory:").await
    }

    async fn migrate(&self) -> Result<()> {
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                source_uri TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                collection_id TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            )"
        ).execute(&self.pool).await.map_err(|e| ArcanumError::Storage(e.to_string()))?;
        Ok(())
    }

    #[instrument(skip(self), fields(doc_id = %id.0, collection_id = collection), err)]
    pub async fn record_document(
        &self, id: &DocumentId, uri: &str, hash: &str, collection: &str,
    ) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO documents (id, source_uri, content_hash, collection_id) VALUES (?, ?, ?, ?)"
        )
        .bind(id.0.to_string())
        .bind(uri)
        .bind(hash)
        .bind(collection)
        .execute(&self.pool).await
        .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        Ok(())
    }

    #[instrument(skip(self), fields(doc_id = %id.0))]
    pub async fn get_document_hash(&self, id: &DocumentId) -> Result<Option<String>> {
        let row = sqlx::query("SELECT content_hash FROM documents WHERE id = ?")
            .bind(id.0.to_string())
            .fetch_optional(&self.pool).await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        Ok(row.map(|r| r.get("content_hash")))
    }

    #[instrument(skip(self), fields(doc_id = %id.0))]
    pub async fn is_document_changed(&self, id: &DocumentId, new_hash: &str) -> Result<bool> {
        match self.get_document_hash(id).await? {
            Some(stored) => Ok(stored != new_hash),
            None => Ok(true),
        }
    }
}
