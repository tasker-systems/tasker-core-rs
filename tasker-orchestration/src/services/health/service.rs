//! # Orchestration Health Service
//!
//! TAS-76: Health check logic extracted from web/handlers/health.rs.
//!
//! This service encapsulates all health check functionality, making it available
//! to both the HTTP API (Axum) and future gRPC consumers (Tonic) without code duplication.

use std::sync::Arc;

use chrono::Utc;
use sqlx::PgPool;
use tokio::sync::RwLock;
use tracing::{debug, error};

use crate::health::QueueDepthTier;
use crate::orchestration::core::{OrchestrationCore, OrchestrationCoreStatus};
use crate::web::circuit_breaker::{CircuitState, WebDatabaseCircuitBreaker};
use crate::web::state::OrchestrationStatus;
use tasker_shared::metrics::channels::global_registry;
use tasker_shared::types::api::health::build_pool_utilization;
use tasker_shared::types::api::orchestration::{
    DetailedHealthChecks, DetailedHealthResponse, HealthCheck, HealthInfo, HealthResponse,
    ReadinessChecks, ReadinessResponse,
};

/// Health Service for orchestration.
///
/// TAS-76: Provides health check functionality independent of the HTTP layer.
///
/// This service can be used by:
/// - Web API handlers (via Axum `State`)
/// - Future gRPC endpoints (via Tonic)
/// - Internal monitoring systems
#[derive(Clone)]
pub struct HealthService {
    /// Web API database pool
    web_db_pool: PgPool,

    /// Orchestration database pool
    orchestration_db_pool: PgPool,

    /// Web database circuit breaker
    web_db_circuit_breaker: WebDatabaseCircuitBreaker,

    /// Orchestration core for system status
    orchestration_core: Arc<OrchestrationCore>,

    /// Cached orchestration status
    orchestration_status: Arc<RwLock<OrchestrationStatus>>,
}

impl std::fmt::Debug for HealthService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HealthService")
            .field("web_db_pool", &"PgPool")
            .field("orchestration_db_pool", &"PgPool")
            .finish()
    }
}

impl HealthService {
    /// Create a new HealthService.
    pub fn new(
        web_db_pool: PgPool,
        orchestration_db_pool: PgPool,
        web_db_circuit_breaker: WebDatabaseCircuitBreaker,
        orchestration_core: Arc<OrchestrationCore>,
        orchestration_status: Arc<RwLock<OrchestrationStatus>>,
    ) -> Self {
        Self {
            web_db_pool,
            orchestration_db_pool,
            web_db_circuit_breaker,
            orchestration_core,
            orchestration_status,
        }
    }

    // =========================================================================
    // Endpoint-Level Methods
    // =========================================================================

    /// Basic health check: GET /health
    ///
    /// Simple health check that returns OK if the service is running.
    /// Always available, even during graceful shutdown.
    pub fn basic_health(&self) -> HealthResponse {
        HealthResponse {
            status: "healthy".to_string(),
            timestamp: Utc::now().to_rfc3339(),
        }
    }

    /// Kubernetes liveness probe: GET /live
    ///
    /// Indicates whether the service is alive and should not be restarted.
    /// Simple check focusing on basic process responsiveness.
    pub async fn liveness(&self) -> HealthResponse {
        // Check if we can access orchestration state (basic process health)
        let _operational_state = self
            .orchestration_status
            .read()
            .await
            .operational_state
            .clone();

        HealthResponse {
            status: "alive".to_string(),
            timestamp: Utc::now().to_rfc3339(),
        }
    }

