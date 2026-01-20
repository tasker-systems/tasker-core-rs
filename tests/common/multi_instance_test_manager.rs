//! # TAS-73: Multi-Instance Test Manager
//!
//! High-level test manager for multi-instance scenarios. Combines orchestration
//! cluster management with worker coordination and provides utilities for
//! validating concurrent behavior.
//!
//! ## Usage
//!
//! ```rust,ignore
//! // Setup 2 orchestration instances + 2 workers
//! let manager = MultiInstanceTestManager::setup(2, 2).await?;
//!
//! // Create tasks concurrently across the cluster
//! let responses = manager.create_tasks_concurrent(requests).await?;
//!
//! // Wait for all tasks to complete
//! for response in &responses {
//!     manager.wait_for_task_completion(response.task_uuid, timeout).await?;
//! }
//!
//! // Verify consistency across all instances
//! for response in &responses {
//!     manager.cluster.verify_task_consistency(response.task_uuid).await?;
//! }
//! ```

#![expect(
    dead_code,
    reason = "TAS-73: Test infrastructure for multi-instance testing"
)]

use super::orchestration_cluster::{ClusterConfig, OrchestrationCluster};
use anyhow::{bail, Result};
use std::time::Duration;
use tasker_client::{WorkerApiClient, WorkerApiConfig};
use tasker_shared::models::core::task_request::TaskRequest;
use tasker_shared::types::api::orchestration::{TaskCreationResponse, TaskResponse};
use uuid::Uuid;

/// Test manager for multi-instance scenarios
pub struct MultiInstanceTestManager {
    /// Orchestration cluster (multiple instances)
    pub cluster: OrchestrationCluster,
    /// Worker API clients (for health checking)
    pub worker_clients: Vec<WorkerApiClient>,
    /// Worker URLs
    pub worker_urls: Vec<String>,
}

impl MultiInstanceTestManager {
    /// Setup with N orchestration instances and M worker instances
    ///
    /// Uses default port allocation:
    /// - Orchestration: 8080 + (N-1)
    /// - Workers: 8100 + (M-1)
    pub async fn setup(orchestration_count: usize, worker_count: usize) -> Result<Self> {
        println!(
            "🚀 Setting up MultiInstanceTestManager ({} orch, {} workers)",
            orchestration_count, worker_count
        );

        let config = ClusterConfig::with_instances(orchestration_count, worker_count);
        let cluster = OrchestrationCluster::new(config.clone()).await?;

        // Create worker clients
        let mut worker_clients = Vec::with_capacity(config.worker_urls.len());
        for url in &config.worker_urls {
            let api_config = WorkerApiConfig {
                base_url: url.clone(),
                ..Default::default()
            };
            worker_clients.push(WorkerApiClient::new(api_config)?);
        }

        Ok(Self {
            cluster,
            worker_clients,
            worker_urls: config.worker_urls,
        })
    }

    /// Setup from environment (for CI/external service mode)
    pub async fn setup_from_env() -> Result<Self> {
        let config = ClusterConfig::from_env();
        let cluster = OrchestrationCluster::new(config.clone()).await?;

        let mut worker_clients = Vec::with_capacity(config.worker_urls.len());
        for url in &config.worker_urls {
            let api_config = WorkerApiConfig {
                base_url: url.clone(),
                ..Default::default()
            };
            worker_clients.push(WorkerApiClient::new(api_config)?);
        }

        Ok(Self {
            cluster,
            worker_clients,
            worker_urls: config.worker_urls,
        })
    }

    /// Wait for all orchestration and worker instances to become healthy
    pub async fn wait_for_healthy(&self, timeout: Duration) -> Result<()> {
        // Wait for orchestration instances
        self.cluster.wait_for_healthy(timeout).await?;

        // Wait for worker instances
        let deadline = std::time::Instant::now() + timeout;

        for (i, client) in self.worker_clients.iter().enumerate() {
            let url = &self.worker_urls[i];

            while std::time::Instant::now() < deadline {
                match client.health_check().await {
                    Ok(health) if health.status == "healthy" => {
                        println!("   ✅ Worker {} ({}) is healthy", i + 1, url);
                        break;
                    }
                    _ => {
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                }
            }

            let health = client.health_check().await?;
            if health.status != "healthy" {
                bail!(
                    "Worker {} ({}) did not become healthy within {:?}",
                    i + 1,
                    url,
                    timeout
                );
            }
        }

        Ok(())
    }

    /// Create N tasks concurrently across the cluster
    ///
    /// Tasks are distributed using the cluster's load balancing strategy.
    pub async fn create_tasks_concurrent(
        &self,
        requests: Vec<TaskRequest>,
    ) -> Result<Vec<TaskCreationResponse>> {
        use futures::future::join_all;

        let futures: Vec<_> = requests
            .into_iter()
            .map(|req| {
                let client = self.cluster.get_client().clone();
                async move { client.create_task(req).await }
            })
            .collect();

        let results = join_all(futures).await;

        // Collect results, propagating first error
        let mut responses = Vec::with_capacity(results.len());
        for result in results {
            responses.push(result?);
        }

        Ok(responses)
    }

    /// Wait for a task to reach a terminal state (Complete or Error)
    pub async fn wait_for_task_completion(
        &self,
        task_uuid: Uuid,
        timeout: Duration,
    ) -> Result<TaskResponse> {
        let deadline = std::time::Instant::now() + timeout;
        let poll_interval = Duration::from_millis(250);

        while std::time::Instant::now() < deadline {
            // Use round-robin to query different instances
            let client = self.cluster.get_client();
            let task = client.get_task(task_uuid).await?;

            match task.status.as_str() {
                // Terminal states - task lifecycle is complete
                "complete" | "all_complete" | "error" | "cancelled" => {
                    return Ok(task);
                }
                _ => {
                    tokio::time::sleep(poll_interval).await;
                }
            }
        }

        // Final check with consistency verification
        self.cluster.verify_task_consistency(task_uuid).await?;

        let task = self.cluster.get_client().get_task(task_uuid).await?;
        bail!(
            "Task {} did not complete within {:?}. Final state: {}",
            task_uuid,
            timeout,
            task.status
        );
    }

    /// Wait for multiple tasks to complete
    pub async fn wait_for_tasks_completion(
        &self,
        task_uuids: Vec<Uuid>,
        timeout: Duration,
    ) -> Result<Vec<TaskResponse>> {
        use futures::future::join_all;

        let futures: Vec<_> = task_uuids
            .into_iter()
            .map(|uuid| self.wait_for_task_completion(uuid, timeout))
            .collect();

        let results = join_all(futures).await;

        let mut responses = Vec::with_capacity(results.len());
        for result in results {
            responses.push(result?);
        }

        Ok(responses)
    }

    /// Verify task state is consistent across all orchestration instances
    pub async fn verify_task_consistency(&self, task_uuid: Uuid) -> Result<()> {
        self.cluster.verify_task_consistency(task_uuid).await
    }

    /// Get the number of orchestration instances
    pub fn orchestration_count(&self) -> usize {
        self.cluster.instance_count()
    }

    /// Get the number of worker instances
    pub fn worker_count(&self) -> usize {
        self.worker_clients.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multi_instance_test_manager_counts() {
        // Just test the config creation, not actual connections
        let config = ClusterConfig::with_instances(3, 2);
        assert_eq!(config.orchestration_urls.len(), 3);
        assert_eq!(config.worker_urls.len(), 2);
    }
}
