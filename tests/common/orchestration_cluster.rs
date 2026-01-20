//! # TAS-73: Orchestration Cluster Abstraction
//!
//! Manages multiple orchestration instances for multi-instance testing.
//! Provides load balancing, health checking, and coordinated operations
//! across N orchestration instances.
//!
//! ## Port Allocation
//!
//! Multi-instance mode uses these port ranges:
//! - Orchestration: 8080-8089 (up to 10 instances)
//! - Rust Workers:  8100-8109
//!
//! ## Usage
//!
//! ```rust,ignore
//! // Create cluster with 2 orchestration instances
//! let cluster = OrchestrationCluster::with_instances(2).await?;
//!
//! // Wait for all instances to become healthy
//! cluster.wait_for_healthy(Duration::from_secs(30)).await?;
//!
//! // Get a client using round-robin load balancing
//! let client = cluster.get_client();
//! let task = client.create_task(request).await?;
//!
//! // Verify state consistency across all instances
//! cluster.verify_task_consistency(task.task_uuid).await?;
//! ```

#![expect(
    dead_code,
    reason = "TAS-73: Test infrastructure for multi-instance testing"
)]

use anyhow::{bail, Result};
use std::env;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tasker_client::{OrchestrationApiClient, OrchestrationApiConfig};

/// Load balancing strategy for distributing requests across instances
#[derive(Debug, Clone, Default)]
pub enum LoadBalancingStrategy {
    /// Round-robin distribution (default)
    #[default]
    RoundRobin,
    /// Random selection
    Random,
    /// Always use first healthy instance
    FirstHealthy,
    /// Sticky routing based on a key (e.g., task UUID hash)
    Sticky(u64),
}

/// Configuration for an orchestration cluster
#[derive(Debug, Clone)]
pub struct ClusterConfig {
    /// URLs of orchestration instances
    pub orchestration_urls: Vec<String>,
    /// URLs of worker instances (for coordination)
    pub worker_urls: Vec<String>,
    /// Load balancing strategy
    pub load_balancing: LoadBalancingStrategy,
    /// Timeout for health checks
    pub health_timeout: Duration,
    /// Retry interval for health checks
    pub health_retry_interval: Duration,
}

impl Default for ClusterConfig {
    fn default() -> Self {
        Self {
            orchestration_urls: vec!["http://localhost:8080".to_string()],
            worker_urls: vec!["http://localhost:8100".to_string()],
            load_balancing: LoadBalancingStrategy::default(),
            health_timeout: Duration::from_secs(5),
            health_retry_interval: Duration::from_millis(500),
        }
    }
}

impl ClusterConfig {
    /// Create config from environment variables
    ///
    /// Reads:
    /// - `TASKER_TEST_ORCHESTRATION_URLS` - Comma-separated orchestration URLs
    /// - `TASKER_TEST_WORKER_URLS` - Comma-separated worker URLs
    pub fn from_env() -> Self {
        let orchestration_urls =
            Self::parse_urls_from_env("TASKER_TEST_ORCHESTRATION_URLS", &["http://localhost:8080"]);

        let worker_urls =
            Self::parse_urls_from_env("TASKER_TEST_WORKER_URLS", &["http://localhost:8100"]);

        Self {
            orchestration_urls,
            worker_urls,
            ..Default::default()
        }
    }

    /// Create config for N orchestration instances starting at base port
    pub fn with_orchestration_instances(count: usize, base_port: u16) -> Self {
        let orchestration_urls = (0..count)
            .map(|i| format!("http://localhost:{}", base_port + i as u16))
            .collect();

        Self {
            orchestration_urls,
            ..Default::default()
        }
    }

    /// Create config for N orchestration and M worker instances
    pub fn with_instances(orchestration_count: usize, worker_count: usize) -> Self {
        let orchestration_urls = (0..orchestration_count)
            .map(|i| format!("http://localhost:{}", 8080 + i as u16))
            .collect();

        let worker_urls = (0..worker_count)
            .map(|i| format!("http://localhost:{}", 8100 + i as u16))
            .collect();

        Self {
            orchestration_urls,
            worker_urls,
            ..Default::default()
        }
    }

    fn parse_urls_from_env(var_name: &str, defaults: &[&str]) -> Vec<String> {
        env::var(var_name)
            .map(|s| s.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_else(|_| defaults.iter().map(|s| s.to_string()).collect())
    }
}

/// Manages a cluster of orchestration instances for testing
pub struct OrchestrationCluster {
    /// API clients for each instance
    clients: Vec<OrchestrationApiClient>,
    /// Cluster configuration
    config: ClusterConfig,
    /// Counter for round-robin load balancing
    round_robin_counter: AtomicUsize,
}

impl OrchestrationCluster {
    /// Create a new cluster from configuration
    pub async fn new(config: ClusterConfig) -> Result<Self> {
        if config.orchestration_urls.is_empty() {
            bail!("OrchestrationCluster requires at least one orchestration URL");
        }

        let mut clients = Vec::with_capacity(config.orchestration_urls.len());
        for url in &config.orchestration_urls {
            let api_config = OrchestrationApiConfig {
                base_url: url.clone(),
                ..Default::default()
            };
            clients.push(OrchestrationApiClient::new(api_config)?);
        }

        println!(
            "🔗 Created OrchestrationCluster with {} instance(s)",
            clients.len()
        );
        for (i, url) in config.orchestration_urls.iter().enumerate() {
            println!("   Instance {}: {}", i + 1, url);
        }

        Ok(Self {
            clients,
            config,
            round_robin_counter: AtomicUsize::new(0),
        })
    }