    /// Kubernetes readiness probe: GET /ready
    ///
    /// Indicates whether the service is ready to accept traffic.
    /// Checks database connectivity and circuit breaker status.
    ///
    /// Returns `Ok(ReadinessResponse)` if ready, `Err(ReadinessResponse)` if not ready.
    pub async fn readiness(&self) -> Result<ReadinessResponse, ReadinessResponse> {
        debug!("Performing readiness probe");

        let checks = ReadinessChecks {
            web_database: self.check_database(&self.web_db_pool, "web_database").await,
            orchestration_database: self
                .check_database(&self.orchestration_db_pool, "orchestration_database")
                .await,
            circuit_breaker: self.check_circuit_breaker().await,
            orchestration_system: self.check_orchestration().await,
            command_processor: self.check_command_processor().await,
        };

        let overall_healthy = checks.all_healthy();

        let response = ReadinessResponse {
            status: if overall_healthy {
                "ready"
            } else {
                "not_ready"
            }
            .to_string(),
            timestamp: Utc::now().to_rfc3339(),
            checks,
            info: self.create_health_info().await,
        };

        if overall_healthy {
            Ok(response)
        } else {
            Err(response)
        }
    }

    /// Comprehensive health check: GET /health/detailed
    ///
    /// Detailed health information about all orchestration subsystems.
    pub async fn detailed_health(&self) -> DetailedHealthResponse {
        debug!("Performing detailed health check");

        let checks = DetailedHealthChecks {
            web_database: self.check_database(&self.web_db_pool, "web_database").await,
            orchestration_database: self
                .check_database(&self.orchestration_db_pool, "orchestration_database")
                .await,
            circuit_breaker: self.check_circuit_breaker().await,
            orchestration_system: self.check_orchestration().await,
            command_processor: self.check_command_processor().await,
            pool_utilization: self.check_pool_utilization(),
            queue_depth: self.check_queue_depth().await,
            channel_saturation: self.check_channel_saturation(),
        };

        let overall_healthy = checks.all_healthy();

        DetailedHealthResponse {
            status: if overall_healthy {
                "healthy"
            } else {
                "degraded"
            }
            .to_string(),
            timestamp: Utc::now().to_rfc3339(),
            checks,
            info: self.create_health_info().await,
        }
    }

    /// Get Prometheus metrics.
    ///
    /// Returns metrics in Prometheus format for monitoring and alerting.
    pub async fn prometheus_metrics(&self) -> String {
        let mut metrics = Vec::new();

        // TAS-65 Phase 1: Get OpenTelemetry metrics in Prometheus format
        let exporter = tasker_shared::metrics::prometheus_exporter();
        let mut output = Vec::new();
        if let Err(e) = exporter.export(&mut output) {
            error!("Failed to export Prometheus metrics: {}", e);
        } else {
            let otel_metrics = String::from_utf8_lossy(&output).to_string();
            metrics.push(otel_metrics);
        }

        // Custom orchestration-specific metrics (not managed by OpenTelemetry)
        let mut custom_metrics = Vec::new();

        // Basic service information
        custom_metrics.push(format!(
            "# HELP tasker_orchestration_info Orchestration service information\n# TYPE tasker_orchestration_info gauge\ntasker_orchestration_info{{version=\"{}\"}} 1",
            env!("CARGO_PKG_VERSION")
        ));

        // Orchestration system status
        let status = self.orchestration_status.read().await;
        custom_metrics.push(format!(
            "# HELP tasker_orchestration_running Orchestration system running status\n# TYPE tasker_orchestration_running gauge\ntasker_orchestration_running {{}} {}",
            if status.running { 1 } else { 0 }
        ));

        // Database pool metrics (legacy)
        custom_metrics.push(format!(
            "# HELP tasker_orchestration_db_pool_size Database connection pool size\n# TYPE tasker_orchestration_db_pool_size gauge\ntasker_orchestration_db_pool_size {{}} {}",
            self.orchestration_db_pool.size()
        ));

        // TAS-164: Detailed pool utilization gauges
        let pools = self.orchestration_core.context.database_pools();
        let utilization = pools.utilization();
        custom_metrics.push(format!(
            "# HELP tasker_db_pool_connections Current database connection pool connections\n# TYPE tasker_db_pool_connections gauge\ntasker_db_pool_connections{{pool=\"tasker\",state=\"active\"}} {}\ntasker_db_pool_connections{{pool=\"tasker\",state=\"idle\"}} {}\ntasker_db_pool_connections{{pool=\"pgmq\",state=\"active\"}} {}\ntasker_db_pool_connections{{pool=\"pgmq\",state=\"idle\"}} {}",
            utilization.tasker_size.saturating_sub(utilization.tasker_idle),
            utilization.tasker_idle,
            utilization.pgmq_size.saturating_sub(utilization.pgmq_idle),
            utilization.pgmq_idle,
        ));

        // Circuit breaker metrics
        let cb_state = self.web_db_circuit_breaker.current_state();
        let cb_state_value = match cb_state {
            CircuitState::Closed => 0,
            CircuitState::HalfOpen => 1,
            CircuitState::Open => 2,
        };
        custom_metrics.push(format!(
            "# HELP tasker_orchestration_circuit_breaker_state Circuit breaker state (0=closed, 1=half-open, 2=open)\n# TYPE tasker_orchestration_circuit_breaker_state gauge\ntasker_orchestration_circuit_breaker_state {{}} {}",
            cb_state_value
        ));

        // TAS-51: Add channel metrics
        let channel_health = global_registry()
            .get_all_health(|_channel_name| {
                // For Prometheus export, we don't need precise capacity
                // Health checks will be done via dedicated health endpoint
                Some(100) // Placeholder capacity for health reporting
            })
            .await;

        for (channel_name, component, health_status) in channel_health {
            let health_value = match health_status.as_str() {
                s if s.starts_with("healthy") => 0,
                s if s.starts_with("degraded") => 1,
                s if s.starts_with("critical") => 2,
                _ => 0,
            };

            custom_metrics.push(format!(
                "# HELP tasker_orchestration_channel_health Channel health status (0=healthy, 1=degraded, 2=critical)\n# TYPE tasker_orchestration_channel_health gauge\ntasker_orchestration_channel_health{{channel_name=\"{}\",component=\"{}\"}} {}",
                channel_name, component, health_value
            ));
        }

        // Append custom metrics after OpenTelemetry metrics
        metrics.push(custom_metrics.join("\n\n"));

        metrics.join("\n\n")
    }

