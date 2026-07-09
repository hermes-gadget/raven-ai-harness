//! Provider factory — builds Provider instances from ProviderConfig.
//!
//! Resolves API keys from direct values or environment variables.

use std::collections::HashMap;
use std::sync::Arc;

use odin_core::config::ProviderConfig;
use odin_core::error::{OdinError, OdinResult};
use odin_core::traits::Provider;

use crate::anthropic::AnthropicProvider;
use crate::fallback::FallbackProvider;
use crate::local::LocalProvider;
use crate::openai_compat::OpenAiCompatProvider;

/// Build a single Provider from ProviderConfig, resolving API keys from env vars.
pub fn create_provider(cfg: &ProviderConfig) -> OdinResult<Arc<dyn Provider>> {
    match cfg.provider_type.as_str() {
        "openai_compat" => {
            let api_key = resolve_api_key(&cfg.api_key, &cfg.api_key_env)?;
            let base_url = cfg
                .base_url
                .clone()
                .unwrap_or_else(|| "https://api.openai.com/v1".into());
            Ok(Arc::new(OpenAiCompatProvider::new_with_timeout(
                "openai",
                base_url,
                api_key,
                Some(cfg.timeout_secs),
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

/// Build a Provider from ProviderConfig, optionally wrapping it in a FallbackProvider
/// when `fallback_chain` is configured.
///
/// The `all_configs` map must contain all providers referenced in the fallback chain.
pub fn create_provider_chain(
    cfg: &ProviderConfig,
    all_configs: &HashMap<String, ProviderConfig>,
) -> OdinResult<Arc<dyn Provider>> {
    // If no fallback chain is configured, just create a single provider
    let fallback_names = match &cfg.fallback_chain {
        Some(names) if !names.is_empty() => names.clone(),
        _ => return create_provider(cfg),
    };

    // Build the primary provider
    let primary = create_provider(cfg)?;
    let primary_name = cfg
        .default_model
        .clone()
        .unwrap_or_else(|| "primary".to_string());

    // Build all fallback providers
    let mut fallbacks: Vec<(String, Arc<dyn Provider>)> = Vec::new();
    for name in &fallback_names {
        let fallback_cfg = all_configs.get(name).ok_or_else(|| {
            OdinError::Config(format!(
                "Fallback provider '{}' not found in config providers map",
                name
            ))
        })?;

        let provider = create_provider(fallback_cfg)?;
        fallbacks.push((name.clone(), provider));
    }

    let health_interval = cfg.health_check_interval_secs;
    let circuit_threshold = cfg.circuit_breaker_threshold;

    Ok(Arc::new(FallbackProvider::new(
        format!(
            "{}_chain",
            fallback_names.first().unwrap_or(&"fallback".into())
        ),
        primary_name,
        primary,
        fallbacks,
        circuit_threshold,
        health_interval,
    )))
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
    if let Some(key) = direct
        && !key.is_empty()
    {
        return Ok(Some(key.clone()));
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
            fallback_chain: None,
            health_check_interval_secs: 0,
            circuit_breaker_threshold: 0,
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
            fallback_chain: None,
            health_check_interval_secs: 0,
            circuit_breaker_threshold: 0,
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
            fallback_chain: None,
            health_check_interval_secs: 0,
            circuit_breaker_threshold: 0,
        };
        let provider = create_provider(&cfg).unwrap();
        assert_eq!(provider.name(), "openai");
    }
}
