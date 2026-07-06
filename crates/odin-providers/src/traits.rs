//! Extended traits and utilities for providers.

use async_trait::async_trait;
use odin_core::error::OdinResult;
use odin_core::traits::Provider;
use odin_core::types::*;

/// Extended provider capabilities beyond the base Provider trait.
#[async_trait]
pub trait ProviderExt: Provider {
    /// Get a completion without tool calling (simpler interface).
    async fn simple_completion(
        &self,
        model: &str,
        system_prompt: &str,
        user_message: &str,
    ) -> OdinResult<String> {
        let messages = vec![
            Message::system(system_prompt),
            Message::user(user_message),
        ];
        let options = CompletionOptions::default();
        let response = self.chat(model, &messages, &[], &options).await?;
        Ok(response.message.text().unwrap_or("").to_string())
    }

    /// Check if a specific model is available.
    async fn model_available(&self, model: &str) -> OdinResult<bool> {
        let models = self.list_models().await?;
        Ok(models.iter().any(|m| m.id == model))
    }

    /// Get the default model for this provider.
    async fn default_model(&self) -> Option<String> {
        self.list_models()
            .await
            .ok()
            .and_then(|models| models.first().map(|m| m.id.clone()))
    }
}

/// Automatically implement ProviderExt for all Provider implementors.
impl<T: Provider> ProviderExt for T {}
