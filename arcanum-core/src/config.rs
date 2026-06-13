use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};
use tokio::sync::RwLock;
use crate::{ArcanumError, Result};
use crate::types::PerBackendChunkConfig;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RuntimeMode {
    Development,
    Production,
    Enterprise,
}

impl Default for RuntimeMode {
    fn default() -> Self {
        Self::Development
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MetadataBackend {
    Sqlite,
    Postgres,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GlobalConfig {
    #[serde(default)]
    pub runtime_mode: RuntimeMode,
    #[serde(default)]
    pub log_level: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub metadata_backend: MetadataBackend,
    pub vector_backend: String,
    pub graph_enabled: bool,
    pub tree_enabled: bool,
}

impl Default for StorageConfig {
    fn default() -> Self {
        Self {
            metadata_backend: MetadataBackend::Sqlite,
            vector_backend: "lancedb".into(),
            graph_enabled: false,
            tree_enabled: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalConfig {
    pub top_k: usize,
    pub orchestration_mode: OrchestrationMode,
    pub fusion_strategy: FusionStrategy,
    pub query_cache_enabled: bool,
}

impl Default for RetrievalConfig {
    fn default() -> Self {
        Self {
            top_k: 5,
            orchestration_mode: OrchestrationMode::ParallelFusion,
            fusion_strategy: FusionStrategy::Rrf,
            query_cache_enabled: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum OrchestrationMode {
    Static,
    QueryClassified,
    ParallelFusion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FusionStrategy {
    Rrf,
    Weighted(Vec<(String, f32)>),
    Learned,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionConfig {
    pub worker_pool_size:    usize,
    pub queue_capacity:      usize,
    pub retry_max_attempts:  u32,
    pub retry_base_delay_ms: u64,
    #[serde(default)]
    pub chunking:            PerBackendChunkConfig,
    #[serde(default)]
    pub docling:             Option<DoclingConfig>,
}

impl Default for IngestionConfig {
    fn default() -> Self {
        Self {
            worker_pool_size:    4,
            queue_capacity:      10_000,
            retry_max_attempts:  3,
            retry_base_delay_ms: 1_000,
            chunking: PerBackendChunkConfig::default(),
            docling: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoclingConfig {
    pub enabled: bool,
    #[serde(default)]
    pub backend: DoclingBackendConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum DoclingBackendConfig {
    Http {
        base_url: String,
        #[serde(default)]
        api_key: Option<String>,
        #[serde(default = "default_timeout")]
        timeout_secs: u64,
        #[serde(default)]
        use_async: bool,
        #[serde(default = "default_poll_interval")]
        poll_interval_ms: u64,
    },
    Cli {
        command: String,
    },
}

fn default_timeout() -> u64 { 300 }
fn default_poll_interval() -> u64 { 2000 }

impl Default for DoclingBackendConfig {
    fn default() -> Self {
        Self::Http {
            base_url: "http://localhost:5001".to_string(),
            api_key: None,
            timeout_secs: 300,
            use_async: false,
            poll_interval_ms: 2000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    pub provider: String,
    pub model_id: String,
    pub dimension: usize,
    pub batch_size: usize,
    pub cache_enabled: bool,
    pub parallelism: usize,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            provider: "ollama".to_string(),
            model_id: "nomic-embed-text".to_string(),
            dimension: 0,
            batch_size: 32,
            cache_enabled: false,
            parallelism: 4,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EnrichmentConfig {
    pub context_prefix_provider: Option<String>,
    pub entity_extraction_provider: Option<String>,
    pub summarize_provider: Option<String>,
    pub caption_provider: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvalConfig {
    pub enabled: bool,
    pub schedule_cron: Option<String>,
    pub default_k: usize,
}

impl Default for EvalConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            schedule_cron: None,
            default_k: 10,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminConfig {
    pub jwt_rs256_public_key_pem: Option<String>,
    pub ip_allowlist: Vec<String>,
    pub portal_enabled: bool,
    pub audit_retention_days: u32,
    pub secret_store_reload_interval_secs: u64,
}

impl Default for AdminConfig {
    fn default() -> Self {
        Self {
            jwt_rs256_public_key_pem: None,
            ip_allowlist: vec![],
            portal_enabled: false,
            audit_retention_days: 90,
            secret_store_reload_interval_secs: 300,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Origins allowed for CORS. Empty = deny all cross-origin requests (fail-closed).
    pub cors_allowed_origins: Vec<String>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self { cors_allowed_origins: vec![] }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArcanumConfig {
    #[serde(default)]
    pub global: GlobalConfig,
    #[serde(default)]
    pub ingestion: IngestionConfig,
    #[serde(default)]
    pub embedding: EmbeddingConfig,
    #[serde(default)]
    pub enrichment: EnrichmentConfig,
    #[serde(default)]
    pub storage: StorageConfig,
    #[serde(default)]
    pub retrieval: RetrievalConfig,
    #[serde(default)]
    pub eval: EvalConfig,
    #[serde(default)]
    pub admin: AdminConfig,
    #[serde(default)]
    pub server: ServerConfig,
}

impl ArcanumConfig {
    pub fn from_env() -> Self {
        let mut cfg = Self::default();
        if let Ok(mode) = std::env::var("ARCANUM_RUNTIME_MODE") {
            cfg.global.runtime_mode = match mode.to_lowercase().as_str() {
                "production" => RuntimeMode::Production,
                "enterprise" => RuntimeMode::Enterprise,
                _ => RuntimeMode::Development,
            };
        }
        if let Ok(v) = std::env::var("ARCANUM_INGESTION_WORKER_POOL_SIZE") {
            if let Ok(n) = v.parse() {
                cfg.ingestion.worker_pool_size = n;
            }
        }
        if let Ok(v) = std::env::var("ARCANUM_INGESTION_QUEUE_CAPACITY") {
            if let Ok(n) = v.parse() {
                cfg.ingestion.queue_capacity = n;
            }
        }
        if let Ok(v) = std::env::var("ARCANUM_EMBEDDING_PROVIDER") {
            cfg.embedding.provider = v;
        }
        if let Ok(v) = std::env::var("ARCANUM_EMBEDDING_MODEL_ID") {
            cfg.embedding.model_id = v;
        }
        if let Ok(v) = std::env::var("ARCANUM_EVAL_ENABLED") {
            cfg.eval.enabled = v == "true" || v == "1";
        }
        if let Ok(v) = std::env::var("ARCANUM_CORS_ALLOWED_ORIGINS") {
            cfg.server.cors_allowed_origins = v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
        cfg
    }

    /// Load config from a TOML or YAML file. Extension determines format.
    pub fn from_file(path: &Path) -> crate::Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| crate::ArcanumError::Config(format!("cannot read config file: {}", e)))?;
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match ext {
            "toml" => toml::from_str(&content)
                .map_err(|e| crate::ArcanumError::Config(format!("TOML parse error: {}", e))),
            "yaml" | "yml" => serde_yaml::from_str(&content)
                .map_err(|e| crate::ArcanumError::Config(format!("YAML parse error: {}", e))),
            other => Err(crate::ArcanumError::Config(
                format!("unsupported config format '.{}'; use .toml or .yaml", other)
            )),
        }
    }

    /// Layer: defaults → file → env. Later layers override earlier ones.
    pub fn merged(file_path: Option<&Path>) -> crate::Result<Self> {
        let mut cfg = Self::default();
        if let Some(path) = file_path {
            cfg = Self::from_file(path)?;
        }
        let from_env = Self::from_env();
        if std::env::var("ARCANUM_RUNTIME_MODE").is_ok() {
            cfg.global.runtime_mode = from_env.global.runtime_mode;
        }
        if std::env::var("ARCANUM_INGESTION_WORKER_POOL_SIZE").is_ok() {
            cfg.ingestion.worker_pool_size = from_env.ingestion.worker_pool_size;
        }
        if std::env::var("ARCANUM_INGESTION_QUEUE_CAPACITY").is_ok() {
            cfg.ingestion.queue_capacity = from_env.ingestion.queue_capacity;
        }
        if std::env::var("ARCANUM_EMBEDDING_PROVIDER").is_ok() {
            cfg.embedding.provider = from_env.embedding.provider;
        }
        if std::env::var("ARCANUM_EMBEDDING_MODEL_ID").is_ok() {
            cfg.embedding.model_id = from_env.embedding.model_id;
        }
        if std::env::var("ARCANUM_EVAL_ENABLED").is_ok() {
            cfg.eval.enabled = from_env.eval.enabled;
        }
        if std::env::var("ARCANUM_CORS_ALLOWED_ORIGINS").is_ok() {
            cfg.server.cors_allowed_origins = from_env.server.cors_allowed_origins;
        }
        Ok(cfg)
    }

    pub fn validate(&self) -> Result<()> {
        if self.global.runtime_mode != RuntimeMode::Development
            && self.storage.metadata_backend == MetadataBackend::Sqlite
        {
            return Err(ArcanumError::Config(
                "SQLite is not allowed in production or enterprise mode. Use PostgreSQL."
                    .into(),
            ));
        }
        Ok(())
    }
}

/// Hot-reloadable config wrapper. The admin API calls `update()` to apply patches at runtime.
#[derive(Clone)]
pub struct LiveConfig(Arc<RwLock<ArcanumConfig>>);

impl LiveConfig {
    pub fn new(config: ArcanumConfig) -> Self {
        Self(Arc::new(RwLock::new(config)))
    }

    pub async fn get(&self) -> ArcanumConfig {
        self.0.read().await.clone()
    }

    pub async fn update(&self, f: impl FnOnce(&mut ArcanumConfig) + Send) {
        let mut cfg = self.0.write().await;
        f(&mut cfg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_defaults() {
        let cfg = ArcanumConfig::default();
        assert_eq!(cfg.global.runtime_mode, RuntimeMode::Development);
        assert_eq!(cfg.retrieval.top_k, 5);
    }

    #[test]
    fn test_config_from_env() {
        std::env::set_var("ARCANUM_RUNTIME_MODE", "production");
        let cfg = ArcanumConfig::from_env();
        assert_eq!(cfg.global.runtime_mode, RuntimeMode::Production);
        std::env::remove_var("ARCANUM_RUNTIME_MODE");
    }

    #[test]
    fn test_production_rejects_sqlite() {
        let mut cfg = ArcanumConfig::default();
        cfg.global.runtime_mode = RuntimeMode::Production;
        cfg.storage.metadata_backend = MetadataBackend::Sqlite;
        assert!(cfg.validate().is_err());
    }

    #[test]
    fn test_new_config_sections_have_defaults() {
        let cfg = ArcanumConfig::default();
        assert_eq!(cfg.ingestion.worker_pool_size, 4);
        assert_eq!(cfg.ingestion.queue_capacity, 10_000);
        assert_eq!(cfg.embedding.provider, "ollama");
        assert!(!cfg.eval.enabled);
        assert!(!cfg.admin.portal_enabled);
    }

    #[test]
    fn test_from_env_ingestion_queue() {
        std::env::set_var("ARCANUM_INGESTION_QUEUE_CAPACITY", "500");
        let cfg = ArcanumConfig::from_env();
        assert_eq!(cfg.ingestion.queue_capacity, 500);
        std::env::remove_var("ARCANUM_INGESTION_QUEUE_CAPACITY");
    }

    #[test]
    fn test_from_toml_file() {
        use std::io::Write;
        let mut f = tempfile::Builder::new().suffix(".toml").tempfile().unwrap();
        writeln!(f, r#"
[global]
log_level = "info"

[ingestion]
worker_pool_size = 8
queue_capacity = 5000
retry_max_attempts = 3
retry_base_delay_ms = 1000
"#).unwrap();
        let cfg = ArcanumConfig::from_file(f.path()).unwrap();
        assert_eq!(cfg.ingestion.worker_pool_size, 8);
    }

    #[test]
    fn test_from_file_unsupported_extension() {
        let result = ArcanumConfig::from_file(std::path::Path::new("config.json"));
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_live_config_hot_update() {
        let live = LiveConfig::new(ArcanumConfig::default());
        live.update(|cfg| cfg.ingestion.worker_pool_size = 99).await;
        assert_eq!(live.get().await.ingestion.worker_pool_size, 99);
    }

    #[test]
    fn test_server_config_cors_defaults_empty() {
        let cfg = ArcanumConfig::default();
        assert!(cfg.server.cors_allowed_origins.is_empty());
    }

    #[test]
    fn test_server_config_cors_from_env() {
        std::env::set_var("ARCANUM_CORS_ALLOWED_ORIGINS", "https://a.com, https://b.com");
        let cfg = ArcanumConfig::from_env();
        assert_eq!(cfg.server.cors_allowed_origins,
            vec!["https://a.com".to_string(), "https://b.com".to_string()]);
        std::env::remove_var("ARCANUM_CORS_ALLOWED_ORIGINS");
    }

    #[test]
    fn ingestion_default_chunking_matches_per_backend_default() {
        let ic  = IngestionConfig::default();
        let pbc = PerBackendChunkConfig::default();
        // PerBackendChunkConfig doesn't derive PartialEq, so compare via JSON serialization.
        assert_eq!(
            serde_json::to_value(&ic.chunking).unwrap(),
            serde_json::to_value(&pbc).unwrap(),
            "IngestionConfig::default().chunking must equal PerBackendChunkConfig::default()"
        );
    }

    #[test]
    fn test_docling_config_http_deserializes() {
        let toml = r#"
[ingestion]
worker_pool_size = 4
queue_capacity = 10000
retry_max_attempts = 3
retry_base_delay_ms = 1000

[ingestion.docling]
enabled = true

[ingestion.docling.backend]
type = "http"
base_url = "http://localhost:5001"
timeout_secs = 300
"#;
        let cfg: ArcanumConfig = toml::from_str(toml).unwrap();
        let docling = cfg.ingestion.docling.unwrap();
        assert!(docling.enabled);
        assert!(matches!(
            docling.backend,
            DoclingBackendConfig::Http { ref base_url, .. } if base_url == "http://localhost:5001"
        ));
    }

    #[test]
    fn test_docling_config_cli_deserializes() {
        let toml = r#"
[ingestion]
worker_pool_size = 4
queue_capacity = 10000
retry_max_attempts = 3
retry_base_delay_ms = 1000

[ingestion.docling]
enabled = true

[ingestion.docling.backend]
type = "cli"
command = "docling"
"#;
        let cfg: ArcanumConfig = toml::from_str(toml).unwrap();
        let docling = cfg.ingestion.docling.unwrap();
        assert!(matches!(
            docling.backend,
            DoclingBackendConfig::Cli { ref command } if command == "docling"
        ));
    }

    #[test]
    fn test_docling_config_http_async_deserializes() {
        let toml = r#"
[ingestion]
worker_pool_size = 4
queue_capacity = 10000
retry_max_attempts = 3
retry_base_delay_ms = 1000

[ingestion.docling]
enabled = true

[ingestion.docling.backend]
type = "http"
base_url = "http://localhost:5001"
use_async = true
poll_interval_ms = 3000
"#;
        let cfg: ArcanumConfig = toml::from_str(toml).unwrap();
        let dc = cfg.ingestion.docling.unwrap();
        assert!(matches!(
            dc.backend,
            DoclingBackendConfig::Http { use_async: true, poll_interval_ms: 3000, .. }
        ));
    }

    #[test]
    fn test_docling_config_absent_leaves_default_none() {
        let toml = r#"
[ingestion]
worker_pool_size = 4
queue_capacity = 10000
retry_max_attempts = 3
retry_base_delay_ms = 1000
"#;
        let cfg: ArcanumConfig = toml::from_str(toml).unwrap();
        assert!(cfg.ingestion.docling.is_none());
    }
}
