use arcanum_core::{
    traits::{ScoredChunk, VectorQuery, VectorStore},
    types::*,
    ArcanumError, Result,
};
use async_trait::async_trait;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tracing::instrument;

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

        sqlx::query(r#"
            CREATE TABLE IF NOT EXISTS arcanum_vector_collections (
                name TEXT PRIMARY KEY
            )
        "#)
        .execute(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("ensure_schema collections: {}", e)))?;

        sqlx::query(
            "ALTER TABLE arcanum_chunks \
             ADD COLUMN IF NOT EXISTS source_uri TEXT NOT NULL DEFAULT ''",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("ensure_schema source_uri column: {}", e)))?;

        sqlx::query(
            "CREATE INDEX IF NOT EXISTS arcanum_chunks_source_uri_idx \
             ON arcanum_chunks (collection, source_uri)",
        )
        .execute(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("ensure_schema source_uri index: {}", e)))?;

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
    #[instrument(skip(self, chunks), fields(store = "pgvector", collection_id = collection, chunk_count = chunks.len()), err)]
    async fn upsert(&self, collection: &str, chunks: Vec<IndexedChunk>) -> Result<()> {
        for chunk in &chunks {
            let id = chunk.chunk.id.0.to_string();
            let json = serde_json::to_string(chunk)
                .map_err(|e| ArcanumError::Storage(e.to_string()))?;
            let vec_literal = Self::vector_to_pg_literal(&chunk.vector);
            let source_uri = chunk.chunk.metadata.source_uri();

            sqlx::query(
                "INSERT INTO arcanum_chunks (id, collection, chunk_json, embedding, source_uri)
                 VALUES ($1, $2, $3, $4::vector, $5)
                 ON CONFLICT (collection, id)
                 DO UPDATE SET chunk_json  = EXCLUDED.chunk_json,
                               embedding   = EXCLUDED.embedding,
                               source_uri  = EXCLUDED.source_uri",
            )
            .bind(&id)
            .bind(collection)
            .bind(&json)
            .bind(&vec_literal)
            .bind(&source_uri)
            .execute(&self.pool)
            .await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        }
        Ok(())
    }

    #[instrument(skip(self, query), fields(store = "pgvector", collection_id = collection, top_k = query.top_k), err)]
    async fn search(&self, collection: &str, query: &VectorQuery) -> Result<Vec<ScoredChunk>> {
        let vec_literal = Self::vector_to_pg_literal(&query.vector);

        let mut source_uri_filter: Option<String> = None;
        for f in &query.filters {
            if f.field == "source_uri" {
                if !matches!(f.op, FilterOp::Eq) {
                    tracing::warn!(
                        store = "pgvector",
                        op = ?f.op,
                        "unsupported filter op for source_uri (only Eq is supported) — ignoring"
                    );
                } else if let Some(s) = f.value.as_str() {
                    source_uri_filter = Some(s.to_string());
                } else {
                    tracing::warn!(
                        store = "pgvector",
                        "source_uri filter value is not a string — ignoring"
                    );
                }
            } else {
                tracing::warn!(
                    store = "pgvector",
                    field = %f.field,
                    "unsupported metadata filter field — ignoring"
                );
            }
        }

        let rows = if let Some(ref uri) = source_uri_filter {
            sqlx::query(
                "SELECT chunk_json, 1 - (embedding <=> $1::vector) AS score
                 FROM arcanum_chunks
                 WHERE collection = $2 AND source_uri = $3
                 ORDER BY embedding <=> $1::vector
                 LIMIT $4",
            )
            .bind(&vec_literal)
            .bind(collection)
            .bind(uri)
            .bind(query.top_k as i64)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?
        } else {
            sqlx::query(
                "SELECT chunk_json, 1 - (embedding <=> $1::vector) AS score
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
            .map_err(|e| ArcanumError::Storage(e.to_string()))?
        };

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

    #[instrument(skip(self, ids), fields(store = "pgvector", collection_id = collection), err)]
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

    #[instrument(skip(self), fields(store = "pgvector", collection_id = collection), err)]
    async fn delete_by_source_uri(&self, collection: &str, source_uri: &str) -> Result<()> {
        if source_uri.is_empty() {
            tracing::warn!(store = "pgvector", "delete_by_source_uri called with empty source_uri — skipping");
            return Ok(());
        }
        sqlx::query(
            "DELETE FROM arcanum_chunks \
             WHERE collection = $1 AND source_uri = $2",
        )
        .bind(collection)
        .bind(source_uri)
        .execute(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        Ok(())
    }

    #[instrument(skip(self), fields(store = "pgvector", collection_id = collection), err)]
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

    #[instrument(skip(self), fields(store = "pgvector"), err)]
    async fn list_collections(&self) -> Result<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT name FROM arcanum_vector_collections ORDER BY name",
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("list_collections: {}", e)))?;
        Ok(rows.into_iter().map(|(name,)| name).collect())
    }

    #[instrument(skip(self), fields(store = "pgvector", collection = collection), err)]
    async fn create_collection(&self, collection: &str) -> Result<()> {
        let result = sqlx::query(
            "INSERT INTO arcanum_vector_collections (name) VALUES ($1) ON CONFLICT (name) DO NOTHING",
        )
        .bind(collection)
        .execute(&self.pool)
        .await
        .map_err(|e| ArcanumError::Storage(format!("create_collection: {}", e)))?;

        if result.rows_affected() == 0 {
            return Err(ArcanumError::AlreadyExists(
                format!("collection '{}' already exists", collection),
            ));
        }
        Ok(())
    }

    #[instrument(skip(self), fields(store = "pgvector", collection = ?collection), err)]
    async fn count_documents(&self, collection: Option<&str>) -> Result<u64> {
        let count: i64 = match collection {
            Some(col) => sqlx::query_scalar(
                "SELECT COUNT(DISTINCT source_uri) \
                 FROM arcanum_chunks WHERE collection = $1 AND source_uri <> ''",
            )
            .bind(col)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| ArcanumError::Storage(format!("count_documents: {}", e)))?,
            None => sqlx::query_scalar(
                "SELECT COUNT(DISTINCT source_uri) FROM arcanum_chunks WHERE source_uri <> ''",
            )
            .fetch_one(&self.pool)
            .await
            .map_err(|e| ArcanumError::Storage(format!("count_documents: {}", e)))?,
        };
        Ok(count as u64)
    }

    #[instrument(skip(self), fields(store = "pgvector", collection = collection), err)]
    async fn delete_collection(&self, collection: &str) -> Result<()> {
        sqlx::query("DELETE FROM arcanum_chunks WHERE collection = $1")
            .bind(collection)
            .execute(&self.pool)
            .await
            .map_err(|e| ArcanumError::Storage(format!("delete_collection chunks: {}", e)))?;
        sqlx::query("DELETE FROM arcanum_vector_collections WHERE name = $1")
            .bind(collection)
            .execute(&self.pool)
            .await
            .map_err(|e| ArcanumError::Storage(format!("delete_collection registry: {}", e)))?;
        Ok(())
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

    #[tokio::test]
    #[ignore]
    async fn test_pg_source_uri_column() {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let store = PgVectorStore::new(&url, 3).await.unwrap();

        let mut meta = std::collections::HashMap::new();
        meta.insert("source_uri".to_string(), serde_json::json!("file:///doc-a.pdf"));
        let chunk = IndexedChunk {
            chunk: Chunk {
                id: ChunkId::new(),
                text: "content a".into(),
                document_id: DocumentId::new(),
                collection_id: CollectionId("src_uri_col_test".into()),
                position: ChunkPosition { start: 0, end: 9, index: 0 },
                metadata: ChunkMetadata(meta),
            },
            vector: Vector(vec![0.1, 0.2, 0.3]),
            token_vectors: None,
            store_id: String::new(),
        };

        store.upsert("src_uri_col_test", vec![chunk]).await.unwrap();
        let count = store.count_documents(Some("src_uri_col_test")).await.unwrap();
        assert_eq!(count, 1, "expected 1 distinct source_uri");

        // Cleanup
        store.delete_collection("src_uri_col_test").await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_pg_delete_by_source_uri() {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let store = PgVectorStore::new(&url, 3).await.unwrap();

        let mut meta_a = std::collections::HashMap::new();
        meta_a.insert("source_uri".to_string(), serde_json::json!("file:///doc-a.pdf"));
        let chunk_a = IndexedChunk {
            chunk: Chunk {
                id: ChunkId::new(),
                text: "doc a".into(),
                document_id: DocumentId::new(),
                collection_id: CollectionId("del_uri_test".into()),
                position: ChunkPosition { start: 0, end: 5, index: 0 },
                metadata: ChunkMetadata(meta_a),
            },
            vector: Vector(vec![0.1, 0.2, 0.3]),
            token_vectors: None,
            store_id: String::new(),
        };

        let mut meta_b = std::collections::HashMap::new();
        meta_b.insert("source_uri".to_string(), serde_json::json!("file:///doc-b.pdf"));
        let chunk_b = IndexedChunk {
            chunk: Chunk {
                id: ChunkId::new(),
                text: "doc b".into(),
                document_id: DocumentId::new(),
                collection_id: CollectionId("del_uri_test".into()),
                position: ChunkPosition { start: 0, end: 5, index: 0 },
                metadata: ChunkMetadata(meta_b),
            },
            vector: Vector(vec![0.4, 0.5, 0.6]),
            token_vectors: None,
            store_id: String::new(),
        };

        store.upsert("del_uri_test", vec![chunk_a, chunk_b]).await.unwrap();
        assert_eq!(store.count_documents(Some("del_uri_test")).await.unwrap(), 2);

        store.delete_by_source_uri("del_uri_test", "file:///doc-a.pdf").await.unwrap();
        assert_eq!(store.count_documents(Some("del_uri_test")).await.unwrap(), 1,
            "only doc-b should remain");

        // Cleanup
        store.delete_collection("del_uri_test").await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_pg_search_source_uri_filter() {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let store = PgVectorStore::new(&url, 3).await.unwrap();

        let mut meta_a = std::collections::HashMap::new();
        meta_a.insert("source_uri".to_string(), serde_json::json!("file:///filter-a.pdf"));
        let chunk_a = IndexedChunk {
            chunk: Chunk {
                id: ChunkId::new(),
                text: "doc a".into(),
                document_id: DocumentId::new(),
                collection_id: CollectionId("filter_test".into()),
                position: ChunkPosition { start: 0, end: 5, index: 0 },
                metadata: ChunkMetadata(meta_a),
            },
            vector: Vector(vec![1.0, 0.0, 0.0]),
            token_vectors: None,
            store_id: String::new(),
        };

        let mut meta_b = std::collections::HashMap::new();
        meta_b.insert("source_uri".to_string(), serde_json::json!("file:///filter-b.pdf"));
        let chunk_b = IndexedChunk {
            chunk: Chunk {
                id: ChunkId::new(),
                text: "doc b".into(),
                document_id: DocumentId::new(),
                collection_id: CollectionId("filter_test".into()),
                position: ChunkPosition { start: 0, end: 5, index: 0 },
                metadata: ChunkMetadata(meta_b),
            },
            vector: Vector(vec![0.0, 1.0, 0.0]),
            token_vectors: None,
            store_id: String::new(),
        };

        store.upsert("filter_test", vec![chunk_a, chunk_b]).await.unwrap();

        let results = store.search("filter_test", &VectorQuery {
            vector: Vector(vec![1.0, 0.0, 0.0]),
            top_k: 10,
            filters: vec![MetadataFilter {
                field: "source_uri".into(),
                op: FilterOp::Eq,
                value: serde_json::json!("file:///filter-a.pdf"),
            }],
        }).await.unwrap();

        assert_eq!(results.len(), 1, "filter should return only doc-a");
        let uri = results[0].chunk.chunk.metadata.0
            .get("source_uri").and_then(|v| v.as_str()).unwrap_or("");
        assert_eq!(uri, "file:///filter-a.pdf");

        // Cleanup
        store.delete_collection("filter_test").await.unwrap();
    }

    #[tokio::test]
    #[ignore]
    async fn test_pg_count_documents_excludes_empty_source_uri() {
        let url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
        let store = PgVectorStore::new(&url, 3).await.unwrap();

        // Chunk with NO source_uri in metadata — will be stored as source_uri = ''
        let chunk = IndexedChunk {
            chunk: Chunk {
                id: ChunkId::new(),
                text: "no-uri chunk".into(),
                document_id: DocumentId::new(),
                collection_id: CollectionId("count_empty_test".into()),
                position: ChunkPosition { start: 0, end: 12, index: 0 },
                metadata: ChunkMetadata::default(),  // no source_uri key
            },
            vector: Vector(vec![0.1, 0.2, 0.3]),
            token_vectors: None,
            store_id: String::new(),
        };

        store.upsert("count_empty_test", vec![chunk]).await.unwrap();
        let count = store.count_documents(Some("count_empty_test")).await.unwrap();
        assert_eq!(count, 0, "chunks with empty source_uri must not be counted as a document");

        store.delete_collection("count_empty_test").await.unwrap();
    }

}
