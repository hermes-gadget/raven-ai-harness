//! Odin Providers — Abstract model provider layer.
//!
//! Supports OpenAI-compatible APIs, Anthropic, and local models.
//! Designed so adding a new provider requires implementing one trait.

pub mod anthropic;
pub mod factory;
pub mod fallback;
pub mod local;
pub mod openai_compat;
pub mod registry;
pub mod traits;

pub use anthropic::AnthropicProvider;
pub use factory::create_provider;
pub use factory::create_provider_chain;
pub use local::LocalProvider;
pub use openai_compat::OpenAiCompatProvider;
pub use registry::ProviderRegistry;
pub use traits::ProviderExt;
