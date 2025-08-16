//! # Orchestration System (pgmq-based)
//!
//! Main orchestration system using the OrchestrationLoop for individual step processing.
//! Implements priority fairness task claiming and autonomous queue-based step execution.

use crate::error::{Result, TaskerError};
use crate::messaging::{PgmqClientTrait, UnifiedPgmqClient};
use crate::orchestration::orchestration_loop::{
    ContinuousOrchestrationSummary, OrchestrationCycleResult, OrchestrationLoop,
    OrchestrationLoopConfig,
};
use chrono;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use tracing::{debug, info, instrument, warn};

/// Configuration for the orchestration system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationSystemConfig {
    /// Queue name for task requests
    pub task_requests_queue_name: String,
    /// Orchestrator instance identifier
    pub orchestrator_id: String,
    /// Orchestration loop configuration
    pub orchestration_loop_config: OrchestrationLoopConfig,
    /// Task request processor polling interval in milliseconds
    pub task_request_polling_interval_ms: u64,
    /// Visibility timeout for task request messages (seconds)
    pub task_request_visibility_timeout_seconds: i32,
    /// Number of task requests to process per batch
    pub task_request_batch_size: i32,
    /// Namespaces to create queues for
    pub active_namespaces: Vec<String>,
    /// Maximum concurrent orchestration loops
    pub max_concurrent_orchestrators: usize,
    /// Enable comprehensive performance logging
    pub enable_performance_logging: bool,
}

impl Default for OrchestrationSystemConfig {
    fn default() -> Self {
        use std::time::SystemTime;

        // Generate a simple orchestrator ID using timestamp
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        Self {
            task_requests_queue_name: "task_requests_queue".to_string(),
            orchestrator_id: format!("orchestrator-{timestamp}"),
            orchestration_loop_config: OrchestrationLoopConfig::default(),
            task_request_polling_interval_ms: 250, // 250ms = 4x/sec default
            task_request_visibility_timeout_seconds: 300, // 5 minutes
            task_request_batch_size: 10,
            active_namespaces: vec![
                "fulfillment".to_string(),
                "inventory".to_string(),
                "notifications".to_string(),
                "payments".to_string(),
                "analytics".to_string(),
            ],
            max_concurrent_orchestrators: 3,
            enable_performance_logging: false,
        }
    }
}

impl OrchestrationSystemConfig {
    /// Create OrchestrationSystemConfig from ConfigManager
    pub fn from_config_manager(config_manager: &crate::config::ConfigManager) -> Self {
        use std::time::SystemTime;

        let config = config_manager.config();

        // Generate a simple orchestrator ID using timestamp
        let timestamp = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        Self {
            task_requests_queue_name: config.orchestration.task_requests_queue_name.clone(),
            orchestrator_id: format!("orchestrator-{timestamp}"),
            orchestration_loop_config: OrchestrationLoopConfig::from_config_manager(config_manager),
            task_request_polling_interval_ms: config.orchestration.task_request_polling_interval_ms,
            task_request_visibility_timeout_seconds: config
                .orchestration
                .task_request_visibility_timeout_seconds
                as i32,
            task_request_batch_size: config.orchestration.task_request_batch_size as i32,
            active_namespaces: config.orchestration.active_namespaces.clone(),
            max_concurrent_orchestrators: config.orchestration.max_concurrent_orchestrators
                as usize,
            enable_performance_logging: config.orchestration.enable_performance_logging,
        }
    }
}

/// Main orchestration system using OrchestrationLoop for individual step processing
pub struct OrchestrationSystem {
    /// PostgreSQL message queue client (unified for circuit breaker support)
    pgmq_client: Arc<UnifiedPgmqClient>,
    /// Main orchestration loop
    orchestration_loop: Arc<OrchestrationLoop>,
    /// Configuration
    config: OrchestrationSystemConfig,
}

