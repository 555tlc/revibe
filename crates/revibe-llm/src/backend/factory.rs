//! Backend factory for creating LLM backends.

use super::{Backend, GenericBackend, MistralBackend, ProviderConfig};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

/// Type of backend to use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BackendType {
    /// Mistral-specific backend.
    Mistral,
    /// Generic OpenAI-compatible backend.
    #[default]
    Generic,
}

impl std::fmt::Display for BackendType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Mistral => write!(f, "mistral"),
            Self::Generic => write!(f, "generic"),
        }
    }
}

impl std::str::FromStr for BackendType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "mistral" => Ok(Self::Mistral),
            "generic" | "openai" => Ok(Self::Generic),
            _ => Err(format!("Unknown backend type: {s}")),
        }
    }
}

/// Factory for creating LLM backends.
pub struct BackendFactory;

impl BackendFactory {
    /// Create a backend for the given provider configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the backend cannot be created.
    pub fn create(
        provider: &ProviderConfig,
        timeout: Duration,
    ) -> Result<Arc<dyn Backend>, String> {
        match provider.backend {
            BackendType::Mistral => {
                let backend =
                    MistralBackend::new(provider.clone(), timeout).map_err(|e| e.to_string())?;
                Ok(Arc::new(backend))
            }
            BackendType::Generic => {
                let backend =
                    GenericBackend::new(provider.clone(), timeout).map_err(|e| e.to_string())?;
                Ok(Arc::new(backend))
            }
        }
    }
}
