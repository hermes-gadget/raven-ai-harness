//! Discord integration stub.
//!
//! This module provides the scaffolding for Discord bot integration
//! via the Discord API. The full implementation will handle:
//! - Message events (create, update, delete)
//! - Slash commands
//! - Thread management
//! - Rate limiting

use odin_core::error::OdinResult;

/// Configuration for the Discord gateway.
#[derive(Debug, Clone)]
pub struct DiscordConfig {
    /// Whether the Discord gateway is enabled.
    pub enabled: bool,

    /// Discord bot token.
    pub token: Option<String>,
}

impl Default for DiscordConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            token: None,
        }
    }
}

/// A stub for the Discord integration.
///
/// To be implemented with a Discord API library (e.g., serenity or twilight).
#[derive(Debug, Clone)]
pub struct DiscordGateway {
    /// Configuration.
    pub config: DiscordConfig,
}

impl DiscordGateway {
    /// Create a new Discord gateway.
    pub fn new(config: DiscordConfig) -> Self {
        Self { config }
    }

    /// Start the Discord gateway (stub).
    ///
    /// In production, this would connect to the Discord API and
    /// begin receiving events.
    pub async fn start(&self) -> OdinResult<()> {
        if !self.config.enabled {
            tracing::info!("[DISCORD] Gateway disabled");
            return Ok(());
        }

        if self.config.token.is_none() {
            tracing::warn!("[DISCORD] No token configured — gateway cannot start");
            return Ok(());
        }

        tracing::info!(
            "[DISCORD] Gateway would start (token configured: {})",
            self.config.token.as_ref().map(|_| "yes").unwrap_or("no")
        );

        // TODO: Connect to Discord API
        // - Authenticate with the bot token
        // - Register slash commands
        // - Start receiving events
        // - Map messages to agent tasks

        Ok(())
    }

    /// Stop the Discord gateway (stub).
    pub async fn stop(&self) -> OdinResult<()> {
        tracing::info!("[DISCORD] Gateway would stop");
        // TODO: Gracefully disconnect from Discord API
        Ok(())
    }

    /// Send a message to a Discord channel (stub).
    pub async fn send_message(&self, _channel_id: &str, _content: &str) -> OdinResult<()> {
        tracing::info!("[DISCORD] Would send message to channel {_channel_id}");
        // TODO: Send message via Discord API
        Ok(())
    }

    /// Check if the gateway is connected (stub).
    pub fn is_connected(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discord_config_default() {
        let config = DiscordConfig::default();
        assert!(!config.enabled);
        assert!(config.token.is_none());
    }

    #[test]
    fn test_discord_gateway_creation() {
        let config = DiscordConfig {
            enabled: false,
            token: None,
        };
        let gateway = DiscordGateway::new(config);
        assert!(!gateway.is_connected());
    }

    #[tokio::test]
    async fn test_start_disabled_gateway() {
        let config = DiscordConfig::default();
        let gateway = DiscordGateway::new(config);
        let result = gateway.start().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_start_without_token() {
        let config = DiscordConfig {
            enabled: true,
            token: None,
        };
        let gateway = DiscordGateway::new(config);
        let result = gateway.start().await;
        assert!(result.is_ok()); // warnings but no error
    }

    #[tokio::test]
    async fn test_stop_gateway() {
        let gateway = DiscordGateway::new(DiscordConfig::default());
        let result = gateway.stop().await;
        assert!(result.is_ok());
    }
}