impl OrchestrationSystem {
    /// Create a new orchestration system
    pub async fn new(
        pgmq_client: Arc<UnifiedPgmqClient>,
        pool: PgPool,
        config: OrchestrationSystemConfig,
    ) -> Result<Self> {
        // Create orchestration loop with the unified pgmq client (supports circuit breakers)
        let orchestration_loop = Arc::new(
            OrchestrationLoop::with_unified_client(
                pool.clone(),
                pgmq_client.clone(),
                config.orchestrator_id.clone(),
                config.orchestration_loop_config.clone(),
            )
            .await?,
        );

        Ok(Self {
            pgmq_client,
            orchestration_loop,
            config,
        })
    }

    /// Bootstrap orchestration system from YAML configuration
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use tasker_core::orchestration::{OrchestrationSystem, config::ConfigurationManager};
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     // Load configuration using component-based config
    ///     let config_manager = ConfigurationManager::new();
    ///
    ///     // Bootstrap orchestration system with unified architecture
    ///     let orchestration_system = OrchestrationSystem::from_config(
    ///         config_manager,
    ///     ).await?;
    ///
    ///     // Start orchestration
    ///     orchestration_system.start().await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn from_config(
        config_manager: crate::orchestration::config::ConfigurationManager,
    ) -> Result<Self> {
        info!("🚀 Creating OrchestrationSystem from config via unified OrchestrationCore");

        let tasker_config = config_manager.system_config();
        let orchestration_system_config =
            tasker_config.orchestration.to_orchestration_system_config();

        // Use OrchestrationCore with unified configuration-driven initialization
        // This provides the unified bootstrap and circuit breaker integration
        // Use ConfigManager directly for environment-aware bootstrap
        let orchestration_core = crate::orchestration::OrchestrationCore::new().await?;

        info!("✅ OrchestrationCore created with unified bootstrap - circuit breaker integration resolved");

        // Extract components from OrchestrationCore for compatibility wrapper
        // Use the unified client directly (supports circuit breakers)
        let pgmq_client = orchestration_core.pgmq_client();

        Self::new(
            pgmq_client,
            orchestration_core.database_pool().clone(),
            orchestration_system_config,
        )
        .await
    }

    /// Bootstrap orchestration system from YAML configuration file path
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use tasker_core::orchestration::{OrchestrationSystem, config::ConfigurationManager};
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     // Bootstrap from component-based configuration
    ///     let config_manager = ConfigurationManager::new();
    ///     let orchestration_system = OrchestrationSystem::from_config(
    ///         config_manager,
    ///     ).await?;
    ///
    ///     // Start orchestration
    ///     orchestration_system.start().await?;
    ///     Ok(())
    /// }
    /// ```
    pub async fn from_config_file<P: AsRef<std::path::Path> + std::fmt::Debug>(
        config_path: P,
    ) -> Result<Self> {
        let config_manager =
            crate::orchestration::config::ConfigurationManager::load_from_file(config_path)
                .await
                .map_err(|e| {
                    TaskerError::ConfigurationError(format!("Failed to load config: {e}"))
                })?;

        Self::from_config(config_manager).await
    }

    /// Start the complete orchestration system with OrchestrationLoopCoordinator
    ///
    /// UNIFIED ARCHITECTURE: This method now delegates to OrchestrationLoopCoordinator
    /// instead of directly spawning processors. This provides dynamic scaling, health
    /// monitoring, and resource management.
    #[instrument(skip(self))]
    pub async fn start(&self) -> Result<ContinuousOrchestrationSummary> {
        info!(
            orchestrator_id = %self.config.orchestrator_id,
            "🚀 Starting orchestration system with unified OrchestrationLoopCoordinator architecture"
        );

        // Initialize all required queues
        self.initialize_queues().await?;

        // Create configuration manager for coordinator
        let config_manager = crate::config::ConfigManager::load().map_err(|e| {
            TaskerError::ConfigurationError(format!("Failed to load config for coordinator: {e}"))
        })?;

        // Create OrchestrationCore from this system's components
        let orchestration_core = Arc::new(crate::orchestration::OrchestrationCore::new().await?);

        // Create OrchestrationLoopCoordinator for unified architecture
        // Note: config_manager is already Arc<ConfigManager> from load()
        let coordinator = Arc::new(
            crate::orchestration::coordinator::OrchestrationLoopCoordinator::new(
                config_manager,
                orchestration_core,
            )
            .await?,
        );

        info!(
            "✅ ORCHESTRATION_SYSTEM: Using OrchestrationLoopCoordinator for unified architecture"
        );

        // Start coordinator and wait for completion
        coordinator.start().await?;

        // For compatibility with the existing interface, we need to return a summary
        // Since OrchestrationLoopCoordinator doesn't return a summary directly,
        // we'll create a synthetic one based on the operation
        info!("✅ ORCHESTRATION_SYSTEM: Coordinator startup completed successfully");

        // Return a summary indicating successful coordinator startup
        Ok(ContinuousOrchestrationSummary {
            orchestrator_id: self.config.orchestrator_id.clone(),
            started_at: chrono::Utc::now(),
            ended_at: None,  // Still running
            total_cycles: 0, // Coordinator doesn't track cycles the same way
            failed_cycles: 0,
            total_tasks_processed: 0, // This would need coordinator metrics integration
            total_tasks_failed: 0,
            total_steps_enqueued: 0, // This would need coordinator metrics integration
            total_steps_failed: 0,
            aggregate_priority_distribution:
                crate::orchestration::orchestration_loop::PriorityDistribution::default(),
            aggregate_performance_metrics:
                crate::orchestration::orchestration_loop::AggregatePerformanceMetrics::default(),
            top_namespaces: vec![], // Would need coordinator metrics integration
            total_warnings: 0,
            recent_warnings: vec![],
        })
    }

    /// Run a single orchestration cycle (for testing or controlled execution)
    #[instrument(skip(self))]
    pub async fn run_single_cycle(&self) -> Result<OrchestrationCycleResult> {
        info!(
            orchestrator_id = %self.config.orchestrator_id,
            "Running single orchestration cycle"
        );

        self.orchestration_loop.run_cycle().await
    }

    /// Initialize all required queues for OrchestrationLoop approach
    async fn initialize_queues(&self) -> Result<()> {
        info!("Initializing orchestration queues for OrchestrationLoop");

        // Create task requests queue
        if let Err(e) = self
            .pgmq_client
            .create_queue(&self.config.task_requests_queue_name)
            .await
        {
            debug!(
                queue = %self.config.task_requests_queue_name,
                error = %e,
                "Task requests queue may already exist"
            );
        }

        // Create step results queue
        let step_results_queue = &self
            .config
            .orchestration_loop_config
            .step_result_processor_config
            .step_results_queue_name;
        if let Err(e) = self.pgmq_client.create_queue(step_results_queue).await {
            debug!(
                queue = %step_results_queue,
                error = %e,
                "Step results queue may already exist"
            );
        }

        // Create namespace-specific step queues (used by OrchestrationLoop)
        for namespace in &self.config.active_namespaces {
            let step_queue_name = format!("{namespace}_queue");
            if let Err(e) = self.pgmq_client.create_queue(&step_queue_name).await {
                debug!(
                    queue = %step_queue_name,
                    error = %e,
                    "Step queue may already exist"
                );
            }
        }

        info!(
            task_requests_queue = %self.config.task_requests_queue_name,
            step_results_queue = %step_results_queue,
            namespace_step_queues = self.config.active_namespaces.len(),
            "All orchestration queues initialized"
        );

        Ok(())
    }

    /// Get orchestration statistics for OrchestrationLoop approach
    pub async fn get_statistics(&self) -> Result<OrchestrationStats> {
        // Get task requests queue statistics
        let task_requests_queue_size = match self
            .pgmq_client
            .queue_metrics(&self.config.task_requests_queue_name)
            .await
        {
            Ok(metrics) => metrics.message_count,
            Err(e) => {
                warn!(
                    queue = %self.config.task_requests_queue_name,
                    error = %e,
                    "Failed to get task requests queue metrics"
                );
                -1 // Indicate unavailable
            }
        };

        // Get namespace-specific step queue statistics
        let mut namespace_queue_sizes = Vec::new();
        for namespace in &self.config.active_namespaces {
            let queue_name = format!("{namespace}_queue");
            let queue_size = match self.pgmq_client.queue_metrics(&queue_name).await {
                Ok(metrics) => metrics.message_count,
                Err(e) => {
                    warn!(
                        queue = %queue_name,
                        error = %e,
                        "Failed to get namespace queue metrics"
                    );
                    -1 // Indicate unavailable
                }
            };
            namespace_queue_sizes.push((queue_name, queue_size));
        }

        Ok(OrchestrationStats {
            task_requests_queue_size,
            namespace_queue_sizes,
            active_namespaces: self.config.active_namespaces.clone(),
            orchestrator_id: self.config.orchestrator_id.clone(),
            orchestration_loop_config: self.config.orchestration_loop_config.clone(),
        })
    }
}

