use serde::{Deserialize, Serialize};
use crate::{ArcanumError, Result};

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
    pub runtime_mode: RuntimeMode,
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
    pub worker_pool_size: usize,
    pub queue_capacity: usize,
    pub retry_max_attempts: u32,
    pub retry_base_delay_ms: u64,
}

impl Default for IngestionConfig {
    fn default() -> Self {
        Self {
            worker_pool_size: 4,
            queue_capacity: 10_000,
            retry_max_attempts: 3,
            retry_base_delay_ms: 1_000,
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
}

impl Default for AdminConfig {
    fn default() -> Self {
        Self {
            jwt_rs256_public_key_pem: None,
            ip_allowlist: vec![],
            portal_enabled: false,
            audit_retention_days: 90,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArcanumConfig {
    pub global: GlobalConfig,
    pub ingestion: IngestionConfig,
    pub embedding: EmbeddingConfig,
    pub enrichment: EnrichmentConfig,
    pub storage: StorageConfig,
    pub retrieval: RetrievalConfig,
    pub eval: EvalConfig,
    pub admin: AdminConfig,
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
        cfg
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
}
