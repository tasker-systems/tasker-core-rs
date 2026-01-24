//! # API Key Authentication
//!
//! Runtime API key validation with per-key permissions.

use std::collections::HashMap;

use super::security::{AuthMethod, SecurityContext};
use crate::config::tasker::ApiKeyConfig;
use crate::types::auth::AuthError;

/// Registry of valid API keys with their associated permissions.
#[derive(Debug, Clone)]
pub struct ApiKeyRegistry {
    keys: HashMap<String, ApiKeyEntry>,
}

#[derive(Debug, Clone)]
struct ApiKeyEntry {
    permissions: Vec<String>,
    description: String,
}

impl ApiKeyRegistry {
    /// Build the registry from configuration.
    pub fn from_config(configs: &[ApiKeyConfig]) -> Self {
        let keys = configs
            .iter()
            .map(|c| {
                let entry = ApiKeyEntry {
                    permissions: c.permissions.clone(),
                    description: c.description.clone(),
                };
                (c.key.clone(), entry)
            })
            .collect();
        Self { keys }
    }

    /// Validate an API key and return a `SecurityContext` on success.
    pub fn validate_key(&self, key: &str) -> Result<SecurityContext, AuthError> {
        match self.keys.get(key) {
            Some(entry) => Ok(SecurityContext {
                subject: entry.description.clone(),
                auth_method: AuthMethod::ApiKey {
                    description: entry.description.clone(),
                },
                permissions: entry.permissions.clone(),
                issuer: None,
                expires_at: None,
            }),
            None => Err(AuthError::InvalidToken("Invalid API key".to_string())),
        }
    }

    /// Check if the registry has any keys configured.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_key_returns_context() {
        let configs = vec![ApiKeyConfig {
            key: "test-key-123".to_string(),
            permissions: vec!["tasks:read".to_string(), "tasks:list".to_string()],
            description: "Test service".to_string(),
        }];
        let registry = ApiKeyRegistry::from_config(&configs);

        let ctx = registry.validate_key("test-key-123").unwrap();
        assert_eq!(ctx.subject, "Test service");
        assert_eq!(
            ctx.auth_method,
            AuthMethod::ApiKey {
                description: "Test service".to_string()
            }
        );
        assert_eq!(ctx.permissions, vec!["tasks:read", "tasks:list"]);
    }

    #[test]
    fn test_invalid_key_returns_error() {
        let configs = vec![ApiKeyConfig {
            key: "valid-key".to_string(),
            permissions: vec!["tasks:read".to_string()],
            description: "Valid".to_string(),
        }];
        let registry = ApiKeyRegistry::from_config(&configs);

        assert!(registry.validate_key("invalid-key").is_err());
    }

    #[test]
    fn test_multiple_keys() {
        let configs = vec![
            ApiKeyConfig {
                key: "key-a".to_string(),
                permissions: vec!["tasks:*".to_string()],
                description: "Service A".to_string(),
            },
            ApiKeyConfig {
                key: "key-b".to_string(),
                permissions: vec!["dlq:read".to_string()],
                description: "Service B".to_string(),
            },
        ];
        let registry = ApiKeyRegistry::from_config(&configs);

        let ctx_a = registry.validate_key("key-a").unwrap();
        assert_eq!(ctx_a.permissions, vec!["tasks:*"]);

        let ctx_b = registry.validate_key("key-b").unwrap();
        assert_eq!(ctx_b.permissions, vec!["dlq:read"]);
    }

    #[test]
    fn test_empty_registry() {
        let registry = ApiKeyRegistry::from_config(&[]);
        assert!(registry.is_empty());
        assert!(registry.validate_key("any-key").is_err());
    }
}
