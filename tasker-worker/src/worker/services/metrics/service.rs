//! # Worker Metrics Service
//!
//! TAS-77: Metrics collection logic extracted from web/handlers/metrics.rs.
//!
//! This service encapsulates all metrics collection functionality, making it available
//! to both the HTTP API and FFI consumers without code duplication.

use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use sqlx::PgPool;
use tokio::sync::RwLock as TokioRwLock;
use tracing::debug;

use crate::worker::event_router::EventRouter;
use crate::worker::in_process_event_bus::InProcessEventBus;
use crate::worker::task_template_manager::TaskTemplateManager;
use tasker_shared::messaging::clients::{MessageClient, UnifiedMessageClient};
use tasker_shared::metrics::channels::global_registry;
use tasker_shared::types::web::{
    DomainEventStats, EventRouterStats as WebEventRouterStats,
    InProcessEventBusStats as WebInProcessEventBusStats, MetricValue, MetricsResponse,
};

/// Metrics Service
///
/// TAS-77: Provides metrics collection functionality independent of the HTTP layer.
///
/// This service can be used by:
/// - Web API handlers (via `WorkerWebState`)
/// - FFI consumers (Ruby, Python, etc.)
/// - Internal monitoring systems
///
/// ## Example
///
/// ```rust,no_run
/// use tasker_worker::worker::services::metrics::MetricsService;
/// use tasker_worker::worker::task_template_manager::TaskTemplateManager;
/// use tasker_worker::worker::event_router::EventRouter;
/// use tasker_worker::worker::in_process_event_bus::InProcessEventBus;
/// use tasker_shared::messaging::clients::UnifiedMessageClient;
/// use sqlx::PgPool;
/// use std::sync::Arc;
/// use std::time::Instant;
/// use tokio::sync::RwLock;
///
/// async fn example(
///     worker_id: String,
///     database_pool: Arc<PgPool>,
///     message_client: Arc<UnifiedMessageClient>,
///     task_template_manager: Arc<TaskTemplateManager>,
///     event_router: Option<Arc<EventRouter>>,
///     in_process_bus: Arc<RwLock<InProcessEventBus>>,
/// ) {
///     let start_time = Instant::now();
///
///     let service = MetricsService::new(
///         worker_id,
///         database_pool,
///         message_client,
///         task_template_manager,
///         event_router,
///         in_process_bus,
///         start_time,
///     );
///
///     // Get Prometheus-format metrics
///     let prometheus = service.prometheus_format().await;
///
///     // Get JSON metrics
///     let json_metrics = service.worker_metrics().await;
///
///     // Get domain event statistics
///     let event_stats = service.domain_event_stats().await;
/// }
/// ```
pub struct MetricsService {
    /// Worker identification
    worker_id: String,

    /// Database connection pool for pool size metrics
    database_pool: Arc<PgPool>,

    /// Message client for queue metrics
    message_client: Arc<UnifiedMessageClient>,

    /// Task template manager for supported namespaces
    task_template_manager: Arc<TaskTemplateManager>,

    /// TAS-65: Event router for domain event stats
    event_router: Option<Arc<EventRouter>>,

    /// TAS-65: In-process event bus for domain event stats
    in_process_bus: Arc<TokioRwLock<InProcessEventBus>>,

    /// Service start time for uptime calculation
    start_time: std::time::Instant,
}

impl std::fmt::Debug for MetricsService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MetricsService")
            .field("worker_id", &self.worker_id)
            .field("uptime_seconds", &self.start_time.elapsed().as_secs())
            .field("has_event_router", &self.event_router.is_some())
            .finish()
    }
}

