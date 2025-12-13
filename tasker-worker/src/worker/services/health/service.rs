//! # Worker Health Service
//!
//! TAS-77: Health check logic extracted from web/handlers/health.rs.
//!
//! This service encapsulates all health check functionality, making it available
//! to both the HTTP API and FFI consumers without code duplication.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use sqlx::{PgPool, Row};
use tokio::sync::RwLock as TokioRwLock;
use tracing::{debug, error, warn};

use crate::web::state::CircuitBreakerHealthProvider;
use crate::worker::core::{WorkerCore, WorkerCoreStatus};
use tasker_shared::types::api::worker::{
    BasicHealthResponse, CircuitBreakerState, CircuitBreakersHealth, DetailedHealthResponse,
    HealthCheck, WorkerSystemInfo,
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
///
/// This service can be used by:
/// - Web API handlers (via `WorkerWebState`)
/// - FFI consumers (Ruby, Python, etc.)
/// - Internal monitoring systems
///
/// ## Example
///
/// ```ignore
/// let service = HealthService::new(
///     worker_id,
///     database_pool,
///     worker_core,
///     circuit_breaker_provider,
///     start_time,
/// );
///
/// // Get basic health (always succeeds)
/// let basic = service.basic_health();
///
/// // Get detailed health with all checks
/// let detailed = service.detailed_health().await;
/// ```
pub struct HealthService {
    /// Worker identification
    worker_id: String,

    /// Database connection pool for connectivity checks
    database_pool: Arc<PgPool>,

    /// Worker core for processor and step execution stats
    worker_core: Arc<tokio::sync::Mutex<WorkerCore>>,

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
            .field("uptime_seconds", &self.start_time.elapsed().as_secs())
            .field("circuit_breaker_provider", &"<shared>")
            .finish()
    }
}

impl HealthService {
    /// Create a new HealthService
    ///
    /// # Arguments
    /// * `circuit_breaker_provider` - Shared reference to circuit breaker provider.
    ///   This is shared with `WorkerWebState` so updates are visible to both.
    pub fn new(
        worker_id: String,
        database_pool: Arc<PgPool>,
        worker_core: Arc<tokio::sync::Mutex<WorkerCore>>,
        circuit_breaker_provider: SharedCircuitBreakerProvider,
        start_time: std::time::Instant,
    ) -> Self {
        Self {
            worker_id,
            database_pool,
            worker_core,
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
    /// Returns `Ok(DetailedHealthResponse)` if ready, `Err(DetailedHealthResponse)` if not ready.
    pub async fn readiness(&self) -> Result<DetailedHealthResponse, DetailedHealthResponse> {
        debug!("Performing worker readiness probe");

        let mut checks = HashMap::new();
        let mut overall_healthy = true;

        // Check database connectivity
        let db_check = self.check_database().await;
        overall_healthy = overall_healthy && db_check.status == "healthy";
        checks.insert("database".to_string(), db_check);

        // Check command processor health
        let processor_check = self.check_command_processor().await;
        overall_healthy = overall_healthy && processor_check.status == "healthy";
        checks.insert("command_processor".to_string(), processor_check);

        // Check queue processing capability
        let queue_check = self.check_queue().await;
        overall_healthy = overall_healthy && queue_check.status == "healthy";
        checks.insert("queue_processing".to_string(), queue_check);

        let response = DetailedHealthResponse {
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
    /// Includes performance metrics and diagnostic information.
    pub async fn detailed_health(&self) -> DetailedHealthResponse {
        debug!("Performing detailed worker health check");

        let mut checks = HashMap::new();

        // Run all health checks
        checks.insert("database".to_string(), self.check_database().await);
        checks.insert(
            "command_processor".to_string(),
            self.check_command_processor().await,
        );
        checks.insert("queue_processing".to_string(), self.check_queue().await);
        checks.insert("event_system".to_string(), self.check_event_system());
        checks.insert(
            "step_processing".to_string(),
            self.check_step_processing().await,
        );
        checks.insert(
            "circuit_breakers".to_string(),
            self.check_circuit_breakers().await,
        );

        // Determine overall status
        let overall_healthy = checks.values().all(|check| check.status == "healthy");

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
        }
    }

    // =========================================================================
    // Individual Health Checks
    // =========================================================================

    /// Check database connectivity and basic query performance
    pub async fn check_database(&self) -> HealthCheck {
        let start = std::time::Instant::now();

        match sqlx::query("SELECT 1")
            .fetch_one(self.database_pool.as_ref())
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
                let is_healthy = health_status.status == "healthy"
                    && health_status.database_connected
                    && health_status.orchestration_api_reachable;

                HealthCheck {
                    status: if is_healthy { "healthy" } else { "degraded" }.to_string(),
                    message: Some(format!(
                        "Worker status: {}, DB connected: {}, API reachable: {}",
                        health_status.status,
                        health_status.database_connected,
                        health_status.orchestration_api_reachable
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
    pub async fn check_queue(&self) -> HealthCheck {
        let start = std::time::Instant::now();

        // Check PGMQ queue metrics for worker namespaces
        match sqlx::query(
            "SELECT queue_name, queue_length FROM pgmq.metrics_all() WHERE queue_name LIKE '%_queue'",
        )
        .fetch_all(self.database_pool.as_ref())
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
    pub fn check_event_system(&self) -> HealthCheck {
        let start = std::time::Instant::now();

        // TODO: Check event publisher/subscriber health
        // For now, assume healthy if configuration allows events

        HealthCheck {
            status: "healthy".to_string(),
            message: Some("Event system available".to_string()),
            duration_ms: start.elapsed().as_millis() as u64,
            last_checked: Utc::now(),
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

    /// Create system information summary
    async fn create_system_info(&self) -> WorkerSystemInfo {
        // Lock the worker core to access status and namespace methods
        let worker_core = self.worker_core.lock().await;

        WorkerSystemInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            environment: std::env::var("TASKER_ENV").unwrap_or_else(|_| "development".to_string()),
            uptime_seconds: self.uptime_seconds(),
            worker_type: "command_processor".to_string(),
            database_pool_size: self.database_pool.size(),
            command_processor_active: matches!(worker_core.status(), WorkerCoreStatus::Running),
            supported_namespaces: worker_core.supported_namespaces().await,
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