    // =========================================================================
    // Individual Health Checks
    // =========================================================================

    /// Check database connectivity.
    pub async fn check_database(&self, pool: &PgPool, name: &str) -> HealthCheck {
        let start = std::time::Instant::now();

        match sqlx::query("SELECT 1").fetch_one(pool).await {
            Ok(_) => HealthCheck::healthy(
                format!("{name} connected"),
                start.elapsed().as_millis() as u64,
            ),
            Err(e) => {
                error!(database = name, error = %e, "Database health check failed");
                HealthCheck::unhealthy(
                    format!("Database connection failed: {e}"),
                    start.elapsed().as_millis() as u64,
                )
            }
        }
    }

    /// Check circuit breaker health.
    pub async fn check_circuit_breaker(&self) -> HealthCheck {
        let start = std::time::Instant::now();

        let is_healthy = !self.web_db_circuit_breaker.is_circuit_open();
        let circuit_state = self.web_db_circuit_breaker.current_state();

        if is_healthy {
            HealthCheck::healthy(
                format!("Circuit breaker state: {circuit_state:?}"),
                start.elapsed().as_millis() as u64,
            )
        } else {
            HealthCheck::degraded(
                format!("Circuit breaker state: {circuit_state:?}"),
                start.elapsed().as_millis() as u64,
            )
        }
    }

