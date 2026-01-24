//! # API Key Authentication
//!
//! Runtime API key validation with per-key permissions.
//! Uses constant-time comparison to prevent timing attacks.

use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use super::security::{AuthMethod, SecurityContext};
use crate::config::tasker::ApiKeyConfig;
use crate::types::auth::AuthError;

/// SHA-256 hash of an API key (32 bytes).
type KeyHash = [u8; 32];

/// Registry of valid API keys with their associated permissions.
///
/// Keys are stored as SHA-256 hashes and validated using constant-time
/// comparison to prevent timing attacks.
#[derive(Debug, Clone)]
pub struct ApiKeyRegistry {
    entries: Vec<HashedKeyEntry>,
}

#[derive(Debug, Clone)]
struct HashedKeyEntry {
    key_hash: KeyHash,
    permissions: Vec<String>,
    description: String,
}

impl ApiKeyRegistry {
    /// Build the registry from configuration.
    ///
    /// API keys are hashed at construction time; plaintext keys are not retained.
    pub fn from_config(configs: &[ApiKeyConfig]) -> Self {
        let entries = configs
            .iter()
            .map(|c| HashedKeyEntry {
                key_hash: Self::hash_key(&c.key),
                permissions: c.permissions.clone(),
                description: c.description.clone(),
            })
            .collect();
        Self { entries }
    }

    /// Validate an API key and return a `SecurityContext` on success.
    ///
    /// Uses constant-time comparison to prevent timing attacks. All stored
    /// key hashes are compared regardless of match position.
    pub fn validate_key(&self, key: &str) -> Result<SecurityContext, AuthError> {
        let input_hash = Self::hash_key(key);

        // Iterate all entries with constant-time comparison.
        // Track match index without short-circuiting to maintain constant time.
        let mut matched_index: Option<usize> = None;
        for (i, entry) in self.entries.iter().enumerate() {
            if input_hash.ct_eq(&entry.key_hash).into() {
                matched_index = Some(i);
            }
        }

        match matched_index {
            Some(i) => {
                let entry = &self.entries[i];
                Ok(SecurityContext {
                    subject: entry.description.clone(),
                    auth_method: AuthMethod::ApiKey {
                        description: entry.description.clone(),
                    },
                    permissions: entry.permissions.clone(),
                    issuer: None,
                    expires_at: None,
                })
            }
            None => Err(AuthError::InvalidToken("Invalid API key".to_string())),
        }
    }

    /// Check if the registry has any keys configured.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Hash a key using SHA-256.
    fn hash_key(key: &str) -> KeyHash {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        hasher.finalize().into()
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

    #[test]
    fn test_plaintext_keys_not_stored() {
        let configs = vec![ApiKeyConfig {
            key: "secret-key-value".to_string(),
            permissions: vec!["tasks:read".to_string()],
            description: "Test".to_string(),
        }];
        let registry = ApiKeyRegistry::from_config(&configs);

        // The debug output should not contain the plaintext key
        let debug_str = format!("{:?}", registry);
        assert!(!debug_str.contains("secret-key-value"));
    }

    #[test]
    fn test_similar_keys_distinguished() {
        let configs = vec![
            ApiKeyConfig {
                key: "key-alpha".to_string(),
                permissions: vec!["tasks:read".to_string()],
                description: "Alpha".to_string(),
            },
            ApiKeyConfig {
                key: "key-alphb".to_string(), // One character different
                permissions: vec!["dlq:read".to_string()],
                description: "Alphb".to_string(),
            },
        ];
        let registry = ApiKeyRegistry::from_config(&configs);

        let ctx = registry.validate_key("key-alpha").unwrap();
        assert_eq!(ctx.subject, "Alpha");
        assert_eq!(ctx.permissions, vec!["tasks:read"]);

        let ctx = registry.validate_key("key-alphb").unwrap();
        assert_eq!(ctx.subject, "Alphb");
        assert_eq!(ctx.permissions, vec!["dlq:read"]);

        assert!(registry.validate_key("key-alphc").is_err());
    }
}
