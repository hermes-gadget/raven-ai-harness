//! Secure credential and secret management for the Odin harness.
//!
//! The [`SecretManager`] provides encrypted storage for API keys, tokens,
//! and other sensitive credentials used by providers and tools.

use odin_core::error::OdinResult;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// A stored secret with metadata.
#[derive(Debug, Clone)]
pub struct Secret {
    /// Name/key identifying this secret.
    pub name: String,
    /// The secret value.
    pub value: String,
    /// Optional description of what this secret is used for.
    pub description: Option<String>,
    /// Whether this secret came from an environment variable.
    pub from_env: bool,
    /// The environment variable name (if from_env is true).
    pub env_var: Option<String>,
}

/// Manages credentials and secrets used across the Odin harness.
///
/// Secrets can be:
/// - Loaded from environment variables at startup
/// - Set programmatically
/// - Referenced by name from providers and tools
///
/// **Note:** This is a basic in-memory implementation. For production use,
/// secrets should be stored encrypted-at-rest (e.g., via OS keychain or
/// a dedicated secrets store).
pub struct SecretManager {
    /// Stored secrets, keyed by name.
    secrets: Arc<RwLock<HashMap<String, Secret>>>,
    /// Whether to mask secrets in logs.
    #[allow(dead_code)]
    mask_in_logs: bool,
}

impl SecretManager {
    /// Create a new secret manager.
    pub fn new(mask_in_logs: bool) -> Self {
        Self {
            secrets: Arc::new(RwLock::new(HashMap::new())),
            mask_in_logs,
        }
    }

    /// Create a new secret manager with default settings.
    pub fn default() -> Self {
        Self::new(true)
    }

    /// Store a secret value.
    pub async fn set_secret(
        &self,
        name: &str,
        value: &str,
        description: Option<String>,
    ) -> OdinResult<()> {
        let secret = Secret {
            name: name.to_string(),
            value: value.to_string(),
            description,
            from_env: false,
            env_var: None,
        };

        self.secrets.write().await.insert(name.to_string(), secret);
        debug!(
            secret_name = %name,
            "Secret stored"
        );
        Ok(())
    }

    /// Store a secret from an environment variable.
    ///
    /// Reads the value from `env_var` at call time. Returns an error if the
    /// environment variable is not set.
    pub async fn set_secret_from_env(
        &self,
        name: &str,
        env_var: &str,
        description: Option<String>,
    ) -> OdinResult<()> {
        let value = std::env::var(env_var).map_err(|_| {
            odin_core::error::OdinError::Config(format!(
                "Environment variable '{}' is not set (required for secret '{}')",
                env_var, name
            ))
        })?;

        let secret = Secret {
            name: name.to_string(),
            value,
            description,
            from_env: true,
            env_var: Some(env_var.to_string()),
        };

        self.secrets.write().await.insert(name.to_string(), secret);
        info!(
            secret_name = %name,
            env_var = %env_var,
            "Secret loaded from environment variable"
        );
        Ok(())
    }

    /// Retrieve a secret by name.
    pub async fn get_secret(&self, name: &str) -> OdinResult<Option<String>> {
        let secrets = self.secrets.read().await;
        Ok(secrets.get(name).map(|s| s.value.clone()))
    }

    /// Get metadata about a secret (without revealing the value).
    pub async fn get_secret_info(&self, name: &str) -> Option<Secret> {
        let secrets = self.secrets.read().await;
        secrets.get(name).cloned()
    }

    /// Remove a secret.
    pub async fn remove_secret(&self, name: &str) -> OdinResult<bool> {
        let existed = self.secrets.write().await.remove(name).is_some();
        if existed {
            info!(secret_name = %name, "Secret removed");
        } else {
            warn!(secret_name = %name, "Secret not found for removal");
        }
        Ok(existed)
    }

    /// List all stored secret names (without values).
    pub async fn list_secrets(&self) -> Vec<String> {
        self.secrets.read().await.keys().cloned().collect()
    }

    /// Check if a secret exists.
    pub async fn has_secret(&self, name: &str) -> bool {
        self.secrets.read().await.contains_key(name)
    }

    /// Get the number of stored secrets.
    pub async fn secret_count(&self) -> usize {
        self.secrets.read().await.len()
    }

    /// Mask a string, hiding any secret values found within it.
    ///
    /// Useful for sanitizing log output that might contain secret data.
    pub async fn mask_string(&self, input: &str) -> String {
        let secrets = self.secrets.read().await;
        let mut result = input.to_string();
        for secret in secrets.values() {
            if secret.value.len() >= 4 {
                result = result.replace(&secret.value, "****");
            }
        }
        result
    }

