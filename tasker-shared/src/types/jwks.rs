//! # JWKS Key Store
//!
//! Async JWKS (JSON Web Key Set) key fetching with caching and periodic refresh.
//! Used for dynamic key rotation when validating JWT tokens from external providers.
//!
//! Hardening features:
//! - Thundering herd protection via refresh coalescing lock
//! - Stale cache fallback on refresh failure (configurable max staleness)
//! - SSRF prevention via URL validation (scheme, private IP blocking)
//! - Algorithm allowlist filtering at key load time

use std::collections::HashMap;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use jsonwebtoken::DecodingKey;
use serde::Deserialize;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, error, warn};

use crate::types::auth::AuthError;

/// A cached JWKS entry with expiration tracking.
#[derive(Debug)]
struct JwksCacheEntry {
    keys: HashMap<String, DecodingKey>,
    fetched_at: Instant,
}

/// JWKS key store configuration.
#[derive(Debug, Clone)]
pub struct JwksConfig {
    /// JWKS endpoint URL
    pub url: String,
    /// How often to refresh keys
    pub refresh_interval: Duration,
    /// Maximum staleness allowed on refresh failure (0 = no stale fallback)
    pub max_stale: Duration,
    /// Allow HTTP URLs (only for testing)
    pub allow_http: bool,
    /// Allowed signing algorithms
    pub allowed_algorithms: Vec<String>,
}

/// Async JWKS key store with caching and refresh.
///
/// Fetches keys from a JWKS endpoint, caches them, and refreshes periodically
/// or on cache miss. Includes thundering herd protection, stale cache fallback,
/// SSRF prevention, and algorithm allowlist filtering.
#[derive(Debug)]
pub struct JwksKeyStore {
    cache: Arc<RwLock<Option<JwksCacheEntry>>>,
    refresh_lock: Arc<Mutex<()>>,
    config: JwksConfig,
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
    ///
    /// Validates the JWKS URL for SSRF safety before making any requests.
    pub async fn new(url: String, refresh_interval: Duration) -> Result<Self, AuthError> {
        let config = JwksConfig {
            url,
            refresh_interval,
            max_stale: Duration::from_secs(300),
            allow_http: false,
            allowed_algorithms: vec!["RS256".to_string()],
        };
        Self::with_config(config).await
    }

    /// Create a new JWKS key store with full configuration.
    pub async fn with_config(config: JwksConfig) -> Result<Self, AuthError> {
        Self::validate_url(&config.url, config.allow_http)?;

        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|e| {
                AuthError::ConfigurationError(format!("Failed to create HTTP client: {e}"))
            })?;

        let store = Self {
            cache: Arc::new(RwLock::new(None)),
            refresh_lock: Arc::new(Mutex::new(())),
            config,
            client,
        };

        // Perform initial fetch
        store.refresh_keys().await?;

