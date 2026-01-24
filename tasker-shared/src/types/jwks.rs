//! # JWKS Key Store
//!
//! Async JWKS (JSON Web Key Set) key fetching with caching and periodic refresh.
//! Used for dynamic key rotation when validating JWT tokens from external providers.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jsonwebtoken::DecodingKey;
use serde::Deserialize;
use tokio::sync::RwLock;
use tracing::{debug, error, warn};

use crate::types::auth::AuthError;

/// A cached JWKS entry with expiration tracking.
#[derive(Debug)]
struct JwksCacheEntry {
    keys: HashMap<String, DecodingKey>,
    fetched_at: Instant,
}

/// Async JWKS key store with caching and refresh.
///
/// Fetches keys from a JWKS endpoint, caches them, and refreshes periodically
/// or on cache miss.
#[derive(Debug)]
pub struct JwksKeyStore {
    cache: Arc<RwLock<Option<JwksCacheEntry>>>,
    jwks_url: String,
    refresh_interval: Duration,
    client: reqwest::Client,
}

/// JWKS response format (RFC 7517).
#[derive(Debug, Deserialize)]
struct JwksResponse {
    keys: Vec<JwkKey>,
}

/// A single JWK key entry.
#[derive(Debug, Deserialize)]
struct JwkKey {
    /// Key ID
    kid: Option<String>,
    /// Key type (must be "RSA")
    kty: String,
    /// RSA modulus (Base64url-encoded)
    n: Option<String>,
    /// RSA exponent (Base64url-encoded)
    e: Option<String>,
    /// Key use (should be "sig")
    #[serde(rename = "use")]
    key_use: Option<String>,
    /// Algorithm (e.g., "RS256")
    alg: Option<String>,
}

impl JwksKeyStore {
    /// Create a new JWKS key store and perform the initial fetch.
    pub async fn new(url: String, refresh_interval: Duration) -> Result<Self, AuthError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| {
                AuthError::ConfigurationError(format!("Failed to create HTTP client: {e}"))
            })?;

        let store = Self {
            cache: Arc::new(RwLock::new(None)),
            jwks_url: url,
            refresh_interval,
            client,
        };

        // Perform initial fetch
        store.refresh_keys().await?;

        Ok(store)
    }

    /// Get a decoding key by key ID (kid).
    ///
    /// If the key is not in cache or the cache is stale, triggers a refresh.
    pub async fn get_key(&self, kid: &str) -> Result<DecodingKey, AuthError> {
        // Try cache first
        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.as_ref() {
                if entry.fetched_at.elapsed() < self.refresh_interval {
                    if let Some(key) = entry.keys.get(kid) {
                        return Ok(key.clone());
                    }
                }
            }
        }

        // Cache miss or stale: refresh
        debug!(kid = %kid, "JWKS cache miss, refreshing");
        self.refresh_keys().await?;

        // Try again after refresh
        let cache = self.cache.read().await;
        match cache.as_ref().and_then(|entry| entry.keys.get(kid)) {
            Some(key) => Ok(key.clone()),
            None => Err(AuthError::InvalidToken(format!(
                "Key ID '{kid}' not found in JWKS"
            ))),
        }
    }

    /// Refresh the key cache from the JWKS endpoint.
    async fn refresh_keys(&self) -> Result<(), AuthError> {
        debug!(url = %self.jwks_url, "Fetching JWKS keys");

        let response = self.client.get(&self.jwks_url).send().await.map_err(|e| {
            error!(url = %self.jwks_url, error = %e, "JWKS fetch failed");
            AuthError::ConfigurationError(format!("JWKS fetch failed: {e}"))
        })?;

        if !response.status().is_success() {
            let status = response.status();
            warn!(url = %self.jwks_url, status = %status, "JWKS endpoint returned error");
            return Err(AuthError::ConfigurationError(format!(
                "JWKS endpoint returned {status}"
            )));
        }

        let jwks: JwksResponse = response.json().await.map_err(|e| {
            AuthError::ConfigurationError(format!("Failed to parse JWKS response: {e}"))
        })?;

        let mut keys = HashMap::new();
        for jwk in &jwks.keys {
            if jwk.kty != "RSA" {
                continue;
            }
            if let Some(key_use) = &jwk.key_use {
                if key_use != "sig" {
                    continue;
                }
            }

            let Some(kid) = &jwk.kid else {
                continue;
            };
            let Some(n) = &jwk.n else {
                continue;
            };
            let Some(e) = &jwk.e else {
                continue;
            };

            match DecodingKey::from_rsa_components(n, e) {
                Ok(decoding_key) => {
                    debug!(kid = %kid, alg = ?jwk.alg, "Loaded JWKS key");
                    keys.insert(kid.clone(), decoding_key);
                }
                Err(err) => {
                    warn!(kid = %kid, error = %err, "Failed to parse JWKS key components");
                }
            }
        }

        if keys.is_empty() {
            warn!(url = %self.jwks_url, "No valid RSA signing keys found in JWKS response");
        } else {
            debug!(count = keys.len(), "JWKS keys refreshed");
        }

        let mut cache = self.cache.write().await;
        *cache = Some(JwksCacheEntry {
            keys,
            fetched_at: Instant::now(),
        });

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jwk_key_deserialization() {
        let json = r#"{
            "keys": [{
                "kty": "RSA",
                "kid": "test-key-1",
                "use": "sig",
                "alg": "RS256",
                "n": "0vx7agoebGcQSuuPiLJXZptN9nndrQmbXEps2aiAFbWhM78LhWx4cbbfAAtVT86zwu1RK7aPFFxuhDR1L6tSoc_BJECPebWKRXjBZCiFV4n3oknjhMstn64tZ_2W-5JsGY4Hc5n9yBXArwl93lqt7_RN5w6Cf0h4QyQ5v-65YGjQR0_FDW2QvzqY368QQMicAtaSqzs8KJZgnYb9c7d0zgdAZHzu6qMQvRL5hajrn1n91CbOpbISD08qNLyrdkt-bFTWhAI4vMQFh6WeZu0fM4lFd2NcRwr3XPksINHaQ-G_xBniIqbw0Ls1jF44-csFCur-kEgU8awapJzKnqDKgw",
                "e": "AQAB"
            }]
        }"#;
        let jwks: JwksResponse = serde_json::from_str(json).unwrap();
        assert_eq!(jwks.keys.len(), 1);
        assert_eq!(jwks.keys[0].kid.as_deref(), Some("test-key-1"));
        assert_eq!(jwks.keys[0].kty, "RSA");
    }

    #[test]
    fn test_jwk_key_filter_non_rsa() {
        let json = r#"{
            "keys": [{
                "kty": "EC",
                "kid": "ec-key",
                "use": "sig"
            }]
        }"#;
        let jwks: JwksResponse = serde_json::from_str(json).unwrap();
        // EC keys should be filtered out during processing
        assert_eq!(jwks.keys[0].kty, "EC");
    }
}
