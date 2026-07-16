use arcanum_core::{traits::{CacheInvalidator, Embedder}, types::*, Result, ArcanumError};
use async_trait::async_trait;
use redis::AsyncCommands;
use sha2::{Sha256, Digest};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::instrument;
use metrics;

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

    #[instrument(skip(self), fields(cache_hit), err)]
    pub async fn get(&self, text: &str) -> Result<Option<Vector>> {
        let start = std::time::Instant::now();
        let key = Self::cache_key(text, &self.model_id, self.dimension);
        let mut conn = self.client.lock().await;
        let val: Option<String> = conn.get(&key).await
            .map_err(|e| ArcanumError::Config(format!("Redis get error: {}", e)))?;
        let result = match val {
            None => Ok(None),
            Some(s) => {
                let floats: Vec<f32> = serde_json::from_str(&s)
                    .map_err(|e| ArcanumError::Config(format!("cache deserialize error: {}", e)))?;
                Ok(Some(Vector(floats)))
            }
        };
        let hit = result.as_ref().map_or(false, |v| v.is_some());
        metrics::counter!("arcanum_cache_ops_total", "op" => "embed_get", "result" => if hit { "hit" } else { "miss" }).increment(1);
        metrics::histogram!("arcanum_model_call_duration_seconds", "provider" => "redis", "operation" => "embed_cache_get").record(start.elapsed().as_secs_f64());
        tracing::Span::current().record("cache_hit", hit);
        result
    }

    #[instrument(skip(self, vector), err)]
    pub async fn set(&self, text: &str, vector: Vector) -> Result<()> {
        let start = std::time::Instant::now();
        let key = Self::cache_key(text, &self.model_id, self.dimension);
        let serialized = serde_json::to_string(&vector.0)
            .map_err(|e| ArcanumError::Config(format!("cache serialize error: {}", e)))?;
        let mut conn = self.client.lock().await;
        let result: std::result::Result<(), _> = conn.set_ex(&key, serialized, 3600).await
            .map_err(|e| ArcanumError::Config(format!("Redis set error: {}", e)));
        let status = if result.is_ok() { "ok" } else { "error" };
        metrics::counter!("arcanum_cache_ops_total", "op" => "embed_set", "result" => status).increment(1);
        metrics::histogram!("arcanum_model_call_duration_seconds", "provider" => "redis", "operation" => "embed_cache_set").record(start.elapsed().as_secs_f64());
        result.map(|_| ())
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

/// Decorator: per-text Redis cache in front of any Embedder.
/// Misses are embedded in one inner batch, preserving input order in the output.
/// Consistency is TTL-based (EmbeddingCache::set uses a 1h expiry); per-source
/// invalidation via record_source_association is a follow-up — see the
/// invalidate_document impl on EmbeddingCache.
pub struct CachingEmbedder {
    inner: Arc<dyn Embedder>,
    cache: Arc<EmbeddingCache>,
}

impl CachingEmbedder {
    pub fn new(inner: Arc<dyn Embedder>, cache: Arc<EmbeddingCache>) -> Self {
        Self { inner, cache }
    }
}

#[async_trait]
impl Embedder for CachingEmbedder {
    #[instrument(skip(self, texts), fields(text_count = texts.len()), err)]
    async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vector>> {
        let mut out: Vec<Option<Vector>> = Vec::with_capacity(texts.len());
        let mut miss_idx: Vec<usize> = vec![];
        for (i, text) in texts.iter().enumerate() {
            match self.cache.get(text).await {
                Ok(Some(v)) => out.push(Some(v)),
                Ok(None) => { out.push(None); miss_idx.push(i); }
                Err(e) => {
                    tracing::warn!(err = %e, "embedding cache get failed; treating as miss");
                    out.push(None);
                    miss_idx.push(i);
                }
            }
        }
        if !miss_idx.is_empty() {
            let miss_texts: Vec<String> = miss_idx.iter().map(|&i| texts[i].clone()).collect();
            let vectors = self.inner.embed(miss_texts).await?;
            if vectors.len() != miss_idx.len() {
                return Err(ArcanumError::Embedding(format!(
                    "inner embedder returned {} vectors for {} texts",
                    vectors.len(), miss_idx.len()
                )));
            }
            for (&i, v) in miss_idx.iter().zip(vectors) {
                // Best-effort set: a cache write failure must not fail the embed.
                if let Err(e) = self.cache.set(&texts[i], v.clone()).await {
                    tracing::warn!(err = %e, "embedding cache set failed");
                }
                out[i] = Some(v);
            }
        }
        out.into_iter()
            .map(|v| v.ok_or_else(|| ArcanumError::Embedding("embedding output slot unfilled".into())))
            .collect()
    }

    fn dimension(&self) -> usize { self.inner.dimension() }
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

    #[tokio::test]
    #[ignore]
    async fn caching_embedder_skips_inner_on_hit() {
        use arcanum_core::traits::Embedder;

        struct CountingEmbedder(std::sync::atomic::AtomicUsize);
        #[async_trait]
        impl Embedder for CountingEmbedder {
            async fn embed(&self, texts: Vec<String>) -> Result<Vec<Vector>> {
                self.0.fetch_add(texts.len(), std::sync::atomic::Ordering::SeqCst);
                // Per-text distinguishable vector so misassigned output slots fail the test.
                Ok(texts.iter().map(|t| Vector(vec![t.as_bytes()[0] as f32, 0.0, 0.0])).collect())
            }
            fn dimension(&self) -> usize { 3 }
        }
        let url = std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://localhost:6379".to_string());
        // Use unique model_id per test run to ensure cache isolation (avoid cross-run pollution)
        let model_id = format!("caching-embedder-test-{}", std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos());
        let cache = Arc::new(EmbeddingCache::new(&url, &model_id, 3).await.unwrap());
        let inner = Arc::new(CountingEmbedder(std::sync::atomic::AtomicUsize::new(0)));
        let counter = inner.clone();
        let embedder = CachingEmbedder::new(inner, cache);
        embedder.embed(vec!["alpha".into(), "beta".into()]).await.unwrap();
        // Second call: alpha+beta cached, only gamma reaches the inner embedder.
        let texts = vec!["alpha".to_string(), "gamma".to_string(), "beta".to_string()];
        let out = embedder.embed(texts.clone()).await.unwrap();
        assert_eq!(out.len(), 3);
        // Output order must match input order, mixing cache hits (alpha, beta) and a miss (gamma).
        for (i, text) in texts.iter().enumerate() {
            assert_eq!(out[i].0[0], text.as_bytes()[0] as f32,
                "out[{}] should be the vector for {:?}", i, text);
        }
        assert_eq!(counter.0.load(std::sync::atomic::Ordering::SeqCst), 3,
            "2 misses on first call + 1 miss on second");
    }
}