    /// Resolve a value that might be a reference to a secret.
    ///
    /// Supports:
    /// - `env:VAR_NAME` — read from environment variable
    /// - `secret:SECRET_NAME` — read from stored secret
    /// - Any other value is returned as-is.
    pub async fn resolve(&self, value: &str) -> OdinResult<String> {
        if let Some(env_var) = value.strip_prefix("env:") {
            std::env::var(env_var).map_err(|_| {
                odin_core::error::OdinError::Config(format!(
                    "Environment variable '{}' referenced but not set",
                    env_var
                ))
            })
        } else if let Some(secret_name) = value.strip_prefix("secret:") {
            self.get_secret(secret_name)
                .await?
                .ok_or_else(|| {
                    odin_core::error::OdinError::Config(format!(
                        "Secret '{}' referenced but not found",
                        secret_name
                    ))
                })
        } else {
            Ok(value.to_string())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_set_and_get_secret() {
        let mgr = SecretManager::default();
        mgr.set_secret("api_key", "sk-123456", Some("API Key".into()))
            .await
            .unwrap();

        let value = mgr.get_secret("api_key").await.unwrap();
        assert_eq!(value, Some("sk-123456".into()));
    }

    #[tokio::test]
    async fn test_remove_secret() {
        let mgr = SecretManager::default();
        mgr.set_secret("test", "value", None).await.unwrap();

        assert!(mgr.remove_secret("test").await.unwrap());
        assert!(!mgr.remove_secret("test").await.unwrap());
        assert_eq!(mgr.secret_count().await, 0);
    }

    #[tokio::test]
    async fn test_list_secrets() {
        let mgr = SecretManager::default();
        mgr.set_secret("key1", "val1", None).await.unwrap();
        mgr.set_secret("key2", "val2", None).await.unwrap();

        let mut names = mgr.list_secrets().await;
        names.sort();
        assert_eq!(names, vec!["key1", "key2"]);
    }

    #[tokio::test]
    async fn test_has_secret() {
        let mgr = SecretManager::default();
        mgr.set_secret("exists", "yes", None).await.unwrap();

        assert!(mgr.has_secret("exists").await);
        assert!(!mgr.has_secret("missing").await);
    }

    #[tokio::test]
    async fn test_resolve_raw_value() {
        let mgr = SecretManager::default();
        let result = mgr.resolve("plain-value").await.unwrap();
        assert_eq!(result, "plain-value");
    }

    #[tokio::test]
    async fn test_resolve_secret_ref() {
        let mgr = SecretManager::default();
        mgr.set_secret("my_key", "my_secret_value", None)
            .await
            .unwrap();

        let result = mgr.resolve("secret:my_key").await.unwrap();
        assert_eq!(result, "my_secret_value");
    }

    #[tokio::test]
    async fn test_resolve_missing_secret() {
        let mgr = SecretManager::default();
        let result = mgr.resolve("secret:missing").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_secret_info() {
        let mgr = SecretManager::default();
        mgr.set_secret("key", "value", Some("desc".into()))
            .await
            .unwrap();

        let info = mgr.get_secret_info("key").await.unwrap();
        assert_eq!(info.name, "key");
        assert_eq!(info.description, Some("desc".into()));
        assert!(!info.from_env);
    }

    #[tokio::test]
    async fn test_mask_string() {
        let mgr = SecretManager::default();
        mgr.set_secret("token", "sk-secret-123", None)
            .await
            .unwrap();

        let masked = mgr
            .mask_string("Bearer sk-secret-123 in request")
            .await;
        assert_eq!(masked, "Bearer **** in request");
    }

    #[tokio::test]
    async fn test_set_secret_from_env() {
        // Set a test env var
        unsafe { std::env::set_var("TEST_SECRET_ENV", "env-value-456"); }
        let mgr = SecretManager::default();

        mgr.set_secret_from_env("test_secret", "TEST_SECRET_ENV", None)
            .await
            .unwrap();

        let value = mgr.get_secret("test_secret").await.unwrap();
        assert_eq!(value, Some("env-value-456".into()));

        let info = mgr.get_secret_info("test_secret").await.unwrap();
        assert!(info.from_env);
        assert_eq!(info.env_var, Some("TEST_SECRET_ENV".into()));

        unsafe { std::env::remove_var("TEST_SECRET_ENV"); }
    }

    #[tokio::test]
    async fn test_set_secret_from_missing_env() {
        let mgr = SecretManager::default();
        let result = mgr
            .set_secret_from_env("missing", "DOES_NOT_EXIST_XYZ", None)
            .await;
        assert!(result.is_err());
    }
}