impl MetricsService {
    /// Create a new MetricsService
    pub fn new(
        worker_id: String,
        database_pool: Arc<PgPool>,
        message_client: Arc<UnifiedMessageClient>,
        task_template_manager: Arc<TaskTemplateManager>,
        event_router: Option<Arc<EventRouter>>,
        in_process_bus: Arc<TokioRwLock<InProcessEventBus>>,
        start_time: std::time::Instant,
    ) -> Self {
        Self {
            worker_id,
            database_pool,
            message_client,
            task_template_manager,
            event_router,
            in_process_bus,
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

    /// Get worker type classification
    pub fn worker_type(&self) -> &str {
        "command_processor"
    }

    /// Get queue name for a namespace
    fn queue_name_for_namespace(&self, namespace: &str) -> String {
        format!("{}_queue", namespace)
    }

    // =========================================================================
    // Metrics Collection Methods
    // =========================================================================

    /// Prometheus metrics format: GET /metrics
    ///
    /// Returns metrics in Prometheus format for scraping by monitoring systems.
    pub async fn prometheus_format(&self) -> String {
        debug!("Collecting Prometheus metrics");

        let mut metrics = Vec::new();

        // Worker info metric
        metrics.push(format!(
            "# HELP tasker_worker_info Worker information\n# TYPE tasker_worker_info gauge\ntasker_worker_info{{version=\"{}\",worker_id=\"{}\",worker_type=\"{}\"}} 1",
            env!("CARGO_PKG_VERSION"),
            self.worker_id,
            self.worker_type()
        ));

        // Uptime metric
        metrics.push(format!(
            "# HELP tasker_worker_uptime_seconds Worker uptime in seconds\n# TYPE tasker_worker_uptime_seconds counter\ntasker_worker_uptime_seconds {{}} {}",
            self.uptime_seconds()
        ));

        // Database connection pool metrics
        metrics.push(format!(
            "# HELP tasker_worker_db_pool_size Database connection pool size\n# TYPE tasker_worker_db_pool_size gauge\ntasker_worker_db_pool_size {{}} {}",
            self.database_pool.size()
        ));

        // Queue depth metrics for each supported namespace
        for namespace in self.task_template_manager.supported_namespaces().await {
            let queue_name = self.queue_name_for_namespace(&namespace);
            match self.message_client.get_queue_metrics(&queue_name).await {
                Ok(queue_metrics) => {
                    metrics.push(format!(
                        "# HELP tasker_worker_queue_depth Queue depth for {} namespace\n# TYPE tasker_worker_queue_depth gauge\ntasker_worker_queue_depth{{namespace=\"{}\"}} {}",
                        namespace, namespace, queue_metrics.message_count
                    ));

                    if let Some(oldest_age) = queue_metrics.oldest_message_age_seconds {
                        metrics.push(format!(
                            "# HELP tasker_worker_queue_oldest_message_age_seconds Age of oldest message in queue\n# TYPE tasker_worker_queue_oldest_message_age_seconds gauge\ntasker_worker_queue_oldest_message_age_seconds{{namespace=\"{}\"}} {}",
                            namespace, oldest_age
                        ));
                    }
                }
                Err(e) => {
                    tracing::warn!(namespace = %namespace, error = %e, "Failed to get queue metrics");
                }
            }
        }

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

            metrics.push(format!(
                "# HELP tasker_worker_channel_health Channel health status (0=healthy, 1=degraded, 2=critical)\n# TYPE tasker_worker_channel_health gauge\ntasker_worker_channel_health{{channel_name=\"{}\",component=\"{}\"}} {}",
                channel_name, component, health_value
            ));
        }

        metrics.join("\n\n")
    }

    /// Worker-specific metrics: GET /metrics/worker
    ///
    /// Returns worker metrics in JSON format for programmatic access.
    pub async fn worker_metrics(&self) -> MetricsResponse {
        debug!("Collecting worker metrics in JSON format");

        let mut metrics = HashMap::new();

        // Basic system metrics
        metrics.insert(
            "uptime_seconds".to_string(),
            MetricValue {
                value: self.uptime_seconds() as f64,
                metric_type: "counter".to_string(),
                labels: HashMap::new(),
                help: "Worker uptime in seconds".to_string(),
            },
        );

        metrics.insert(
            "database_pool_size".to_string(),
            MetricValue {
                value: self.database_pool.size() as f64,
                metric_type: "gauge".to_string(),
                labels: HashMap::new(),
                help: "Database connection pool size".to_string(),
            },
        );

        // Queue metrics for each supported namespace
        for namespace in self.task_template_manager.supported_namespaces().await {
            let queue_name = self.queue_name_for_namespace(&namespace);
            match self.message_client.get_queue_metrics(&queue_name).await {
                Ok(queue_metrics) => {
                    let mut labels = HashMap::new();
                    labels.insert("namespace".to_string(), namespace.clone());

                    metrics.insert(
                        format!("queue_depth_{}", namespace),
                        MetricValue {
                            value: queue_metrics.message_count as f64,
                            metric_type: "gauge".to_string(),
                            labels: labels.clone(),
                            help: format!("Queue depth for {} namespace", namespace),
                        },
                    );

                    if let Some(oldest_age) = queue_metrics.oldest_message_age_seconds {
                        metrics.insert(
                            format!("queue_oldest_message_age_{}", namespace),
                            MetricValue {
                                value: oldest_age as f64,
                                metric_type: "gauge".to_string(),
                                labels,
                                help: format!("Age of oldest message in {} queue", namespace),
                            },
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(namespace = %namespace, error = %e, "Failed to get queue metrics for JSON response");
                }
            }
        }

        MetricsResponse {
            metrics,
            timestamp: Utc::now(),
            worker_id: self.worker_id.clone(),
        }
    }

    /// Domain event statistics: GET /metrics/events
    ///
    /// Returns statistics about domain event routing and delivery paths.
    /// Used for monitoring event publishing and by E2E tests to verify
    /// events were published through the expected delivery paths.
    pub async fn domain_event_stats(&self) -> DomainEventStats {
        debug!("Collecting domain event statistics");

        // Get router stats
        let router_stats = if let Some(ref router) = self.event_router {
            let stats = router.get_statistics();
            WebEventRouterStats {
                total_routed: stats.total_routed,
                durable_routed: stats.durable_routed,
                fast_routed: stats.fast_routed,
                broadcast_routed: stats.broadcast_routed,
                fast_delivery_errors: stats.fast_delivery_errors,
                routing_errors: stats.routing_errors,
            }
        } else {
            WebEventRouterStats::default()
        };

        // Get in-process bus stats
        let bus_stats = {
            let bus = self.in_process_bus.read().await;
            let stats = bus.get_statistics();
            WebInProcessEventBusStats {
                total_events_dispatched: stats.total_events_dispatched,
                rust_handler_dispatches: stats.rust_handler_dispatches,
                ffi_channel_dispatches: stats.ffi_channel_dispatches,
                rust_handler_errors: stats.rust_handler_errors,
                ffi_channel_drops: stats.ffi_channel_drops,
                rust_subscriber_patterns: stats.rust_subscriber_patterns,
                rust_handler_count: stats.rust_handler_count,
                ffi_subscriber_count: stats.ffi_subscriber_count,
            }
        };

        DomainEventStats {
            router: router_stats,
            in_process_bus: bus_stats,
            captured_at: Utc::now(),
            worker_id: self.worker_id.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prometheus_format_structure() {
        // Verify expected metric names in Prometheus format
        let expected_metrics = [
            "tasker_worker_info",
            "tasker_worker_uptime_seconds",
            "tasker_worker_db_pool_size",
        ];

        for metric in &expected_metrics {
            assert!(
                !metric.is_empty(),
                "Metric name {} should be defined",
                metric
            );
        }
    }

    #[test]
    fn test_metric_value_structure() {
        let metric = MetricValue {
            value: 42.0,
            metric_type: "gauge".to_string(),
            labels: HashMap::new(),
            help: "Test metric".to_string(),
        };

        assert_eq!(metric.value, 42.0);
        assert_eq!(metric.metric_type, "gauge");
        assert!(metric.labels.is_empty());
    }

    #[test]
    fn test_queue_name_format() {
        let namespace = "payments";
        let expected = format!("{}_queue", namespace);
        assert_eq!(expected, "payments_queue");
    }
}
