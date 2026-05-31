use arcanum_core::{
    traits::{ScoredChunk, VectorQuery, VectorStore},
    types::*,
    ArcanumError, Result,
};
use async_trait::async_trait;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

pub struct PgVectorStore {
    pool: PgPool,
    dimension: usize,
}

impl PgVectorStore {
    pub async fn new(database_url: &str, dimension: usize) -> Result<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(database_url)
            .await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        let store = Self { pool, dimension };
        store.ensure_schema().await?;
        Ok(store)
    }

    async fn ensure_schema(&self) -> Result<()> {
        sqlx::query("CREATE EXTENSION IF NOT EXISTS vector")
            .execute(&self.pool)
            .await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;

        sqlx::query(&format!(
            "CREATE TABLE IF NOT EXISTS arcanum_chunks (
                id          TEXT NOT NULL,
                collection  TEXT NOT NULL,
                chunk_json  TEXT NOT NULL,
                embedding   vector({dim}),
                PRIMARY KEY (collection, id)
            )",
            dim = self.dimension
        ))
        .execute(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(e.to_string()))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS arcanum_chunks_embedding_idx
             ON arcanum_chunks
             USING ivfflat (embedding vector_cosine_ops)",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(e.to_string()))?;

        Ok(())
    }

    /// Encodes a `Vector` as a PostgreSQL literal, e.g. `[1,2,3]`.
    pub fn vector_to_pg_literal(v: &Vector) -> String {
        let inner = v
            .0
            .iter()
            .map(|f| {
                // Strip trailing zeros for a compact representation.
                let s = format!("{}", f);
                s
            })
            .collect::<Vec<_>>()
            .join(",");
        format!("[{}]", inner)
    }
}

#[async_trait]
impl VectorStore for PgVectorStore {
    async fn upsert(&self, collection: &str, chunks: Vec<IndexedChunk>) -> Result<()> {
        for chunk in &chunks {
            let id = chunk.chunk.id.0.to_string();
            let json = serde_json::to_string(chunk)
                .map_err(|e| ArcanumError::Storage(e.to_string()))?;
            let vec_literal = Self::vector_to_pg_literal(&chunk.vector);

            sqlx::query(
                "INSERT INTO arcanum_chunks (id, collection, chunk_json, embedding)
                 VALUES ($1, $2, $3, $4::vector)
                 ON CONFLICT (collection, id)
                 DO UPDATE SET chunk_json = EXCLUDED.chunk_json,
                               embedding  = EXCLUDED.embedding",
            )
            .bind(&id)
            .bind(collection)
            .bind(&json)
            .bind(&vec_literal)
            .execute(&self.pool)
            .await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        }
        Ok(())
    }

    async fn search(&self, collection: &str, query: &VectorQuery) -> Result<Vec<ScoredChunk>> {
        let vec_literal = Self::vector_to_pg_literal(&query.vector);

        let rows = sqlx::query(
            "SELECT chunk_json,
                    1 - (embedding <=> $1::vector) AS score
             FROM arcanum_chunks
             WHERE collection = $2
             ORDER BY embedding <=> $1::vector
             LIMIT $3",
        )
        .bind(&vec_literal)
        .bind(collection)
        .bind(query.top_k as i64)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(e.to_string()))?;

        let mut results = Vec::with_capacity(rows.len());
        for row in rows {
            use sqlx::Row;
            let json: String = row
                .try_get("chunk_json")
                .map_err(|e| ArcanumError::Storage(e.to_string()))?;
            let score: f64 = row
                .try_get("score")
                .map_err(|e| ArcanumError::Storage(e.to_string()))?;
            let chunk: IndexedChunk = serde_json::from_str(&json)
                .map_err(|e| ArcanumError::Storage(e.to_string()))?;
            results.push(ScoredChunk {
                chunk,
                score: score as f32,
            });
        }
        Ok(results)
    }

    async fn delete(&self, collection: &str, ids: &[ChunkId]) -> Result<()> {
        for id in ids {
            sqlx::query(
                "DELETE FROM arcanum_chunks WHERE collection = $1 AND id = $2",
            )
            .bind(collection)
            .bind(id.0.to_string())
            .execute(&self.pool)
            .await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        }
        Ok(())
    }

    async fn collection_exists(&self, collection: &str) -> Result<bool> {
        use sqlx::Row;
        let row = sqlx::query(
            "SELECT COUNT(*) AS cnt FROM arcanum_chunks WHERE collection = $1 LIMIT 1",
        )
        .bind(collection)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(e.to_string()))?;

        let cnt: i64 = row
            .try_get("cnt")
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        Ok(cnt > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_to_pg_literal() {
        let v = Vector(vec![1.0, 2.0, 3.0]);
        let lit = PgVectorStore::vector_to_pg_literal(&v);
        assert_eq!(lit, "[1,2,3]");
    }

    // Integration tests below require a running PostgreSQL instance with pgvector.
    // Run with: DATABASE_URL=postgres://... cargo test -p arcanum-vector -- --ignored

    #[tokio::test]
    #[ignore]
    async fn test_pg_upsert_and_search() {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let store = PgVectorStore::new(&url, 3).await.unwrap();

        let chunk = IndexedChunk {
            chunk: Chunk {
                id: ChunkId::new(),
                text: "hello pgvector".into(),
                document_id: DocumentId::new(),
                collection_id: CollectionId("test".into()),
                position: ChunkPosition { start: 0, end: 14, index: 0 },
                metadata: ChunkMetadata::default(),
            },
            vector: Vector(vec![0.1, 0.2, 0.3]),
            token_vectors: None,
            store_id: String::new(),
        };

        store.upsert("test", vec![chunk]).await.unwrap();

        let results = store
            .search(
                "test",
                &VectorQuery {
                    vector: Vector(vec![0.1, 0.2, 0.3]),
                    top_k: 5,
                    filters: vec![],
                },
            )
            .await
            .unwrap();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    #[ignore]
    async fn test_pg_delete() {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let store = PgVectorStore::new(&url, 3).await.unwrap();

        let id = ChunkId::new();
        let chunk = IndexedChunk {
            chunk: Chunk {
                id: id.clone(),
                text: "to be deleted".into(),
                document_id: DocumentId::new(),
                collection_id: CollectionId("del_test".into()),
                position: ChunkPosition { start: 0, end: 13, index: 0 },
                metadata: ChunkMetadata::default(),
            },
            vector: Vector(vec![0.5, 0.5, 0.5]),
            token_vectors: None,
            store_id: String::new(),
        };

        store.upsert("del_test", vec![chunk]).await.unwrap();
        store.delete("del_test", &[id]).await.unwrap();

        let results = store
            .search(
                "del_test",
                &VectorQuery {
                    vector: Vector(vec![0.5, 0.5, 0.5]),
                    top_k: 5,
                    filters: vec![],
                },
            )
            .await
            .unwrap();
        assert!(results.is_empty());
    }
}