    /// Check orchestration system health.
    pub async fn check_orchestration(&self) -> HealthCheck {
        let start = std::time::Instant::now();
        // Read directly from OrchestrationCore's shared status for real-time state
        let core_status = self.orchestration_core.status().read().await.clone();

        let is_running = matches!(core_status, OrchestrationCoreStatus::Running);
        let operational_state = match &core_status {
            OrchestrationCoreStatus::Running => "Normal",
            OrchestrationCoreStatus::Starting => "Startup",
            OrchestrationCoreStatus::Stopping => "GracefulShutdown",
            OrchestrationCoreStatus::Stopped => "Stopped",
            OrchestrationCoreStatus::Error(e) => {
                return HealthCheck::unhealthy(
                    format!("Orchestration error: {e}"),
                    start.elapsed().as_millis() as u64,
                );
            }
            OrchestrationCoreStatus::Created => "Startup",
        };

        if is_running {
            HealthCheck::healthy(
                format!("Operational state: {operational_state}"),
                start.elapsed().as_millis() as u64,
            )
        } else {
            HealthCheck::unhealthy(
                format!("Operational state: {operational_state}"),
                start.elapsed().as_millis() as u64,
            )
        }
    }

    /// Check command processor health.
    pub async fn check_command_processor(&self) -> HealthCheck {
        let start = std::time::Instant::now();

        match tokio::time::timeout(
            std::time::Duration::from_millis(1000),
            self.orchestration_core.get_health(),
        )
        .await
        {
            Ok(Ok(system_health)) => {
                let message = format!(
                    "Command processor responsive - DB: {}, Queues: {}, Processors: {}",
                    system_health.database_connected,
                    system_health.message_queues_healthy,
                    system_health.active_processors
                );

                if system_health.database_connected && system_health.message_queues_healthy {
                    HealthCheck::healthy(message, start.elapsed().as_millis() as u64)
                } else {
                    HealthCheck::degraded(message, start.elapsed().as_millis() as u64)
                }
            }
            Ok(Err(e)) => HealthCheck::unhealthy(
                format!("Command processor error: {e}"),
                start.elapsed().as_millis() as u64,
            ),
            Err(_) => HealthCheck::unhealthy(
                "Command processor health check timeout".to_string(),
                start.elapsed().as_millis() as u64,
            ),
        }
    }

    /// TAS-164: Check pool utilization health.
    ///
    /// Reports healthy (<80%), degraded (80-95%), or unhealthy (>95%) based on
    /// the highest utilization across tasker and pgmq pools.
    pub fn check_pool_utilization(&self) -> HealthCheck {
        let start = std::time::Instant::now();

        let pools = self.orchestration_core.context.database_pools();
        let utilization = pools.utilization();

        let tasker_active = utilization
            .tasker_size
            .saturating_sub(utilization.tasker_idle);
        let pgmq_active = utilization.pgmq_size.saturating_sub(utilization.pgmq_idle);

        let tasker_pct = if utilization.tasker_max > 0 {
            f64::from(tasker_active) / f64::from(utilization.tasker_max) * 100.0
        } else {
            0.0
        };
        let pgmq_pct = if utilization.pgmq_max > 0 {
            f64::from(pgmq_active) / f64::from(utilization.pgmq_max) * 100.0
        } else {
            0.0
        };

        let max_pct = tasker_pct.max(pgmq_pct);

        let message = format!(
            "tasker={tasker_active}/{} ({tasker_pct:.1}%), pgmq={pgmq_active}/{} ({pgmq_pct:.1}%)",
            utilization.tasker_max, utilization.pgmq_max,
        );

        if max_pct > 95.0 {
            HealthCheck::unhealthy(
                format!("Pool utilization critical: {message}"),
                start.elapsed().as_millis() as u64,
            )
        } else if max_pct > 80.0 {
            HealthCheck::degraded(
                format!("Pool utilization elevated: {message}"),
                start.elapsed().as_millis() as u64,
            )
        } else {
            HealthCheck::healthy(
                format!("Pool utilization normal: {message}"),
                start.elapsed().as_millis() as u64,
            )
        }
    }

