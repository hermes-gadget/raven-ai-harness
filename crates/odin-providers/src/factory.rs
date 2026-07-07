//! Provider factory — builds Provider instances from ProviderConfig.
//!
//! Resolves API keys from direct values or environment variables.

use std::sync::Arc;

use odin_core::config::ProviderConfig;
use odin_core::error::{OdinError, OdinResult};
use odin_core::traits::Provider;

use crate::anthropic::AnthropicProvider;
use crate::local::LocalProvider;
use crate::openai_compat::OpenAiCompatProvider;

/// Build a Provider from ProviderConfig, resolving API keys from env vars.
pub fn create_provider(cfg: &ProviderConfig) -> OdinResult<Arc<dyn Provider>> {
    match cfg.provider_type.as_str() {
        "openai_compat" => {
            let api_key = resolve_api_key(&cfg.api_key, &cfg.api_key_env)?;
            let base_url = cfg
                .base_url
                .clone()
                .unwrap_or_else(|| "https://api.openai.com/v1".into());
            Ok(Arc::new(OpenAiCompatProvider::new(
                "openai", base_url, api_key,
            )))
        }
        "anthropic" => {
            let api_key = resolve_api_key(&cfg.api_key, &cfg.api_key_env)?
                .ok_or_else(|| OdinError::Config("Anthropic API key required".into()))?;
            Ok(Arc::new(AnthropicProvider::new(api_key)))
        }
        "local" | "ollama" => {
            let base_url = cfg
                .base_url
                .clone()
                .unwrap_or_else(|| "http://localhost:11434/v1".into());
            Ok(Arc::new(LocalProvider::new(base_url)))
        }
        _ => Err(OdinError::Config(format!(
            "Unknown provider type: {}",
            cfg.provider_type
        ))),
    }
}

/// Resolve an API key from a direct value or an environment variable name.
///
/// Returns `Ok(None)` when neither source is provided (acceptable for
/// providers that work without an API key, like local/ollama).
fn resolve_api_key(
    direct: &Option<String>,
    env_var: &Option<String>,
) -> OdinResult<Option<String>> {
    // Prefer direct key
    if let Some(key) = direct {
        if !key.is_empty() {
            return Ok(Some(key.clone()));
        }
    }

    // Otherwise try env var
    if let Some(var_name) = env_var {
        let var_name = var_name.trim();
        if !var_name.is_empty() {
            match std::env::var(var_name) {
                Ok(val) if !val.is_empty() => return Ok(Some(val)),
                Ok(_) => {
                    tracing::warn!("[FACTORY] Env var '{}' is set but empty", var_name);
                }
                Err(std::env::VarError::NotPresent) => {
                    tracing::debug!(
                        "[FACTORY] Env var '{}' not set, proceeding without key",
                        var_name
                    );
                }
                Err(std::env::VarError::NotUnicode(_)) => {
                    tracing::warn!("[FACTORY] Env var '{}' contains non-unicode data", var_name);
                }
            }
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use odin_core::config::ProviderConfig;

    #[test]
    fn test_resolve_api_key_direct() {
        let result = resolve_api_key(&Some("sk-direct".into()), &Some("API_KEY".into())).unwrap();
        assert_eq!(result, Some("sk-direct".into()));
    }

    #[test]
    fn test_resolve_api_key_env() {
        unsafe { std::env::set_var("TEST_ODIN_KEY", "sk-from-env") };
        let result = resolve_api_key(&None, &Some("TEST_ODIN_KEY".into())).unwrap();
        assert_eq!(result, Some("sk-from-env".into()));
        unsafe { std::env::remove_var("TEST_ODIN_KEY") };
    }

    #[test]
    fn test_resolve_api_key_none() {
        let result = resolve_api_key(&None, &None).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_create_provider_unknown_type() {
        let cfg = ProviderConfig {
            provider_type: "nonexistent".into(),
            base_url: None,
            api_key: None,
            api_key_env: None,
            default_model: None,
            headers: Default::default(),
            timeout_secs: 60,
            max_retries: 3,
        };
        let result = create_provider(&cfg);
        assert!(result.is_err());
        let err = match result {
            Err(e) => e,
            Ok(_) => panic!("expected error"),
        };
        assert!(err.to_string().contains("Unknown provider type"));
    }

    #[test]
    fn test_create_provider_local() {
        let cfg = ProviderConfig {
            provider_type: "local".into(),
            base_url: Some("http://localhost:11434/v1".into()),
            api_key: None,
            api_key_env: None,
            default_model: None,
            headers: Default::default(),
            timeout_secs: 60,
            max_retries: 3,
        };
        let provider = create_provider(&cfg).unwrap();
        assert_eq!(provider.name(), "local");
    }

    #[test]
    fn test_create_provider_openai_compat() {
        let cfg = ProviderConfig {
            provider_type: "openai_compat".into(),
            base_url: Some("https://api.openai.com/v1".into()),
            api_key: None,
            api_key_env: None,
            default_model: None,
            headers: Default::default(),
            timeout_secs: 60,
            max_retries: 3,
        };
        let provider = create_provider(&cfg).unwrap();
        assert_eq!(provider.name(), "openai");
    }
}
