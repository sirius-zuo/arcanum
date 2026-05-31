use arcanum_core::{traits::CacheInvalidator, types::*, Result, ArcanumError};
use async_trait::async_trait;
use redis::AsyncCommands;
use sha2::{Sha256, Digest};
use std::sync::Arc;
use tokio::sync::Mutex;

pub struct EmbeddingCache {
    client: Arc<Mutex<redis::aio::MultiplexedConnection>>,
    model_id: String,
    dimension: usize,
}

impl EmbeddingCache {
    pub async fn new(redis_url: &str, model_id: &str, dimension: usize) -> Result<Self> {
        let client = redis::Client::open(redis_url)
            .map_err(|e| ArcanumError::Config(format!("Redis connect error: {}", e)))?;
        let conn = client.get_multiplexed_async_connection().await
            .map_err(|e| ArcanumError::Config(format!("Redis connection error: {}", e)))?;
        Ok(Self {
            client: Arc::new(Mutex::new(conn)),
            model_id: model_id.to_string(),
            dimension,
        })
    }

    pub fn text_hash(text: &str) -> String {
        let mut h = Sha256::new();
        h.update(text.as_bytes());
        hex::encode(h.finalize())
    }

    pub fn cache_key(text: &str, model_id: &str, dimension: usize) -> String {
        format!("embed:{}:{}:{}", Self::text_hash(text), model_id, dimension)
    }

    pub async fn get(&self, text: &str) -> Result<Option<Vector>> {
        let key = Self::cache_key(text, &self.model_id, self.dimension);
        let mut conn = self.client.lock().await;
        let val: Option<String> = conn.get(&key).await
            .map_err(|e| ArcanumError::Config(format!("Redis get error: {}", e)))?;
        match val {
            None => Ok(None),
            Some(s) => {
                let floats: Vec<f32> = serde_json::from_str(&s)
                    .map_err(|e| ArcanumError::Config(format!("cache deserialize error: {}", e)))?;
                Ok(Some(Vector(floats)))
            }
        }
    }

    pub async fn set(&self, text: &str, vector: Vector) -> Result<()> {
        let key = Self::cache_key(text, &self.model_id, self.dimension);
        let serialized = serde_json::to_string(&vector.0)
            .map_err(|e| ArcanumError::Config(format!("cache serialize error: {}", e)))?;
        let mut conn = self.client.lock().await;
        let _: () = conn.set_ex(&key, serialized, 3600).await
            .map_err(|e| ArcanumError::Config(format!("Redis set error: {}", e)))?;
        Ok(())
    }

    pub async fn record_source_association(&self, source_uri: &str, text: &str) -> Result<()> {
        let text_hash = Self::text_hash(text);
        let src_key = format!("embed_src:{}", source_uri);
        let mut conn = self.client.lock().await;
        let _: () = conn.sadd(&src_key, &text_hash).await
            .map_err(|e| ArcanumError::Config(format!("Redis sadd error: {}", e)))?;
        Ok(())
    }
}

#[async_trait]
impl CacheInvalidator for EmbeddingCache {
    async fn invalidate_document(&self, source_uri: &str, _collection_id: &CollectionId) {
        let src_key = format!("embed_src:{}", source_uri);
        let mut conn = self.client.lock().await;
        let hashes: Vec<String> = conn.smembers(&src_key).await.unwrap_or_default();
        for hash in &hashes {
            let embed_key = format!("embed:{}:{}:{}", hash, self.model_id, self.dimension);
            let _: std::result::Result<(), _> = conn.del(&embed_key).await;
        }
        let _: std::result::Result<(), _> = conn.del(&src_key).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_format() {
        let key = EmbeddingCache::cache_key("hello", "my-model", 768);
        assert!(key.starts_with("embed:"), "key should start with embed:");
        assert!(key.contains("my-model"), "key should contain model_id");
        assert!(key.contains("768"), "key should contain dimension");
    }

    #[test]
    fn test_text_hash_is_deterministic() {
        let h1 = EmbeddingCache::text_hash("hello world");
        let h2 = EmbeddingCache::text_hash("hello world");
        assert_eq!(h1, h2);
        assert_ne!(h1, EmbeddingCache::text_hash("hello WORLD"));
    }

    #[tokio::test]
    #[ignore]
    async fn test_embedding_cache_round_trip() {
        let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
        let cache = EmbeddingCache::new(&url, "test-model", 3).await.unwrap();
        let v = Vector(vec![0.1, 0.2, 0.3]);
        cache.set("hello world", v).await.unwrap();
        let got = cache.get("hello world").await.unwrap();
        assert!(got.is_some());
        let floats = got.unwrap().0;
        assert!((floats[0] - 0.1f32).abs() < 1e-5);
    }
}
