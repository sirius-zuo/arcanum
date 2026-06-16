use arcanum_core::{
    traits::ChunkMetadataStore,
    types::{ChunkId, ChunkMetadataRecord, DocumentId},
    ArcanumError, Result,
};
use async_trait::async_trait;
use chrono::Utc;
use sqlx::PgPool;
use tracing::instrument;

#[derive(sqlx::FromRow)]
struct ChunkMetadataRow {
    chunk_id:        uuid::Uuid,
    document_id:     uuid::Uuid,
    collection_id:   String,
    version_num:     i32,
    source_uri:      String,
    snapshot_uri:    String,
    canonical_uri:   Option<String>,
    page:            Option<i32>,
    section:         Option<String>,
    block_ids:       serde_json::Value,
    offset_start:    i32,
    offset_end:      i32,
    ingested_at:     chrono::DateTime<Utc>,
}

pub struct PostgresChunkMetadataStore {
    pool: PgPool,
}

impl PostgresChunkMetadataStore {
    pub async fn new(database_url: &str) -> Result<Self> {
        let pool = PgPool::connect(database_url).await
            .map_err(|e| ArcanumError::Storage(format!("PostgresChunkMetadataStore connect: {}", e)))?;
        let store = Self { pool };
        store.ensure_schema().await?;
        Ok(store)
    }

    async fn ensure_schema(&self) -> Result<()> {
        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS chunk_metadata (
                chunk_id      UUID        PRIMARY KEY,
                document_id   UUID        NOT NULL,
                collection_id TEXT        NOT NULL,
                version_num   INTEGER     NOT NULL,
                source_uri    TEXT        NOT NULL,
                snapshot_uri  TEXT        NOT NULL,
                canonical_uri TEXT,
                page          INTEGER,
                section       TEXT,
                block_ids     JSONB       NOT NULL DEFAULT '[]',
                offset_start  INTEGER     NOT NULL,
                offset_end    INTEGER     NOT NULL,
                ingested_at   TIMESTAMPTZ NOT NULL DEFAULT NOW()
            )
        "#).execute(&self.pool).await
            .map_err(|e| ArcanumError::Storage(format!("ensure chunk_metadata: {}", e)))?;

        sqlx::query("CREATE INDEX IF NOT EXISTS idx_chunk_meta_doc_ver ON chunk_metadata (document_id, version_num)")
            .execute(&self.pool).await.ok();
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_chunk_meta_col_uri ON chunk_metadata (collection_id, source_uri)")
            .execute(&self.pool).await.ok();

        Ok(())
    }
}

#[async_trait]
impl ChunkMetadataStore for PostgresChunkMetadataStore {
    #[instrument(skip(self, record), fields(store = "postgres_chunk_meta", chunk_id = %record.chunk_id.0), err)]
    async fn put(&self, record: &ChunkMetadataRecord) -> Result<()> {
        let block_ids = serde_json::to_value(&record.block_ids)
            .map_err(|e| ArcanumError::Storage(format!("serialize block_ids: {}", e)))?;
        sqlx::query(
            r#"INSERT INTO chunk_metadata
               (chunk_id, document_id, collection_id, version_num, source_uri, snapshot_uri,
                canonical_uri, page, section, block_ids, offset_start, offset_end, ingested_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)
               ON CONFLICT (chunk_id) DO UPDATE SET
                 snapshot_uri  = EXCLUDED.snapshot_uri,
                 canonical_uri = EXCLUDED.canonical_uri,
                 block_ids     = EXCLUDED.block_ids,
                 ingested_at   = EXCLUDED.ingested_at"#,
        )
        .bind(record.chunk_id.0)
        .bind(record.document_id.0)
        .bind(&record.collection_id)
        .bind(record.version_num as i32)
        .bind(&record.source_uri)
        .bind(&record.snapshot_uri)
        .bind(&record.canonical_uri)
        .bind(record.page.map(|p| p as i32))
        .bind(&record.section)
        .bind(&block_ids)
        .bind(record.offset_start as i32)
        .bind(record.offset_end as i32)
        .bind(record.ingested_at)
        .execute(&self.pool).await
        .map_err(|e| ArcanumError::Storage(format!("put chunk_metadata: {}", e)))?;
        Ok(())
    }

