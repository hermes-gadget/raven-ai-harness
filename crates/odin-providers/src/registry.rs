//! Provider registry — manages available model providers.

use async_trait::async_trait;
use dashmap::DashMap;
use odin_core::error::{OdinError, OdinResult};
use odin_core::traits::Provider;
use odin_core::types::{ChatResponse, CompletionOptions, Message, ModelInfo, ToolSchema};
use std::sync::Arc;

use crate::traits::ProviderExt;

/// Registry of all available model providers.
///
/// Thread-safe, allows dynamic addition/removal of providers at runtime.
pub struct ProviderRegistry {
    providers: DashMap<String, Arc<dyn ProviderExt>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: DashMap::new(),
        }
    }

    /// Register a provider.
    pub fn register<P: ProviderExt + 'static>(&self, provider: P) {
        let name = provider.name().to_string();
        self.providers.insert(name, Arc::new(provider));
    }

    /// Get a provider by name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn ProviderExt>> {
        self.providers.get(name).map(|p| p.clone())
    }

    /// Remove a provider.
    pub fn remove(&self, name: &str) -> Option<Arc<dyn ProviderExt>> {
        self.providers.remove(name).map(|(_, p)| p)
    }

    /// List all registered provider names.
    pub fn list_names(&self) -> Vec<String> {
        self.providers.iter().map(|p| p.name().to_string()).collect()
    }

    /// Get the default provider (first registered, or one named "default").
    pub fn default(&self) -> OdinResult<Arc<dyn ProviderExt>> {
        // Check for explicit "default" key
        if let Some(p) = self.providers.get("default") {
            return Ok(p.clone());
        }
        // Fall back to first registered
        self.providers
            .iter()
            .next()
            .map(|p| p.clone())
            .ok_or_else(|| OdinError::Config("No providers registered".into()))
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// A Provider that delegates to a registry — picks provider based on model name.
pub struct DelegatingProvider {
    registry: Arc<ProviderRegistry>,
    default_provider: String,
}

impl DelegatingProvider {
    pub fn new(registry: Arc<ProviderRegistry>, default_provider: impl Into<String>) -> Self {
        Self {
            registry,
            default_provider: default_provider.into(),
        }
    }

    fn resolve(&self, _model: &str) -> OdinResult<Arc<dyn ProviderExt>> {
        // Simple implementation: use default provider
        self.registry
            .get(&self.default_provider)
            .or_else(|| self.registry.default().ok())
            .ok_or_else(|| OdinError::Config("No provider available".into()))
    }
}

#[async_trait]
impl Provider for DelegatingProvider {
    fn name(&self) -> &str {
        "delegating"
    }

    async fn list_models(&self) -> OdinResult<Vec<ModelInfo>> {
        let provider = self.resolve("")?;
        provider.list_models().await
    }

    async fn chat(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[ToolSchema],
        options: &CompletionOptions,
    ) -> OdinResult<ChatResponse> {
        let provider = self.resolve(model)?;
        provider.chat(model, messages, tools, options).await
    }

    async fn chat_stream(
        &self,
        model: &str,
        messages: &[Message],
        tools: &[ToolSchema],
        options: &CompletionOptions,
    ) -> OdinResult<Box<dyn odin_core::traits::ChatStream>> {
        let provider = self.resolve(model)?;
        provider.chat_stream(model, messages, tools, options).await
    }

    async fn health_check(&self) -> OdinResult<bool> {
        let provider = self.resolve("")?;
        provider.health_check().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::openai_compat::OpenAiCompatProvider;

    #[test]
    fn test_registry_register_and_get() {
        let registry = ProviderRegistry::new();
        let provider = OpenAiCompatProvider::new(
            "test-provider",
            "http://localhost:11434/v1",
            None,
        );
        registry.register(provider);

        let retrieved = registry.get("test-provider");
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name(), "test-provider");
    }

    #[test]
    fn test_registry_list_names() {
        let registry = ProviderRegistry::new();
        registry.register(OpenAiCompatProvider::new("p1", "http://localhost:8080", None));
        registry.register(OpenAiCompatProvider::new("p2", "http://localhost:8081", None));

        let names = registry.list_names();
        assert!(names.contains(&"p1".to_string()));
        assert!(names.contains(&"p2".to_string()));
        assert_eq!(names.len(), 2);
    }

    #[test]
    fn test_registry_remove() {
        let registry = ProviderRegistry::new();
        registry.register(OpenAiCompatProvider::new("temp", "http://localhost:8080", None));
        assert!(registry.get("temp").is_some());

        registry.remove("temp");
        assert!(registry.get("temp").is_none());
    }
}