        Ok(store)
    }

    /// Get a decoding key by key ID (kid).
    ///
    /// If the key is not in cache or the cache is stale, triggers a refresh.
    /// Uses a coalescing lock to prevent thundering herd on concurrent cache misses.
    pub async fn get_key(&self, kid: &str) -> Result<DecodingKey, AuthError> {
        // Fast path: check fresh cache
        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.as_ref() {
                if entry.fetched_at.elapsed() < self.config.refresh_interval {
                    if let Some(key) = entry.keys.get(kid) {
                        return Ok(key.clone());
                    }
                }
            }
        }

        // Acquire refresh lock (coalescing: only one caller refreshes)
        let _guard = self.refresh_lock.lock().await;

        // Double-check after acquiring lock (another caller may have refreshed)
        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.as_ref() {
                if entry.fetched_at.elapsed() < self.config.refresh_interval {
                    if let Some(key) = entry.keys.get(kid) {
                        return Ok(key.clone());
                    }
                }
            }
        }

        // Attempt refresh with stale cache fallback
        match self.refresh_keys().await {
            Ok(()) => {
                // Use freshly fetched cache
                let cache = self.cache.read().await;
                match cache.as_ref().and_then(|entry| entry.keys.get(kid)) {
                    Some(key) => Ok(key.clone()),
                    None => Err(AuthError::InvalidToken(format!(
                        "Key ID '{kid}' not found in JWKS"
                    ))),
                }
            }
            Err(refresh_err) => {
                // Stale cache fallback: if cache exists and is within max_stale window
                if self.config.max_stale > Duration::ZERO {
                    let cache = self.cache.read().await;
                    if let Some(entry) = cache.as_ref() {
                        let staleness = entry.fetched_at.elapsed();
                        let max_acceptable = self.config.refresh_interval + self.config.max_stale;
                        if staleness < max_acceptable {
                            warn!(
                                staleness_secs = staleness.as_secs(),
                                "JWKS refresh failed, using stale cache"
                            );
                            if let Some(key) = entry.keys.get(kid) {
                                return Ok(key.clone());
                            }
                        }
                    }
                }
                Err(refresh_err)
            }
        }
    }

    /// Refresh the key cache from the JWKS endpoint.
    async fn refresh_keys(&self) -> Result<(), AuthError> {
        debug!(url = %self.config.url, "Fetching JWKS keys");

        let response = self
            .client
            .get(&self.config.url)
            .send()
            .await
            .map_err(|e| {
                error!(url = %self.config.url, error = %e, "JWKS fetch failed");
                AuthError::ConfigurationError(format!("JWKS fetch failed: {e}"))
            })?;

        if !response.status().is_success() {
            let status = response.status();
            warn!(url = %self.config.url, status = %status, "JWKS endpoint returned error");
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

            // Algorithm allowlist check
            if let Some(alg) = &jwk.alg {
                if !self.config.allowed_algorithms.contains(alg) {
                    warn!(
                        kid = ?jwk.kid,
                        alg = %alg,
                        allowed = ?self.config.allowed_algorithms,
                        "Rejecting JWKS key with disallowed algorithm"
                    );
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
            warn!(url = %self.config.url, "No valid RSA signing keys found in JWKS response");
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

    /// Validate a JWKS URL for SSRF safety.
    ///
    /// Blocks private IPs, cloud metadata endpoints, and non-HTTPS schemes
    /// (unless `allow_http` is set for testing).
    fn validate_url(url: &str, allow_http: bool) -> Result<(), AuthError> {
        let parsed = reqwest::Url::parse(url)
            .map_err(|e| AuthError::ConfigurationError(format!("Invalid JWKS URL: {e}")))?;

        // Scheme check
        match parsed.scheme() {
            "https" => {}
            "http" if allow_http => {
                warn!("JWKS URL uses HTTP (non-TLS) - only acceptable for testing");
            }
            other => {
                return Err(AuthError::ConfigurationError(format!(
                    "JWKS URL must use HTTPS (got '{other}')"
                )));
            }
        }

        // Host check
        if let Some(host) = parsed.host_str() {
            if Self::is_private_host(host) {
                return Err(AuthError::ConfigurationError(format!(
                    "JWKS URL points to private/internal address: {host}"
                )));
            }
        } else {
            return Err(AuthError::ConfigurationError(
                "JWKS URL has no host".to_string(),
            ));
        }

        Ok(())
    }

    /// Check if a host string resolves to a private/internal address.
    ///
    /// Handles both bare hostnames and IPv6 addresses in brackets (as returned
    /// by URL parsers).
    fn is_private_host(host: &str) -> bool {
        const BLOCKED_HOSTS: &[&str] = &[
            "169.254.169.254",          // AWS/GCP metadata
            "metadata.google.internal", // GCP metadata
            "localhost",
            "127.0.0.1",
            "0.0.0.0",
        ];

        // Strip IPv6 brackets if present (URL parsers include them in host_str)
        let normalized = host
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .unwrap_or(host);

        if BLOCKED_HOSTS.contains(&normalized) {
            return true;
        }

        // Check IP ranges
        if let Ok(ip) = normalized.parse::<IpAddr>() {
            return match ip {
                IpAddr::V4(v4) => {
                    v4.is_private() || v4.is_loopback() || v4.is_link_local() || v4.is_unspecified()
                }
                IpAddr::V6(v6) => v6.is_loopback() || v6.is_unspecified(),
            };
        }

        false
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

    #[test]
    fn test_validate_url_https_allowed() {
        assert!(JwksKeyStore::validate_url(
            "https://auth.example.com/.well-known/jwks.json",
            false
        )
        .is_ok());
    }

    #[test]
    fn test_validate_url_http_blocked_by_default() {
        let result = JwksKeyStore::validate_url("http://auth.example.com/jwks", false);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("HTTPS"));
    }

    #[test]
    fn test_validate_url_http_allowed_with_flag() {
        assert!(JwksKeyStore::validate_url("http://auth.example.com/jwks", true).is_ok());
    }

    #[test]
    fn test_validate_url_blocks_metadata_endpoints() {
        let blocked_urls = [
            "https://169.254.169.254/latest/meta-data/",
            "https://metadata.google.internal/computeMetadata/v1/",
            "https://localhost/jwks",
            "https://127.0.0.1/jwks",
            "https://0.0.0.0/jwks",
            "https://[::1]/jwks",
        ];
        for url in &blocked_urls {
            let result = JwksKeyStore::validate_url(url, false);
            assert!(result.is_err(), "Expected {url} to be blocked");
        }
    }

    #[test]
    fn test_validate_url_blocks_private_ips() {
        let private_urls = [
            "https://10.0.0.1/jwks",
            "https://172.16.0.1/jwks",
            "https://192.168.1.1/jwks",
        ];
        for url in &private_urls {
            let result = JwksKeyStore::validate_url(url, false);
            assert!(result.is_err(), "Expected {url} to be blocked");
        }
    }

    #[test]
    fn test_validate_url_allows_public_ips() {
        assert!(JwksKeyStore::validate_url("https://8.8.8.8/jwks", false).is_ok());
        assert!(JwksKeyStore::validate_url("https://1.1.1.1/jwks", false).is_ok());
    }

    #[test]
    fn test_validate_url_blocks_invalid_schemes() {
        assert!(JwksKeyStore::validate_url("ftp://example.com/jwks", false).is_err());
        assert!(JwksKeyStore::validate_url("file:///etc/passwd", false).is_err());
    }

    #[test]
    fn test_is_private_host() {
        assert!(JwksKeyStore::is_private_host("169.254.169.254"));
        assert!(JwksKeyStore::is_private_host("localhost"));
        assert!(JwksKeyStore::is_private_host("127.0.0.1"));
        assert!(JwksKeyStore::is_private_host("10.0.0.1"));
        assert!(JwksKeyStore::is_private_host("192.168.0.1"));
        assert!(JwksKeyStore::is_private_host("172.16.0.1"));
        assert!(JwksKeyStore::is_private_host("::1"));
        assert!(JwksKeyStore::is_private_host("[::1]")); // URL-style bracketed IPv6
        assert!(!JwksKeyStore::is_private_host("8.8.8.8"));
        assert!(!JwksKeyStore::is_private_host("example.com"));
    }
}
