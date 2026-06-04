use arcanum_core::{
    traits::{ScoredChunk, VectorQuery, VectorStore},
    types::*,
    ArcanumError, Result,
};
use arrow_array::{
    types::Float32Type, Array, FixedSizeListArray, RecordBatch, StringArray,
};
use arrow_schema::{DataType, Field, Schema};
use async_trait::async_trait;
use futures::TryStreamExt;
use lancedb::query::{ExecutableQuery, QueryBase};
use std::sync::Arc;
use tracing::instrument;

pub struct LanceDbStore {
    uri: String,
}

impl LanceDbStore {
    pub async fn new(path: &str) -> Result<Self> {
        Ok(Self { uri: path.to_string() })
    }

    fn make_schema(dim: i32) -> Arc<Schema> {
        Arc::new(Schema::new(vec![
            Field::new("id", DataType::Utf8, false),
            Field::new("text", DataType::Utf8, false),
            Field::new("chunk_json", DataType::Utf8, false),
            Field::new(
                "vector",
                DataType::FixedSizeList(
                    Arc::new(Field::new("item", DataType::Float32, true)),
                    dim,
                ),
                false,
            ),
        ]))
    }

    fn build_batch(schema: Arc<Schema>, dim: i32, chunks: &[IndexedChunk]) -> Result<RecordBatch> {
        let id_strings: Vec<String> = chunks.iter().map(|c| c.chunk.id.0.to_string()).collect();
        let text_strings: Vec<String> = chunks.iter().map(|c| c.chunk.text.clone()).collect();
        let json_strings: Vec<String> = chunks
            .iter()
            .map(|c| serde_json::to_string(c).unwrap_or_default())
            .collect();

        let vec_array = FixedSizeListArray::from_iter_primitive::<Float32Type, _, _>(
            chunks
                .iter()
                .map(|c| Some(c.vector.0.iter().map(|&v| Some(v)).collect::<Vec<_>>())),
            dim,
        );

        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(StringArray::from(
                    id_strings.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                )),
                Arc::new(StringArray::from(
                    text_strings.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                )),
                Arc::new(StringArray::from(
                    json_strings.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
                )),
                Arc::new(vec_array),
            ],
        )
        .map_err(|e| ArcanumError::Storage(e.to_string()))
    }
}

#[async_trait]
impl VectorStore for LanceDbStore {
    #[instrument(skip(self, chunks), fields(store = "lancedb", collection_id = collection, chunk_count = chunks.len()), err)]
    async fn upsert(&self, collection: &str, chunks: Vec<IndexedChunk>) -> Result<()> {
        if chunks.is_empty() {
            return Ok(());
        }

        let dim = chunks[0].vector.0.len() as i32;
        let schema = Self::make_schema(dim);
        let batch = Self::build_batch(schema.clone(), dim, &chunks)?;

        let conn = lancedb::connect(&self.uri)
            .execute()
            .await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;

        match conn.open_table(collection).execute().await {
            Ok(table) => {
                table
                    .add(vec![batch])
                    .execute()
                    .await
                    .map_err(|e| ArcanumError::Storage(e.to_string()))?;
            }
            Err(_) => {
                conn.create_table(collection, vec![batch])
                    .execute()
                    .await
                    .map_err(|e| ArcanumError::Storage(e.to_string()))?;
            }
        }

        Ok(())
    }

    #[instrument(skip(self, query), fields(store = "lancedb", collection_id = collection, top_k = query.top_k), err)]
    async fn search(&self, collection: &str, query: &VectorQuery) -> Result<Vec<ScoredChunk>> {
        let conn = lancedb::connect(&self.uri)
            .execute()
            .await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;

        let table = match conn.open_table(collection).execute().await {
            Ok(t) => t,
            Err(_) => return Ok(vec![]),
        };

        let query_vec: Vec<f32> = query.vector.0.clone();
        let results = table
            .query()
            .nearest_to(query_vec.as_slice())
            .map_err(|e| ArcanumError::Storage(e.to_string()))?
            .limit(query.top_k)
            .execute()
            .await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;

        let batches: Vec<RecordBatch> = results
            .try_collect()
            .await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;

        let mut scored = vec![];
        for batch in batches {
            if let Some(json_col) = batch.column_by_name("chunk_json") {
                let strings = json_col
                    .as_any()
                    .downcast_ref::<StringArray>()
                    .ok_or_else(|| {
                        ArcanumError::Storage("chunk_json is not StringArray".into())
                    })?;
                for i in 0..strings.len() {
                    if let Ok(chunk) = serde_json::from_str::<IndexedChunk>(strings.value(i)) {
                        scored.push(ScoredChunk { chunk, score: 1.0 });
                    }
                }
            }
        }

        Ok(scored)
    }

    #[instrument(skip(self, ids), fields(store = "lancedb", collection_id = collection), err)]
    async fn delete(&self, collection: &str, ids: &[ChunkId]) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let conn = lancedb::connect(&self.uri)
            .execute()
            .await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;

        if let Ok(table) = conn.open_table(collection).execute().await {
            let id_list = ids
                .iter()
                .map(|id| format!("'{}'", id.0))
                .collect::<Vec<_>>()
                .join(", ");
            table
                .delete(&format!("id IN ({})", id_list))
                .await
                .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        }

        Ok(())
    }

    #[instrument(skip(self), fields(store = "lancedb", collection_id = collection), err)]
    async fn delete_by_source_uri(&self, collection: &str, source_uri: &str) -> Result<()> {
        let conn = lancedb::connect(&self.uri)
            .execute()
            .await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        let table = match conn.open_table(collection).execute().await {
            Ok(t) => t,
            Err(_) => return Ok(()),
        };
        // Filter on source_uri embedded in chunk_json. Escape LIKE wildcards and backslashes.
        let escaped = source_uri
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
            .replace('"', "\\\"");
        let predicate = format!(r#"chunk_json LIKE '%"source_uri":"{}"%' ESCAPE '\'"#, escaped);
        table
            .delete(&predicate)
            .await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        Ok(())
    }

    #[instrument(skip(self), fields(store = "lancedb", collection_id = collection), err)]
    async fn collection_exists(&self, collection: &str) -> Result<bool> {
        let conn = lancedb::connect(&self.uri)
            .execute()
            .await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;

        let table_names = conn
            .table_names()
            .execute()
            .await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;

        Ok(table_names.contains(&collection.to_string()))
    }
}
