//! # OpenTelemetry Metrics Module (TAS-29 Phase 3.3)
//!
//! This module provides OpenTelemetry metrics collection for the Tasker system.
//! Metrics are exported via OTLP to the configured observability backend (Grafana LGTM, Honeycomb, etc.).
//!
//! ## Architecture
//!
//! The metrics system is organized into domain-specific modules:
//! - `orchestration`: Task lifecycle metrics (counters, histograms, gauges)
//! - `worker`: Worker step execution metrics
//! - `database`: SQL query duration metrics
//! - `messaging`: Message queue latency metrics
//!
//! ## Usage
//!
//! ```rust
//! use tasker_shared::metrics;
//! use opentelemetry::KeyValue;
//!
//! // Initialize metrics (usually called in init_tracing())
//! metrics::init_metrics();
//!
//! // Use domain-specific metrics
//! let correlation_id = uuid::Uuid::new_v4();
//! metrics::orchestration::task_requests_total().add(
//!     1,
//!     &[KeyValue::new("correlation_id", correlation_id.to_string())],
//! );
//! ```
//!
//! ## Configuration
//!
//! Metrics are configured via the same telemetry configuration as tracing:
//! - `TELEMETRY_ENABLED=true` - Enable metrics export
//! - `OTEL_EXPORTER_OTLP_ENDPOINT` - OTLP endpoint (default: http://localhost:4317)
//! - `OTEL_SERVICE_NAME` - Service name for resource attributes
//!
//! Export interval is fixed at 60 seconds per TAS-29 specification.

use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    metrics::{reader::DefaultTemporalitySelector, PeriodicReader, SdkMeterProvider},
    runtime, Resource,
};
use std::sync::OnceLock;
use std::time::Duration;

pub mod channels;
pub mod database;
pub mod messaging;
pub mod orchestration;
pub mod worker;

/// Global metrics initialization state
static METRICS_INITIALIZED: OnceLock<()> = OnceLock::new();

/// Telemetry configuration (reused from logging module concept)
#[derive(Debug, Clone)]
pub struct MetricsConfig {
    pub enabled: bool,
    pub service_name: String,
    pub service_version: String,
    pub deployment_environment: String,
    pub otlp_endpoint: String,
    pub export_interval_seconds: u64,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: std::env::var("TELEMETRY_ENABLED")
                .map(|v| v.to_lowercase() == "true")
                .unwrap_or(false),
            service_name: std::env::var("OTEL_SERVICE_NAME")
                .unwrap_or_else(|_| "tasker-core".to_string()),
            service_version: std::env::var("OTEL_SERVICE_VERSION")
                .unwrap_or_else(|_| "0.1.0".to_string()),
            deployment_environment: std::env::var("DEPLOYMENT_ENVIRONMENT")
                .or_else(|_| std::env::var("TASKER_ENV"))
                .unwrap_or_else(|_| "development".to_string()),
            otlp_endpoint: std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
                .unwrap_or_else(|_| "http://localhost:4317".to_string()),
            export_interval_seconds: 60, // Fixed per TAS-29 spec
        }
    }
}

/// Initialize OpenTelemetry MeterProvider
///
/// Creates a MeterProvider configured with OTLP exporter and periodic reader.
/// Exports metrics every 60 seconds as specified in TAS-29 Phase 3.3.
fn init_opentelemetry_meter(
    config: &MetricsConfig,
) -> Result<SdkMeterProvider, Box<dyn std::error::Error>> {
    // Build resource attributes
    let resource = Resource::new(vec![
        KeyValue::new("service.name", config.service_name.clone()),
        KeyValue::new("service.version", config.service_version.clone()),
        KeyValue::new(
            "deployment.environment",
            config.deployment_environment.clone(),
        ),
    ]);

    // Create OTLP exporter with default temporality selector (Cumulative)
    let exporter = opentelemetry_otlp::new_exporter()
        .tonic()
        .with_endpoint(&config.otlp_endpoint)
        .build_metrics_exporter(Box::new(DefaultTemporalitySelector::new()))?;

    // Create periodic reader with 60-second export interval
    let reader = PeriodicReader::builder(exporter, runtime::Tokio)
        .with_interval(Duration::from_secs(config.export_interval_seconds))
        .build();

    // Build MeterProvider
    let meter_provider = SdkMeterProvider::builder()
        .with_reader(reader)
        .with_resource(resource)
        .build();

    Ok(meter_provider)
}

/// Initialize metrics collection with OpenTelemetry
///
/// This function should be called during application startup to initialize
/// the global MeterProvider. It's safe to call multiple times - subsequent
/// calls are no-ops.
///
/// Metrics are only initialized if `TELEMETRY_ENABLED=true` is set.
pub fn init_metrics() {
    METRICS_INITIALIZED.get_or_init(|| {
        let config = MetricsConfig::default();

        if config.enabled {
            match init_opentelemetry_meter(&config) {
                Ok(meter_provider) => {
                    opentelemetry::global::set_meter_provider(meter_provider);

                    // Initialize domain-specific metrics
                    channels::init();
                    orchestration::init();
                    worker::init();
                    // database and messaging metrics are initialized on-demand

                    tracing::info!(
                        service_name = %config.service_name,
                        otlp_endpoint = %config.otlp_endpoint,
                        export_interval_seconds = config.export_interval_seconds,
                        "OpenTelemetry metrics initialized (channels, orchestration, worker)"
                    );
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "Failed to initialize OpenTelemetry metrics - metrics collection disabled"
                    );
                }
            }
        } else {
            tracing::debug!("Metrics collection disabled (TELEMETRY_ENABLED=false)");
        }
    });
}

/// Shutdown OpenTelemetry metrics gracefully
///
/// This should be called before application exit to ensure all pending
/// metrics are exported.
///
/// Note: In OpenTelemetry 0.26, the MeterProvider is managed by the global
/// provider and will be shut down when the global provider is dropped.
/// This function is provided for API consistency but currently is a no-op.
pub fn shutdown_metrics() {
    // In OpenTelemetry 0.26, there's no explicit global shutdown for metrics
    // The MeterProvider is cleaned up when it's dropped
    tracing::debug!("Metrics shutdown requested - metrics will flush on drop");
}