    #[instrument(skip(self), fields(store = "postgres_chunk_meta", chunk_id = %chunk_id.0), err)]
    async fn get(&self, chunk_id: &ChunkId) -> Result<Option<ChunkMetadataRecord>> {
        let row = sqlx::query_as::<_, ChunkMetadataRow>(
            r#"SELECT chunk_id, document_id, collection_id, version_num, source_uri, snapshot_uri,
                      canonical_uri, page, section, block_ids, offset_start, offset_end, ingested_at
               FROM chunk_metadata WHERE chunk_id = $1"#,
        )
        .bind(chunk_id.0)
        .fetch_optional(&self.pool).await
        .map_err(|e| ArcanumError::Storage(format!("get chunk_metadata: {}", e)))?;

        let Some(r) = row else { return Ok(None) };

        let block_ids: Vec<String> = serde_json::from_value(r.block_ids)
            .map_err(|e| ArcanumError::Storage(format!("deserialize block_ids: {}", e)))?;

        Ok(Some(ChunkMetadataRecord {
            chunk_id:      ChunkId(r.chunk_id),
            document_id:   DocumentId(r.document_id),
            collection_id: r.collection_id,
            version_num:   r.version_num as u32,
            source_uri:    r.source_uri,
            snapshot_uri:  r.snapshot_uri,
            canonical_uri: r.canonical_uri,
            page:          r.page.map(|p| p as u32),
            section:       r.section,
            block_ids,
            offset_start:  r.offset_start as usize,
            offset_end:    r.offset_end as usize,
            ingested_at:   r.ingested_at,
        }))
    }

    #[instrument(skip(self), fields(store = "postgres_chunk_meta", collection_id, source_uri), err)]
    async fn delete_by_source_uri(&self, collection_id: &str, source_uri: &str) -> Result<()> {
        sqlx::query(
            "DELETE FROM chunk_metadata WHERE collection_id = $1 AND source_uri = $2",
        )
        .bind(collection_id)
        .bind(source_uri)
        .execute(&self.pool).await
        .map_err(|e| ArcanumError::Storage(format!("delete chunk_metadata: {}", e)))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "requires Postgres — set TEST_DATABASE_URL"]
    async fn test_put_and_get_chunk_metadata() {
        let url = std::env::var("TEST_DATABASE_URL").unwrap();
        let store = PostgresChunkMetadataStore::new(&url).await.unwrap();
        let record = ChunkMetadataRecord {
            chunk_id:      ChunkId::new(),
            document_id:   DocumentId::new(),
            collection_id: "test_col".into(),
            version_num:   1,
            source_uri:    "file://doc.pdf".into(),
            snapshot_uri:  "file:///snapshots/doc/1.raw".into(),
            canonical_uri: None,
            page:          Some(3),
            section:       Some("§3.2".into()),
            block_ids:     vec!["b1".into()],
            offset_start:  100,
            offset_end:    200,
            ingested_at:   Utc::now(),
        };
        let chunk_id = record.chunk_id.clone();
        store.put(&record).await.unwrap();
        let found = store.get(&chunk_id).await.unwrap().unwrap();
        assert_eq!(found.source_uri, "file://doc.pdf");
        assert_eq!(found.page, Some(3));
        assert_eq!(found.block_ids, vec!["b1"]);
        assert_eq!(found.offset_start, 100);
    }

    #[tokio::test]
    #[ignore = "requires Postgres — set TEST_DATABASE_URL"]
    async fn test_delete_by_source_uri() {
        let url = std::env::var("TEST_DATABASE_URL").unwrap();
        let store = PostgresChunkMetadataStore::new(&url).await.unwrap();
        let record = ChunkMetadataRecord {
            chunk_id:      ChunkId::new(),
            document_id:   DocumentId::new(),
            collection_id: "test_col".into(),
            version_num:   1,
            source_uri:    "file://to_delete.pdf".into(),
            snapshot_uri:  "file:///snapshots/d/1.raw".into(),
            canonical_uri: None,
            page:          None,
            section:       None,
            block_ids:     vec![],
            offset_start:  0,
            offset_end:    10,
            ingested_at:   Utc::now(),
        };
        let chunk_id = record.chunk_id.clone();
        store.put(&record).await.unwrap();
        store.delete_by_source_uri("test_col", "file://to_delete.pdf").await.unwrap();
        assert!(store.get(&chunk_id).await.unwrap().is_none());
    }
}