/// Statistics for the orchestration system using OrchestrationLoop
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrchestrationStats {
    /// Task requests queue size (pending task creation requests)
    pub task_requests_queue_size: i64,
    /// Namespace-specific step queue sizes: (queue_name, queue_length)
    pub namespace_queue_sizes: Vec<(String, i64)>,
    /// Active namespaces in the system
    pub active_namespaces: Vec<String>,
    /// Orchestrator instance ID
    pub orchestrator_id: String,
    /// Current orchestration loop configuration
    pub orchestration_loop_config: OrchestrationLoopConfig,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_config_defaults() {
        let config = OrchestrationSystemConfig::default();

        assert_eq!(config.task_requests_queue_name, "task_requests_queue");
        assert!(config.orchestrator_id.starts_with("orchestrator-"));
        assert_eq!(config.task_request_polling_interval_ms, 250);
        assert_eq!(config.task_request_visibility_timeout_seconds, 300);
        assert_eq!(config.task_request_batch_size, 10);
        assert_eq!(config.max_concurrent_orchestrators, 3);
        assert!(!config.enable_performance_logging);

        // Verify default namespaces
        let expected_namespaces = vec![
            "fulfillment",
            "inventory",
            "notifications",
            "payments",
            "analytics",
        ];
        assert_eq!(config.active_namespaces.len(), expected_namespaces.len());
        for namespace in expected_namespaces {
            assert!(config.active_namespaces.contains(&namespace.to_string()));
        }

        // Verify orchestration loop config defaults
        assert_eq!(config.orchestration_loop_config.tasks_per_cycle, 5);
        assert_eq!(config.orchestration_loop_config.cycle_interval.as_secs(), 1);
    }

    #[test]
    fn test_config_customization() {
        let config = OrchestrationSystemConfig {
            task_requests_queue_name: "custom_task_requests".to_string(),
            orchestrator_id: "custom-orchestrator-123".to_string(),
            orchestration_loop_config: OrchestrationLoopConfig {
                tasks_per_cycle: 20,
                cycle_interval: Duration::from_secs(5),
                ..OrchestrationLoopConfig::default()
            },
            task_request_polling_interval_ms: 500, // 500ms for test
            task_request_visibility_timeout_seconds: 600,
            task_request_batch_size: 25,
            active_namespaces: vec!["custom".to_string(), "test".to_string()],
            max_concurrent_orchestrators: 5,
            enable_performance_logging: true,
        };

        assert_eq!(config.task_requests_queue_name, "custom_task_requests");
        assert_eq!(config.orchestrator_id, "custom-orchestrator-123");
        assert_eq!(config.task_request_polling_interval_ms, 500);
        assert_eq!(config.task_request_visibility_timeout_seconds, 600);
        assert_eq!(config.task_request_batch_size, 25);
        assert_eq!(config.max_concurrent_orchestrators, 5);
        assert!(config.enable_performance_logging);
        assert_eq!(config.active_namespaces, vec!["custom", "test"]);
        assert_eq!(config.orchestration_loop_config.tasks_per_cycle, 20);
        assert_eq!(config.orchestration_loop_config.cycle_interval.as_secs(), 5);
    }
}
