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
use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use tracing::instrument;

pub struct LanceDbStore {
    uri: String,
    collections_file: String,
    sidecar_lock: Arc<TokioMutex<()>>,
}

impl LanceDbStore {
    pub async fn new(path: &str) -> Result<Self> {
        // Resolve to absolute path so the sidecar location is stable
        // across working-directory changes and process restarts.
        let abs = std::fs::canonicalize(path)
            .or_else(|_| {
                // Path may not exist yet (new DB). Use absolute form of the given path.
                let p = std::path::Path::new(path);
                if p.is_absolute() {
                    Ok(p.to_path_buf())
                } else {
                    std::env::current_dir().map(|d| d.join(p))
                }
            })
            .map_err(|e| ArcanumError::Storage(format!("resolve lancedb path: {}", e)))?;
        let uri = abs.to_string_lossy().to_string();
        Ok(Self {
            collections_file: format!("{}.collections.json", uri),
            uri,
            sidecar_lock: Arc::new(TokioMutex::new(())),
        })
    }

    fn load_sidecar(&self) -> Vec<String> {
        std::fs::read_to_string(&self.collections_file)
            .ok()
            .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
            .unwrap_or_default()
    }

    fn save_sidecar(&self, names: &[String]) -> Result<()> {
        let json = serde_json::to_string(names)
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        std::fs::write(&self.collections_file, json)
            .map_err(|e| ArcanumError::Storage(format!("write collections sidecar: {}", e)))?;
        Ok(())
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
        if source_uri.is_empty() {
            tracing::warn!(store = "lancedb", "delete_by_source_uri called with empty source_uri — skipping");
            return Ok(());
        }
        let conn = lancedb::connect(&self.uri)
            .execute()
            .await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        let table = match conn.open_table(collection).execute().await {
            Ok(t) => t,
            Err(_) => return Ok(()),
        };
        // Use the first-class source_uri column for exact equality.
        let safe_uri = source_uri.replace('\'', "''");
        table
            .delete(&format!("source_uri = '{}'", safe_uri))
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

    #[instrument(skip(self), fields(store = "lancedb"), err)]
    async fn list_collections(&self) -> Result<Vec<String>> {
        let conn = lancedb::connect(&self.uri)
            .execute()
            .await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        let table_names = conn
            .table_names()
            .execute()
            .await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        let mut all: HashSet<String> = table_names.into_iter().collect();
        for name in self.load_sidecar() {
            all.insert(name);
        }
        let mut result: Vec<String> = all.into_iter().collect();
        result.sort();
        Ok(result)
    }

    #[instrument(skip(self), fields(store = "lancedb", collection = collection), err)]
    async fn create_collection(&self, collection: &str) -> Result<()> {
        let _guard = self.sidecar_lock.lock().await;
        let in_lance = self.collection_exists(collection).await?;
        let names = self.load_sidecar();
        let in_sidecar = names.contains(&collection.to_string());
        if in_lance || in_sidecar {
            return Err(ArcanumError::AlreadyExists(
                format!("collection '{}' already exists", collection)
            ));
        }
        let mut updated = names;
        updated.push(collection.to_string());
        self.save_sidecar(&updated)
    }

    #[instrument(skip(self), fields(store = "lancedb", collection = ?collection), err)]
    async fn count_documents(&self, collection: Option<&str>) -> Result<u64> {
        let conn = lancedb::connect(&self.uri)
            .execute()
            .await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        let table_names = match collection {
            Some(name) => vec![name.to_string()],
            None => conn
                .table_names()
                .execute()
                .await
                .map_err(|e| ArcanumError::Storage(e.to_string()))?,
        };
        let mut total = 0u64;
        for table_name in &table_names {
            let table = match conn.open_table(table_name).execute().await {
                Ok(t) => t,
                Err(_) => continue,
            };
            let batches: Vec<RecordBatch> = table
                .query()
                .execute()
                .await
                .map_err(|e| ArcanumError::Storage(e.to_string()))?
                .try_collect()
                .await
                .map_err(|e| ArcanumError::Storage(e.to_string()))?;
            let mut source_uris = HashSet::new();
            for batch in &batches {
                if let Some(col) = batch.column_by_name("chunk_json") {
                    if let Some(strings) = col.as_any().downcast_ref::<StringArray>() {
                        for i in 0..strings.len() {
                            if let Ok(chunk) = serde_json::from_str::<IndexedChunk>(strings.value(i)) {
                                if let Some(uri) = chunk.chunk.metadata.0.get("source_uri") {
                                    if let Some(s) = uri.as_str() {
                                        source_uris.insert(s.to_string());
                                    }
                                }
                            }
                        }
                    }
                }
            }
            total += source_uris.len() as u64;
        }
        Ok(total)
    }

    #[instrument(skip(self), fields(store = "lancedb", collection = collection), err)]
    async fn delete_collection(&self, collection: &str) -> Result<()> {
        let _guard = self.sidecar_lock.lock().await;
        let conn = lancedb::connect(&self.uri)
            .execute()
            .await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        let table_names = conn
            .table_names()
            .execute()
            .await
            .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        if table_names.contains(&collection.to_string()) {
            conn.drop_table(collection, &[])
                .await
                .map_err(|e| ArcanumError::Storage(e.to_string()))?;
        }
        let names: Vec<String> = self
            .load_sidecar()
            .into_iter()
            .filter(|n| n != collection)
            .collect();
        self.save_sidecar(&names)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    async fn make_store(dir: &TempDir) -> LanceDbStore {
        let path = dir.path().join("test.lance");
        LanceDbStore::new(path.to_str().unwrap()).await.unwrap()
    }

    #[tokio::test]
    async fn create_lists_and_deletes_empty_collection() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir).await;

        store.create_collection("col1").await.unwrap();
        let cols = store.list_collections().await.unwrap();
        assert!(
            cols.contains(&"col1".to_string()),
            "created collection should appear in list"
        );

        store.delete_collection("col1").await.unwrap();
        let cols = store.list_collections().await.unwrap();
        assert!(
            !cols.contains(&"col1".to_string()),
            "deleted collection should not appear"
        );
    }

    #[tokio::test]
    async fn create_duplicate_returns_already_exists() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir).await;

        store.create_collection("col1").await.unwrap();
        let err = store.create_collection("col1").await.unwrap_err();
        assert!(matches!(
            err,
            arcanum_core::ArcanumError::AlreadyExists(_)
        ));
    }

    #[tokio::test]
    async fn delete_nonexistent_is_ok() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir).await;
        store.delete_collection("ghost").await.unwrap();
    }

    #[tokio::test]
    async fn count_documents_empty_collection() {
        let dir = TempDir::new().unwrap();
        let store = make_store(&dir).await;
        store.create_collection("empty").await.unwrap();
        assert_eq!(
            store.count_documents(Some("empty")).await.unwrap(),
            0
        );
        assert_eq!(store.count_documents(None).await.unwrap(), 0);
    }
}
