use arcanum_core::{traits::SecretStore, Result, ArcanumError};
use async_trait::async_trait;

pub struct EnvSecretStore;

#[async_trait]
impl SecretStore for EnvSecretStore {
    async fn get(&self, key_path: &str) -> Result<String> {
        std::env::var(key_path).map_err(|_| ArcanumError::Config(
            format!("secret not found in environment: {}", key_path)
        ))
    }

    async fn reload(&self) -> Result<()> {
        // Environment variables are read live; no reload needed.
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_env_secret_store_reads_env_var() {
        std::env::set_var("ARCANUM_SECRET_TEST_XYZ", "my-secret");
        let val = EnvSecretStore.get("ARCANUM_SECRET_TEST_XYZ").await.unwrap();
        assert_eq!(val, "my-secret");
        std::env::remove_var("ARCANUM_SECRET_TEST_XYZ");
    }

    #[tokio::test]
    async fn test_env_secret_store_errors_on_missing() {
        let result = EnvSecretStore.get("ARCANUM_DEFINITELY_NOT_SET_9999").await;
        assert!(result.is_err());
    }
}