    /// Create cluster with N instances at default ports (8080, 8081, ...)
    pub async fn with_instances(count: usize) -> Result<Self> {
        let config = ClusterConfig::with_orchestration_instances(count, 8080);
        Self::new(config).await
    }

    /// Create cluster from environment configuration
    pub async fn from_env() -> Result<Self> {
        let config = ClusterConfig::from_env();
        Self::new(config).await
    }

    /// Wait for all instances to become healthy
    pub async fn wait_for_healthy(&self, timeout: Duration) -> Result<()> {
        let deadline = std::time::Instant::now() + timeout;

        println!(
            "⏳ Waiting for {} instance(s) to become healthy...",
            self.clients.len()
        );

        for (i, client) in self.clients.iter().enumerate() {
            let url = &self.config.orchestration_urls[i];
            let mut is_healthy = false;

            while std::time::Instant::now() < deadline {
                match client.health_check().await {
                    Ok(()) => {
                        println!("   ✅ Instance {} ({}) is healthy", i + 1, url);
                        is_healthy = true;
                        break;
                    }
                    Err(e) => {
                        println!("   ⏳ Instance {} ({}) not ready: {}", i + 1, url, e);
                    }
                }
                tokio::time::sleep(self.config.health_retry_interval).await;
            }

            // Final check
            if !is_healthy {
                client.health_check().await.map_err(|e| {
                    anyhow::anyhow!(
                        "Instance {} ({}) did not become healthy within {:?}: {}",
                        i + 1,
                        url,
                        timeout,
                        e
                    )
                })?;
            }
        }

        println!("✅ All {} instance(s) healthy", self.clients.len());
        Ok(())
    }

    /// Get a client using the configured load balancing strategy
    pub fn get_client(&self) -> &OrchestrationApiClient {
        match &self.config.load_balancing {
            LoadBalancingStrategy::RoundRobin => {
                let idx = self.round_robin_counter.fetch_add(1, Ordering::Relaxed);
                &self.clients[idx % self.clients.len()]
            }
            LoadBalancingStrategy::Random => {
                // Use fastrand instead of rand for simpler dependency management
                let idx = fastrand::usize(0..self.clients.len());
                &self.clients[idx]
            }
            LoadBalancingStrategy::FirstHealthy => {
                // For now, just return first (could add async health checking)
                &self.clients[0]
            }
            LoadBalancingStrategy::Sticky(key) => {
                let idx = (*key as usize) % self.clients.len();
                &self.clients[idx]
            }
        }
    }

    /// Get all clients (for parallel operations or validation)
    pub fn all_clients(&self) -> &[OrchestrationApiClient] {
        &self.clients
    }

    /// Get specific client by index
    pub fn client(&self, index: usize) -> Option<&OrchestrationApiClient> {
        self.clients.get(index)
    }

    /// Number of instances in the cluster
    pub fn instance_count(&self) -> usize {
        self.clients.len()
    }

    /// Get the URLs of all orchestration instances
    pub fn urls(&self) -> &[String] {
        &self.config.orchestration_urls
    }

    /// Set load balancing strategy
    pub fn set_load_balancing(&mut self, strategy: LoadBalancingStrategy) {
        self.config.load_balancing = strategy;
    }

    /// Verify task state is consistent across all orchestration instances
    ///
    /// Queries all instances and ensures they report the same task state.
    /// This validates that state is properly synchronized in the database.
    pub async fn verify_task_consistency(&self, task_uuid: uuid::Uuid) -> Result<()> {
        let mut states = Vec::new();
        let mut errors = Vec::new();

        for (i, client) in self.clients.iter().enumerate() {
            match client.get_task(task_uuid).await {
                Ok(task) => {
                    states.push((i, task.status.clone()));
                }
                Err(e) => {
                    errors.push((i, e.to_string()));
                }
            }
        }

        // Check for errors
        if !errors.is_empty() {
            let error_details: Vec<String> = errors
                .iter()
                .map(|(i, e)| format!("Instance {}: {}", i + 1, e))
                .collect();
            bail!(
                "Failed to query task {} from some instances:\n{}",
                task_uuid,
                error_details.join("\n")
            );
        }

        // Check for consistency
        if states.is_empty() {
            bail!("No instances returned task state for {}", task_uuid);
        }

        let first_state = &states[0].1;
        for (i, state) in states.iter().skip(1) {
            if state != first_state {
                bail!(
                    "State inconsistency for task {}: Instance 1 reports '{}', Instance {} reports '{}'",
                    task_uuid,
                    first_state,
                    i + 1,
                    state
                );
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cluster_config_default() {
        let config = ClusterConfig::default();
        assert_eq!(config.orchestration_urls.len(), 1);
        assert_eq!(config.orchestration_urls[0], "http://localhost:8080");
    }

    #[test]
    fn test_cluster_config_with_instances() {
        let config = ClusterConfig::with_orchestration_instances(3, 8080);
        assert_eq!(config.orchestration_urls.len(), 3);
        assert_eq!(config.orchestration_urls[0], "http://localhost:8080");
        assert_eq!(config.orchestration_urls[1], "http://localhost:8081");
        assert_eq!(config.orchestration_urls[2], "http://localhost:8082");
    }

    #[test]
    fn test_cluster_config_with_both_instances() {
        let config = ClusterConfig::with_instances(2, 3);
        assert_eq!(config.orchestration_urls.len(), 2);
        assert_eq!(config.worker_urls.len(), 3);
        assert_eq!(config.orchestration_urls[0], "http://localhost:8080");
        assert_eq!(config.worker_urls[0], "http://localhost:8100");
    }
}