    /// TAS-75 Phase 5: Check queue depth health using cached status.
    ///
    /// Uses the BackpressureChecker's cached queue depth status for non-blocking health checks.
    pub async fn check_queue_depth(&self) -> HealthCheck {
        let start = std::time::Instant::now();

        // TAS-75 Phase 5: Use synchronous cached status (no database query)
        let queue_status = self
            .orchestration_core
            .backpressure_checker()
            .try_get_queue_depth_status();

        match queue_status.tier {
            QueueDepthTier::Unknown => HealthCheck::unknown(
                "Queue depth status not yet evaluated or check disabled",
                start.elapsed().as_millis() as u64,
            ),
            QueueDepthTier::Normal => HealthCheck::healthy(
                format!(
                    "All queues normal (max: {} messages)",
                    queue_status.max_depth
                ),
                start.elapsed().as_millis() as u64,
            ),
            QueueDepthTier::Warning => HealthCheck::degraded(
                format!(
                    "Queue '{}' at warning level ({} messages)",
                    queue_status.worst_queue, queue_status.max_depth
                ),
                start.elapsed().as_millis() as u64,
            ),
            QueueDepthTier::Critical => HealthCheck::unhealthy(
                format!(
                    "Queue '{}' at critical depth ({} messages)",
                    queue_status.worst_queue, queue_status.max_depth
                ),
                start.elapsed().as_millis() as u64,
            ),
            QueueDepthTier::Overflow => HealthCheck::unhealthy(
                format!(
                    "Queue '{}' overflow ({} messages)",
                    queue_status.worst_queue, queue_status.max_depth
                ),
                start.elapsed().as_millis() as u64,
            ),
        }
    }

    /// TAS-75 Phase 5: Check channel saturation health using cached status.
    ///
    /// Uses the BackpressureChecker's cached channel status for non-blocking health checks.
    pub fn check_channel_saturation(&self) -> HealthCheck {
        let start = std::time::Instant::now();

        // TAS-75 Phase 5: Use synchronous cached channel status
        let channel_status = self
            .orchestration_core
            .backpressure_checker()
            .try_get_channel_status();

        if !channel_status.evaluated {
            HealthCheck::unknown(
                "Channel saturation not yet evaluated or check disabled",
                start.elapsed().as_millis() as u64,
            )
        } else if channel_status.is_critical {
            HealthCheck::unhealthy(
                format!(
                    "Command channel critically saturated ({:.1}%)",
                    channel_status.command_saturation_percent
                ),
                start.elapsed().as_millis() as u64,
            )
        } else if channel_status.is_saturated {
            HealthCheck::degraded(
                format!(
                    "Command channel saturated ({:.1}%)",
                    channel_status.command_saturation_percent
                ),
                start.elapsed().as_millis() as u64,
            )
        } else {
            HealthCheck::healthy(
                format!(
                    "Command channel healthy ({:.1}% saturation, {} capacity available)",
                    channel_status.command_saturation_percent,
                    channel_status.command_available_capacity
                ),
                start.elapsed().as_millis() as u64,
            )
        }
    }

    // =========================================================================
    // Helper Methods
    // =========================================================================

    /// Create health info summary.
    pub async fn create_health_info(&self) -> HealthInfo {
        let cached_status = self.orchestration_status.read().await;
        // Get real-time operational state from OrchestrationCore
        let core_status = self.orchestration_core.status().read().await.clone();
        let operational_state = match core_status {
            OrchestrationCoreStatus::Running => "Normal",
            OrchestrationCoreStatus::Starting => "Startup",
            OrchestrationCoreStatus::Stopping => "GracefulShutdown",
            OrchestrationCoreStatus::Stopped => "Stopped",
            OrchestrationCoreStatus::Error(_) => "Emergency",
            OrchestrationCoreStatus::Created => "Startup",
        };

        // TAS-164: Build pool utilization info using shared helper
        let pools = self.orchestration_core.context.database_pools();
        let pool_utilization = Some(build_pool_utilization(pools));

        HealthInfo {
            version: env!("CARGO_PKG_VERSION").to_string(),
            environment: cached_status.environment.clone(),
            operational_state: operational_state.to_string(),
            web_database_pool_size: self.web_db_pool.size(),
            orchestration_database_pool_size: cached_status.database_pool_size,
            circuit_breaker_state: format!("{:?}", self.web_db_circuit_breaker.current_state()),
            pool_utilization,
        }
    }
}
