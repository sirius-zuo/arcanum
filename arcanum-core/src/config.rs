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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ArcanumConfig {
    pub global: GlobalConfig,
    pub storage: StorageConfig,
    pub retrieval: RetrievalConfig,
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
}
