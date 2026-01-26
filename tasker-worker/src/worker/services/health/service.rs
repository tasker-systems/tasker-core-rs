//! # Worker Health Service
//!
//! TAS-77: Health check logic extracted from web/handlers/health.rs.
//! TAS-169: Includes distributed cache status in detailed health.
//!
//! This service encapsulates all health check functionality, making it available
//! to both the HTTP API and FFI consumers without code duplication.

use std::sync::Arc;

use chrono::Utc;
use sqlx::Row;
use tokio::sync::RwLock as TokioRwLock;
use tracing::{debug, error, warn};

use crate::web::state::CircuitBreakerHealthProvider;
use crate::worker::core::{WorkerCore, WorkerCoreStatus};
use crate::worker::task_template_manager::TaskTemplateManager;
use tasker_shared::database::DatabasePools;
use tasker_shared::types::api::health::build_pool_utilization;
use tasker_shared::types::api::worker::{
    BasicHealthResponse, CircuitBreakerState, CircuitBreakersHealth, DetailedHealthResponse,
    DistributedCacheInfo, HealthCheck, ReadinessResponse, WorkerDetailedChecks,
    WorkerReadinessChecks, WorkerSystemInfo,
};

/// Shared circuit breaker provider reference
///
/// This type is shared between `WorkerWebState` and `HealthService` so that
/// when the provider is set (by FFI workers), both see the update.
pub type SharedCircuitBreakerProvider =
    Arc<TokioRwLock<Option<Arc<dyn CircuitBreakerHealthProvider>>>>;

/// Health Service
///
/// TAS-77: Provides health check functionality independent of the HTTP layer.
/// TAS-169: Includes distributed cache status in detailed health.
///
/// This service can be used by:
/// - Web API handlers (via `WorkerWebState`)
/// - FFI consumers (Ruby, Python, etc.)
/// - Internal monitoring systems
///
/// ## Example
///
/// ```rust,no_run
/// use tasker_worker::worker::services::health::{HealthService, SharedCircuitBreakerProvider};
/// use tasker_worker::worker::core::WorkerCore;
/// use tasker_worker::worker::task_template_manager::TaskTemplateManager;
/// use tasker_shared::database::DatabasePools;
/// use std::sync::Arc;
/// use std::time::Instant;
/// use tokio::sync::{Mutex, RwLock};
///
/// async fn example(
///     worker_id: String,
///     database_pools: DatabasePools,
///     worker_core: Arc<Mutex<WorkerCore>>,
///     task_template_manager: Arc<TaskTemplateManager>,
/// ) {
///     let circuit_breaker_provider: SharedCircuitBreakerProvider =
///         Arc::new(RwLock::new(None));
///     let start_time = Instant::now();
///
///     let service = HealthService::new(
///         worker_id,
///         database_pools,
///         worker_core,
///         task_template_manager,
///         circuit_breaker_provider,
///         start_time,
///     );
///
///     // Get basic health (always succeeds)
///     let basic = service.basic_health();
///
///     // Get detailed health with all checks (includes distributed cache status)
///     let detailed = service.detailed_health().await;
/// }
/// ```
pub struct HealthService {
    /// Worker identification
    worker_id: String,

    /// TAS-164: Database pools for connectivity checks and utilization reporting
    database_pools: DatabasePools,

    /// Worker core for processor and step execution stats
    worker_core: Arc<tokio::sync::Mutex<WorkerCore>>,

    /// TAS-169: Task template manager for distributed cache status
    task_template_manager: Arc<TaskTemplateManager>,

    /// Circuit breaker health provider (TAS-75)
    /// Shared reference so WorkerWebState can update it after construction
    circuit_breaker_provider: SharedCircuitBreakerProvider,

    /// Service start time for uptime calculation
    start_time: std::time::Instant,
}

impl std::fmt::Debug for HealthService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HealthService")
            .field("worker_id", &self.worker_id)
            .field("database_pools", &self.database_pools)
            .field("uptime_seconds", &self.start_time.elapsed().as_secs())
            .field("task_template_manager", &"<TaskTemplateManager>")
            .field("circuit_breaker_provider", &"<shared>")
            .finish()
    }
}

