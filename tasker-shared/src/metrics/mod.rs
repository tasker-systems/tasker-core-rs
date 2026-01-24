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
use opentelemetry_prometheus_text_exporter::PrometheusExporter;
use opentelemetry_sdk::{
    metrics::{PeriodicReader, SdkMeterProvider},
    Resource,
};
use std::sync::OnceLock;
use std::time::Duration;

pub mod channels;
pub mod database;
pub mod health;
pub mod messaging;
pub mod orchestration;
pub mod security;
pub mod worker;

/// Global metrics initialization state
static METRICS_INITIALIZED: OnceLock<()> = OnceLock::new();

/// Global Prometheus exporter for scraping metrics
/// TAS-65 Phase 1: Store exporter for /metrics endpoint access
static PROMETHEUS_EXPORTER: OnceLock<PrometheusExporter> = OnceLock::new();

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
///
/// Note: Currently unused as we use Prometheus text exporter instead.
/// Kept for potential future OTLP metrics export functionality.
#[expect(
    dead_code,
    reason = "Reserved for future OTLP metrics export - currently using Prometheus"
)]
fn init_opentelemetry_meter(
    config: &MetricsConfig,
) -> Result<SdkMeterProvider, Box<dyn std::error::Error>> {
    // Build resource with builder pattern (OpenTelemetry 0.28+ API)
    let resource = Resource::builder()
        .with_service_name(config.service_name.clone())
        .with_attributes([
            KeyValue::new("service.version", config.service_version.clone()),
            KeyValue::new(
                "deployment.environment",
                config.deployment_environment.clone(),
            ),
        ])
        .build();

    // Create OTLP metrics exporter (OpenTelemetry 0.27+ API)
    let exporter = opentelemetry_otlp::MetricExporter::builder()
        .with_tonic()
        .with_endpoint(config.otlp_endpoint.clone())
        .build()?;

    // Create periodic reader with 60-second export interval
    // OpenTelemetry 0.31+: Runtime is configured in exporter, not reader
    let reader = PeriodicReader::builder(exporter)
        .with_interval(Duration::from_secs(config.export_interval_seconds))
        .build();

    // Build MeterProvider
    let meter_provider = SdkMeterProvider::builder()
        .with_reader(reader)
        .with_resource(resource)
        .build();

    Ok(meter_provider)
}

/// Initialize metrics collection with OpenTelemetry Prometheus text exporter
///
/// This function should be called during application startup to initialize
/// the global MeterProvider with Prometheus text exporter. It's safe to call
/// multiple times - subsequent calls are no-ops.
///
/// TAS-65 Phase 1: Uses Prometheus text exporter for /metrics endpoint.
/// Metrics are always enabled (no TELEMETRY_ENABLED check needed for Prometheus).
pub fn init_metrics() {
    METRICS_INITIALIZED.get_or_init(|| {
        // Create Prometheus text exporter
        let exporter = PrometheusExporter::new();

        // Build resource with builder pattern (OpenTelemetry 0.28+ API)
        let resource = Resource::builder()
            .with_service_name(
                std::env::var("OTEL_SERVICE_NAME").unwrap_or_else(|_| "tasker-core".to_string()),
            )
            .with_attributes([
                KeyValue::new(
                    "service.version",
                    std::env::var("OTEL_SERVICE_VERSION").unwrap_or_else(|_| "0.1.0".to_string()),
                ),
                KeyValue::new(
                    "deployment.environment",
                    std::env::var("DEPLOYMENT_ENVIRONMENT")
                        .or_else(|_| std::env::var("TASKER_ENV"))
                        .unwrap_or_else(|_| "development".to_string()),
                ),
            ])
            .build();

        // Create meter provider with Prometheus exporter as reader
        let provider = SdkMeterProvider::builder()
            .with_reader(exporter.clone())
            .with_resource(resource)
            .build();

        // Set as global meter provider
        opentelemetry::global::set_meter_provider(provider);

        // Store exporter for /metrics endpoint access
        PROMETHEUS_EXPORTER
            .set(exporter)
            .expect("Failed to set PROMETHEUS_EXPORTER");

        // Initialize domain-specific metrics
        channels::init();
        database::init();
        health::init();
        orchestration::init();
        worker::init();
        // messaging metrics are initialized on-demand

        tracing::info!(
            "OpenTelemetry Prometheus text exporter initialized (channels, database, health, orchestration, worker)"
        );
    });
}

/// Get the Prometheus exporter for exporting metrics
///
/// Returns a reference to the global PrometheusExporter that can be used
/// to export metrics in Prometheus text format via `exporter.export(&mut buf)`.
///
/// # Panics
///
/// Panics if metrics have not been initialized via `init_metrics()`.
///
/// # Example
///
/// ```rust,no_run
/// use tasker_shared::metrics;
///
/// // Initialize metrics first
/// metrics::init_metrics();
///
/// // Get metrics in Prometheus format
/// let exporter = metrics::prometheus_exporter();
/// let mut output = Vec::new();
/// exporter.export(&mut output).unwrap();
/// let metrics_text = String::from_utf8(output).unwrap();
/// ```
pub fn prometheus_exporter() -> &'static PrometheusExporter {
    PROMETHEUS_EXPORTER
        .get()
        .expect("Metrics not initialized - call init_metrics() first")
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