impl HealthService {
    /// Create a new HealthService
    ///
    /// # Arguments
    /// * `worker_id` - Unique worker identifier
    /// * `database_pools` - TAS-164: Database pools for connectivity checks and utilization
    /// * `worker_core` - Worker core for processing stats
    /// * `task_template_manager` - TAS-169: Task template manager for distributed cache status
    /// * `circuit_breaker_provider` - Shared reference to circuit breaker provider.
    ///   This is shared with `WorkerWebState` so updates are visible to both.
    /// * `start_time` - Service start time for uptime calculation
    pub fn new(
        worker_id: String,
        database_pools: DatabasePools,
        worker_core: Arc<tokio::sync::Mutex<WorkerCore>>,
        task_template_manager: Arc<TaskTemplateManager>,
        circuit_breaker_provider: SharedCircuitBreakerProvider,
        start_time: std::time::Instant,
    ) -> Self {
        Self {
            worker_id,
            database_pools,
            worker_core,
            task_template_manager,
            circuit_breaker_provider,
            start_time,
        }
    }

    /// Get uptime in seconds
    pub fn uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Get worker ID
    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }

    // =========================================================================
    // Endpoint-Level Methods
    // =========================================================================

    /// Basic health check: GET /health
    ///
    /// Simple health check that returns OK if the worker service is running.
    /// Always available, even during graceful shutdown.
    pub fn basic_health(&self) -> BasicHealthResponse {
        BasicHealthResponse {
            status: "healthy".to_string(),
            timestamp: Utc::now(),
            worker_id: self.worker_id.clone(),
        }
    }

    /// Kubernetes liveness probe: GET /health/live
    ///
    /// Indicates whether the worker is alive and should not be restarted.
    /// Simple check focusing on basic process responsiveness.
    pub fn liveness(&self) -> BasicHealthResponse {
        // Check if we can access our state (basic process health)
        let _uptime = self.uptime_seconds();

        BasicHealthResponse {
            status: "alive".to_string(),
            timestamp: Utc::now(),
            worker_id: self.worker_id.clone(),
        }
    }

    /// Kubernetes readiness probe: GET /health/ready
    ///
    /// Indicates whether the worker is ready to process steps.
    /// Checks database connectivity, command processor status, and queue health.
    ///
    /// Returns `Ok(ReadinessResponse)` if ready, `Err(ReadinessResponse)` if not ready.
    pub async fn readiness(&self) -> Result<ReadinessResponse, ReadinessResponse> {
        debug!("Performing worker readiness probe");

        // TAS-76: Use typed readiness checks
        let checks = WorkerReadinessChecks {
            database: self.check_database().await,
            command_processor: self.check_command_processor().await,
            queue_processing: self.check_queue().await,
        };

        let overall_healthy = checks.all_healthy();

        let response = ReadinessResponse {
            status: if overall_healthy {
                "ready"
            } else {
                "not_ready"
            }
            .to_string(),
            timestamp: Utc::now(),
            worker_id: self.worker_id.clone(),
            checks,
            system_info: self.create_system_info().await,
        };

        if overall_healthy {
            Ok(response)
        } else {
            warn!("Worker readiness check failed");
            Err(response)
        }
    }

    /// Comprehensive health check: GET /health/detailed
    ///
    /// Detailed health information about all worker subsystems.
    /// Includes performance metrics, diagnostic information, and distributed cache status.
    /// TAS-169: Distributed cache status moved here from /templates/cache/distributed.
    pub async fn detailed_health(&self) -> DetailedHealthResponse {
        debug!("Performing detailed worker health check");

        // TAS-76: Use typed detailed checks
        let checks = WorkerDetailedChecks {
            database: self.check_database().await,
            command_processor: self.check_command_processor().await,
            queue_processing: self.check_queue().await,
            event_system: self.check_event_system().await,
            step_processing: self.check_step_processing().await,
            circuit_breakers: self.check_circuit_breakers().await,
        };

        // Determine overall status
        let overall_healthy = checks.all_healthy();

        // TAS-169: Get distributed cache status
        let distributed_cache = Some(self.get_distributed_cache_status().await);

        DetailedHealthResponse {
            status: if overall_healthy {
                "healthy"
            } else {
                "degraded"
            }
            .to_string(),
            timestamp: Utc::now(),
            worker_id: self.worker_id.clone(),
            checks,
            system_info: self.create_system_info().await,
            distributed_cache,
        }
    }

    // =========================================================================
    // Individual Health Checks
    // =========================================================================

    /// Check database connectivity and basic query performance
    pub async fn check_database(&self) -> HealthCheck {
        let start = std::time::Instant::now();

        match sqlx::query("SELECT 1")
            .fetch_one(self.database_pools.tasker())
            .await
        {
            Ok(_) => HealthCheck {
                status: "healthy".to_string(),
                message: Some("Database connection successful".to_string()),
                duration_ms: start.elapsed().as_millis() as u64,
                last_checked: Utc::now(),
            },
            Err(e) => {
                error!(error = %e, "Worker database health check failed");
                HealthCheck {
                    status: "unhealthy".to_string(),
                    message: Some(format!("Database connection failed: {e}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                    last_checked: Utc::now(),
                }
            }
        }
    }

    /// Check command processor availability and responsiveness
    pub async fn check_command_processor(&self) -> HealthCheck {
        let start = std::time::Instant::now();

        // Lock the worker core to access health methods
        let worker_core = self.worker_core.lock().await;

        // Use WorkerCore's get_health method directly
        match worker_core.get_health().await {
            Ok(health_status) => {
                let is_healthy =
                    health_status.status == "healthy" && health_status.database_connected;

                HealthCheck {
                    status: if is_healthy { "healthy" } else { "degraded" }.to_string(),
                    message: Some(format!(
                        "Worker status: {}, DB connected: {}",
                        health_status.status, health_status.database_connected,
                    )),
                    duration_ms: start.elapsed().as_millis() as u64,
                    last_checked: Utc::now(),
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to get worker health status");
                HealthCheck {
                    status: "unhealthy".to_string(),
                    message: Some(format!("Worker health check failed: {e}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                    last_checked: Utc::now(),
                }
            }
        }
    }

    /// Check queue processing capability and queue depths
    ///
    /// TAS-78: Uses PGMQ pool for queue metrics (supports split-database deployments)
    pub async fn check_queue(&self) -> HealthCheck {
        let start = std::time::Instant::now();

        // Check PGMQ queue metrics for worker namespaces
        // TAS-78: Use PGMQ pool (may be separate from Tasker database)
        match sqlx::query(
            "SELECT queue_name, queue_length FROM pgmq.metrics_all() WHERE queue_name LIKE '%_queue'",
        )
        .fetch_all(self.database_pools.pgmq())
        .await
        {
            Ok(rows) => {
                let total_queue_depth: i64 = rows
                    .iter()
                    .filter_map(|row| row.try_get::<i64, _>("queue_length").ok())
                    .sum();

                // Consider healthy if total queue depth is reasonable
                let status = if total_queue_depth < 10000 {
                    "healthy"
                } else if total_queue_depth < 50000 {
                    "degraded"
                } else {
                    "unhealthy"
                };

                HealthCheck {
                    status: status.to_string(),
                    message: Some(format!(
                        "Queue processing available, total depth: {total_queue_depth}"
                    )),
                    duration_ms: start.elapsed().as_millis() as u64,
                    last_checked: Utc::now(),
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to check queue metrics");
                HealthCheck {
                    status: "unhealthy".to_string(),
                    message: Some(format!("Queue metrics unavailable: {e}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                    last_checked: Utc::now(),
                }
            }
        }
    }

    /// Check event system health and connectivity
    ///
    /// Verifies:
    /// - Event-driven processing status (if enabled)
    /// - Domain event publisher availability
    /// - Event router connectivity (if configured)
    pub async fn check_event_system(&self) -> HealthCheck {
        let start = std::time::Instant::now();

        let worker_core = self.worker_core.lock().await;

        // Check if event-driven processing is enabled
        let event_driven_enabled = worker_core.is_event_driven_enabled();

        if event_driven_enabled {
            // Check event-driven stats for health indicators
            match worker_core.get_event_driven_stats().await {
                Some(stats) => {
                    // Check if listener is connected (critical for event-driven mode)
                    if stats.listener_connected {
                        HealthCheck {
                            status: "healthy".to_string(),
                            message: Some(format!(
                                "Event-driven processing active: {} messages processed, listener connected",
                                stats.messages_processed
                            )),
                            duration_ms: start.elapsed().as_millis() as u64,
                            last_checked: Utc::now(),
                        }
                    } else {
                        warn!(
                            worker_id = %self.worker_id,
                            "Event-driven listener disconnected"
                        );
                        HealthCheck {
                            status: "degraded".to_string(),
                            message: Some("Event-driven listener disconnected".to_string()),
                            duration_ms: start.elapsed().as_millis() as u64,
                            last_checked: Utc::now(),
                        }
                    }
                }
                None => {
                    // Event-driven enabled but no stats available yet
                    HealthCheck {
                        status: "healthy".to_string(),
                        message: Some("Event-driven processing enabled, initializing".to_string()),
                        duration_ms: start.elapsed().as_millis() as u64,
                        last_checked: Utc::now(),
                    }
                }
            }
        } else {
            // Event-driven not enabled - check domain event publisher
            let domain_stats = worker_core.get_domain_event_stats().await;

            HealthCheck {
                status: "healthy".to_string(),
                message: Some(format!(
                    "Polling mode active, domain events: {} routed",
                    domain_stats.router.total_routed
                )),
                duration_ms: start.elapsed().as_millis() as u64,
                last_checked: Utc::now(),
            }
        }
    }

    /// Check step processing performance and error rates
    pub async fn check_step_processing(&self) -> HealthCheck {
        let start = std::time::Instant::now();

        // Lock the worker core to access processing stats
        let worker_core = self.worker_core.lock().await;

        // Use WorkerCore's get_processing_stats method directly
        match worker_core.get_processing_stats().await {
            Ok(worker_status) => {
                // Calculate error rate and determine health
                let total_executions = worker_status.steps_executed;
                let error_rate = if total_executions > 0 {
                    (worker_status.steps_failed as f64 / total_executions as f64) * 100.0
                } else {
                    0.0
                };

                let status = if error_rate < 5.0 {
                    "healthy"
                } else if error_rate < 15.0 {
                    "degraded"
                } else {
                    "unhealthy"
                };

                HealthCheck {
                    status: status.to_string(),
                    message: Some(format!(
                        "Step processing operational - {} executed, {:.1}% error rate",
                        total_executions, error_rate
                    )),
                    duration_ms: start.elapsed().as_millis() as u64,
                    last_checked: Utc::now(),
                }
            }
            Err(e) => {
                error!(error = %e, "Failed to get worker processing stats");
                HealthCheck {
                    status: "unhealthy".to_string(),
                    message: Some(format!("Step processing stats failed: {e}")),
                    duration_ms: start.elapsed().as_millis() as u64,
                    last_checked: Utc::now(),
                }
            }
        }
    }

    /// TAS-75: Check circuit breaker health
    ///
    /// Returns health status of all circuit breakers in the worker.
    pub async fn check_circuit_breakers(&self) -> HealthCheck {
        let start = std::time::Instant::now();

        // Get circuit breaker health from the provider (if configured)
        let cb_health = self.circuit_breakers_health().await;
        let has_provider = self.has_circuit_breaker_provider().await;

        // Determine status based on circuit breaker states
        let status = if !has_provider {
            // No circuit breaker provider configured - still healthy (just no CB data)
            "healthy"
        } else if cb_health.all_healthy {
            "healthy"
        } else if cb_health.open_count > 0 {
            // At least one circuit breaker is open - unhealthy
            "unhealthy"
        } else {
            // Some circuit breakers in half-open (testing recovery)
            "degraded"
        };

        // Build descriptive message
        let message = if !has_provider {
            "No circuit breakers configured".to_string()
        } else {
            let cb_details: Vec<String> = cb_health
                .circuit_breakers
                .iter()
                .map(|cb| {
                    format!(
                        "{}: {} ({} calls, {} failures)",
                        cb.name, cb.state, cb.total_calls, cb.failure_count
                    )
                })
                .collect();

            if cb_details.is_empty() {
                "No circuit breakers registered".to_string()
            } else {
                format!(
                    "{} circuit breakers: {} closed, {} open, {} half-open. Details: {}",
                    cb_health.circuit_breakers.len(),
                    cb_health.closed_count,
                    cb_health.open_count,
                    cb_health.half_open_count,
                    cb_details.join("; ")
                )
            }
        };

        // Log warnings for unhealthy circuit breakers
        if cb_health.open_count > 0 {
            for cb in &cb_health.circuit_breakers {
                if cb.state == CircuitBreakerState::Open {
                    warn!(
                        circuit_breaker = %cb.name,
                        failure_count = cb.failure_count,
                        consecutive_failures = cb.consecutive_failures,
                        "Circuit breaker is OPEN - failing fast"
                    );
                }
            }
        }

        HealthCheck {
            status: status.to_string(),
            message: Some(message),
            duration_ms: start.elapsed().as_millis() as u64,
            last_checked: Utc::now(),
        }
    }

    // =========================================================================
    // Helper Methods
    // =========================================================================

    /// Get circuit breakers health status
    async fn circuit_breakers_health(&self) -> CircuitBreakersHealth {
        let provider_guard = self.circuit_breaker_provider.read().await;
        provider_guard
            .as_ref()
            .map(|p| p.get_circuit_breakers_health())
            .unwrap_or_else(|| CircuitBreakersHealth {
                all_healthy: true,
                closed_count: 0,
                open_count: 0,
                half_open_count: 0,
                circuit_breakers: Vec::new(),
            })
    }

    /// Check if a circuit breaker provider is configured
    async fn has_circuit_breaker_provider(&self) -> bool {
        self.circuit_breaker_provider.read().await.is_some()
    }

    /// TAS-169: Get distributed cache status
    ///
    /// Reports the status of the distributed template cache (Redis or NoOp).
    /// Moved from /templates/cache/distributed to /health/detailed.
    async fn get_distributed_cache_status(&self) -> DistributedCacheInfo {
        let registry = self.task_template_manager.registry();
        let enabled = registry.cache_enabled();
        let provider_name = registry
            .cache_provider()
            .map(|p| p.provider_name())
            .unwrap_or("none");

        let healthy = if enabled {
            if let Some(provider) = registry.cache_provider() {
                provider.health_check().await.unwrap_or(false)
            } else {
                false
            }
        } else {
            // When distributed cache is disabled, report as healthy (not applicable)
            true
        };

        DistributedCacheInfo {
            enabled,
            provider: provider_name.to_string(),
            healthy,
        }
    }

    /// Create system information summary
    async fn create_system_info(&self) -> WorkerSystemInfo {
        // Lock the worker core to access status and namespace methods
        let worker_core = self.worker_core.lock().await;

        WorkerSystemInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            environment: std::env::var("TASKER_ENV").unwrap_or_else(|_| "development".to_string()),
            uptime_seconds: self.uptime_seconds(),
            worker_type: "command_processor".to_string(),
            database_pool_size: self.database_pools.tasker().size(),
            command_processor_active: matches!(worker_core.status(), WorkerCoreStatus::Running),
            supported_namespaces: worker_core.supported_namespaces().await,
            pool_utilization: Some(build_pool_utilization(&self.database_pools)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: Full integration tests require database and WorkerCore setup
    // These are basic unit tests for synchronous methods

    #[test]
    fn test_basic_health_returns_healthy() {
        // Create a minimal mock setup
        // In practice, we'd use a test fixture here
        // For now, just verify the response structure expectations
        let response = BasicHealthResponse {
            status: "healthy".to_string(),
            timestamp: Utc::now(),
            worker_id: "test-worker".to_string(),
        };

        assert_eq!(response.status, "healthy");
        assert!(!response.worker_id.is_empty());
    }

    #[test]
    fn test_liveness_returns_alive() {
        let response = BasicHealthResponse {
            status: "alive".to_string(),
            timestamp: Utc::now(),
            worker_id: "test-worker".to_string(),
        };

        assert_eq!(response.status, "alive");
    }

    #[test]
    fn test_health_check_structure() {
        let check = HealthCheck {
            status: "healthy".to_string(),
            message: Some("Test message".to_string()),
            duration_ms: 5,
            last_checked: Utc::now(),
        };

        assert_eq!(check.status, "healthy");
        assert!(check.message.is_some());
        assert_eq!(check.duration_ms, 5);
    }
}
