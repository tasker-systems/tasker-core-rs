//! # Tracing Module
//!
//! Environment-aware console logging using the tracing ecosystem.
//! Designed for containerized applications where logs should go to stdout/stderr.
//!
//! This module provides:
//! - Simple console-only logging (container-friendly)
//! - Environment-based log level configuration
//! - Domain-specific structured logging macros
//! - TTY-aware ANSI color output
//! - OpenTelemetry integration for distributed tracing (TAS-29 Phase 3)
//!
//! ## Distributed Tracing with correlation_id (TAS-29 Phase 2 + 3)
//!
//! The Tasker system uses `correlation_id` (a UUID) as the primary distributed tracing
//! identifier across all components. This enables end-to-end request tracking from
//! task creation through orchestration and worker execution.
//!
//! ### Correlation ID Flow
//!
//! ```text
//! 1. Task Creation
//!    - Client creates TaskRequest with correlation_id
//!    - correlation_id stored in tasker_tasks table
//!    - correlation_id propagated to all workflow steps
//!
//! 2. Orchestration Layer
//!    - task_initializer: Fetches correlation_id at entry, logs with correlation_id first
//!    - step_enqueuer: Propagates correlation_id through all step discovery
//!    - result_processor: Extracts correlation_id when processing step results
//!    - task_finalizer: Uses correlation_id throughout finalization decision tree
//!
//! 3. Worker Layer
//!    - step_claim: Receives correlation_id in message, uses for claiming
//!    - command_processor: Propagates correlation_id through execution pipeline
//!    - Ruby/Rust handlers: correlation_id available in execution context
//!
//! 4. Message Queue
//!    - All PGMQ messages include correlation_id field
//!    - SimpleStepMessage, StepExecutionResult include correlation_id
//!    - Enables tracing across async message boundaries
//! ```
//!
//! ### OpenTelemetry Integration
//!
//! When OpenTelemetry is enabled (TELEMETRY_ENABLED=true):
//!
//! 1. **Automatic Span Creation**: The `#[instrument]` macros on hot path functions
//!    automatically create OpenTelemetry spans
//!
//! 2. **correlation_id in Spans**: Since correlation_id is always the first field in
//!    structured logs, it's automatically captured in span attributes
//!
//! 3. **Trace Context Propagation**: The TraceContextPropagator ensures trace context
//!    flows across service boundaries following W3C Trace Context specification
//!
//! 4. **Service Mesh Integration**: correlation_id enables correlation between:
//!    - Orchestration service spans
//!    - Worker service spans
//!    - Database query traces
//!    - Message queue operations
//!
//! ### Configuration
//!
//! OpenTelemetry is configured via environment variables:
//!
//! ```bash
//! # Enable OpenTelemetry
//! export TELEMETRY_ENABLED=true
//!
//! # OTLP endpoint (default: http://localhost:4317)
//! export OTEL_EXPORTER_OTLP_ENDPOINT=http://localhost:4317
//!
//! # Service identification
//! export OTEL_SERVICE_NAME=tasker-orchestration
//! export OTEL_SERVICE_VERSION=0.1.0
//!
//! # Sampling rate (0.0 to 1.0, default: 1.0 = 100%)
//! export OTEL_TRACES_SAMPLER_ARG=1.0
//! ```
//!
//! ### Backend-Specific Configuration
//!
//! See `config/tasker/base/telemetry.toml` for commented examples of:
//! - Honeycomb (professional observability platform)
//! - Jaeger (open source tracing)
//! - Grafana Tempo (open source tracing)
//!
//! Example Honeycomb configuration:
//! ```bash
//! export TELEMETRY_ENABLED=true
//! export OTEL_EXPORTER_OTLP_ENDPOINT=https://api.honeycomb.io:443
//! export OTEL_EXPORTER_OTLP_HEADERS="x-honeycomb-team=YOUR_API_KEY,x-honeycomb-dataset=tasker-core"
//! ```
//!
//! ### Querying Traces
//!
//! With OpenTelemetry enabled, you can query traces by correlation_id:
//!
//! - **Honeycomb**: `correlation_id = "550e8400-e29b-41d4-a716-446655440000"`
//! - **Jaeger**: Search by tag `correlation_id`
//! - **Grafana Tempo**: TraceQL: `{correlation_id="550e8400-e29b-41d4-a716-446655440000"}`

use chrono::Utc;
use std::io::IsTerminal;
use std::sync::OnceLock;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};
use uuid::Uuid;

// OpenTelemetry imports (TAS-29 Phase 3)
use opentelemetry::{
    trace::{TraceError, TracerProvider as _},
    KeyValue,
};
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    propagation::TraceContextPropagator,
    runtime,
    trace::{Sampler, TracerProvider},
    Resource,
};
use tracing_opentelemetry::OpenTelemetryLayer;

static TRACING_INITIALIZED: OnceLock<()> = OnceLock::new();

/// Configuration for OpenTelemetry (loaded from environment or defaults)
struct TelemetryConfig {
    enabled: bool,
    service_name: String,
    service_version: String,
    deployment_environment: String,
    otlp_endpoint: String,
    sample_rate: f64,
}

impl Default for TelemetryConfig {
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
            sample_rate: std::env::var("OTEL_TRACES_SAMPLER_ARG")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1.0),
        }
    }
}

/// Initialize OpenTelemetry tracer provider
///
/// Creates a TracerProvider configured with OTLP exporter and appropriate
/// resource attributes for distributed tracing.
fn init_opentelemetry_tracer(config: &TelemetryConfig) -> Result<TracerProvider, TraceError> {
    // Set global propagator for context propagation
    opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());

    // Build resource attributes
    let resource = Resource::new(vec![
        KeyValue::new("service.name", config.service_name.clone()),
        KeyValue::new("service.version", config.service_version.clone()),
        KeyValue::new(
            "deployment.environment",
            config.deployment_environment.clone(),
        ),
    ]);

    // Configure tracer with sampling
    let sampler = if config.sample_rate >= 1.0 {
        Sampler::AlwaysOn
    } else if config.sample_rate <= 0.0 {
        Sampler::AlwaysOff
    } else {
        Sampler::TraceIdRatioBased(config.sample_rate)
    };

    // Use the OTLP pipeline to build the tracer provider
    let tracer_provider = opentelemetry_otlp::new_pipeline()
        .tracing()
        .with_exporter(
            opentelemetry_otlp::new_exporter()
                .tonic()
                .with_endpoint(&config.otlp_endpoint),
        )
        .with_trace_config(
            opentelemetry_sdk::trace::Config::default()
                .with_sampler(sampler)
                .with_resource(resource),
        )
        .install_batch(runtime::Tokio)?;

    Ok(tracer_provider)
}

/// Initialize tracing with console output and optional OpenTelemetry
///
/// This function sets up structured logging that outputs to stdout/stderr,
/// which is appropriate for containerized applications and follows modern
/// observability practices.
///
/// When OpenTelemetry is enabled (via TELEMETRY_ENABLED=true), it also
/// configures distributed tracing with OTLP exporter.
pub fn init_tracing() {
    TRACING_INITIALIZED.get_or_init(|| {
        let environment = get_environment();
        let log_level = get_log_level(&environment);
        let telemetry_config = TelemetryConfig::default();

        // Determine if we're in a TTY for ANSI color support
        let use_ansi = IsTerminal::is_terminal(&std::io::stdout());

        // Create base console layer
        let console_layer = fmt::layer()
            .with_target(true)
            .with_thread_ids(true)
            .with_level(true)
            .with_ansi(use_ansi)
            .with_filter(EnvFilter::new(&log_level));

        // Build subscriber with optional OpenTelemetry layer
        let subscriber = tracing_subscriber::registry().with(console_layer);

        if telemetry_config.enabled {
            // Initialize OpenTelemetry and add layer
            match init_opentelemetry_tracer(&telemetry_config) {
                Ok(tracer_provider) => {
                    let tracer = tracer_provider.tracer("tasker-core");
                    let telemetry_layer = OpenTelemetryLayer::new(tracer);

                    let subscriber = subscriber.with(telemetry_layer);

                    // Use try_init to avoid panic if global subscriber already set
                    if subscriber.try_init().is_err() {
                        tracing::debug!("Global tracing subscriber already initialized - continuing with existing subscriber");
                    } else {
                        tracing::info!(
                            environment = %environment,
                            ansi_colors = use_ansi,
                            opentelemetry_enabled = true,
                            otlp_endpoint = %telemetry_config.otlp_endpoint,
                            service_name = %telemetry_config.service_name,
                            "Console logging with OpenTelemetry initialized"
                        );
                    }
                }
                Err(e) => {
                    // Fall back to console-only logging
                    let subscriber = subscriber;
                    if subscriber.try_init().is_err() {
                        tracing::debug!("Global tracing subscriber already initialized - continuing with existing subscriber");
                    } else {
                        tracing::warn!(
                            environment = %environment,
                            ansi_colors = use_ansi,
                            error = %e,
                            "Failed to initialize OpenTelemetry - falling back to console-only logging"
                        );
                    }
                }
            }
        } else {
            // Console-only logging
            if subscriber.try_init().is_err() {
                tracing::debug!("Global tracing subscriber already initialized - continuing with existing subscriber");
            } else {
                tracing::info!(
                    environment = %environment,
                    ansi_colors = use_ansi,
                    opentelemetry_enabled = false,
                    "Console logging initialized"
                );
            }
        }
    });
}

/// Shutdown OpenTelemetry gracefully
///
/// This should be called before application exit to ensure all pending
/// spans are exported.
pub fn shutdown_telemetry() {
    opentelemetry::global::shutdown_tracer_provider();
}

/// Legacy alias for backward compatibility
///
/// This function is deprecated and will be removed in a future version.
/// Use `init_tracing()` instead.
#[deprecated(since = "0.2.0", note = "Use init_tracing() instead")]
pub fn init_structured_logging() {
    init_tracing();
}

/// Get current environment from environment variables
fn get_environment() -> String {
    std::env::var("TASKER_ENV")
        .or_else(|_| std::env::var("APP_ENV"))
        .unwrap_or_else(|_| "development".to_string())
}

/// Get log level based on environment variables or environment defaults
fn get_log_level(environment: &str) -> String {
    // First check for explicit LOG_LEVEL environment variable
    if let Ok(level) = std::env::var("LOG_LEVEL") {
        return level.to_lowercase();
    }

    // Then check for RUST_LOG environment variable
    if let Ok(level) = std::env::var("RUST_LOG") {
        return level.to_lowercase();
    }

    // Fall back to environment-based defaults
    match environment {
        "test" => "debug".to_string(),
        "development" => "debug".to_string(),
        "production" => "info".to_string(),
        _ => "debug".to_string(),
    }
}

/// Unified logging macros that match Ruby TaskerCore::Logging::Logger patterns
///
/// These macros provide structured logging with the same emoji + component format
/// used by the Ruby side, ensuring consistent log output across languages.
///
/// Log task operations with unified Ruby-compatible format
#[macro_export]
macro_rules! log_task {
    // Full form with task_uuid
    ($level:ident, $operation:expr, task_uuid: $task_uuid:expr, $($key:ident: $value:expr),* $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            task_uuid = format!("{:?}", $task_uuid),
            $($key = ?$value,)*
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "{}", $operation
        );
    };
    // Simple form - just operation
    ($level:ident, $operation:expr $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "{}", $operation
        );
    };
    // Generic form with additional fields
    ($level:ident, $operation:expr, $($key:ident: $value:expr),+ $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            $($key = ?$value,)*
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "{}", $operation
        );
    };
}

/// Log queue worker operations with unified Ruby-compatible format
#[macro_export]
macro_rules! log_queue_worker {
    // Full form with namespace
    ($level:ident, $operation:expr, namespace: $namespace:expr, $($key:ident: $value:expr),* $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            namespace = %$namespace,
            $($key = ?$value,)*
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "QUEUE_{} (namespace: {})", $operation, $namespace
        );
    };
    // Simple form - just operation
    ($level:ident, $operation:expr $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "QUEUE_{}", $operation
        );
    };
    // Generic form with additional fields
    ($level:ident, $operation:expr, $($key:ident: $value:expr),+ $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            $($key = ?$value,)*
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "QUEUE_{}", $operation
        );
    };
}

/// Log orchestrator operations with unified Ruby-compatible format
#[macro_export]
macro_rules! log_orchestrator {
    // Simple form - just operation
    ($level:ident, $operation:expr $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "{}", $operation
        );
    };
    // Generic form with additional fields
    ($level:ident, $operation:expr, $($key:ident: $value:expr),+ $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            $($key = ?$value,)*
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "{}", $operation
        );
    };
}

/// Log step operations with unified Ruby-compatible format
#[macro_export]
macro_rules! log_step {
    // Full form with step_uuid and task_uuid
    ($level:ident, $operation:expr, step_uuid: $step_uuid:expr, task_uuid: $task_uuid:expr, $($key:ident: $value:expr),* $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            step_uuid = format!("{:?}", $step_uuid),
            task_uuid = format!("{:?}", $task_uuid),
            $($key = ?$value,)*
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "{}", $operation
        );
    };
    // Simple form - just operation
    ($level:ident, $operation:expr $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "{}", $operation
        );
    };
    // Generic form with additional fields
    ($level:ident, $operation:expr, $($key:ident: $value:expr),+ $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            $($key = ?$value,)*
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "{}", $operation
        );
    };
}

/// Log database operations with unified Ruby-compatible format
#[macro_export]
macro_rules! log_database {
    // Simple form - just operation
    ($level:ident, $operation:expr $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "{}", $operation
        );
    };
    // Generic form with additional fields
    ($level:ident, $operation:expr, $($key:ident: $value:expr),+ $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            $($key = ?$value,)*
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "{}", $operation
        );
    };
}

/// Log FFI operations with unified Ruby-compatible format
#[macro_export]
macro_rules! log_ffi {
    // Full form with component
    ($level:ident, $operation:expr, component: $component:expr, $($key:ident: $value:expr),* $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            component = %$component,
            $($key = ?$value,)*
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "{} ({})", $operation, $component
        );
    };
    // Simple form - just operation
    ($level:ident, $operation:expr $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "{}", $operation
        );
    };
    // Generic form with additional fields
    ($level:ident, $operation:expr, $($key:ident: $value:expr),+ $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            $($key = ?$value,)*
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "{}", $operation
        );
    };
}

/// Log configuration operations with unified Ruby-compatible format
#[macro_export]
macro_rules! log_config {
    // Simple form - just operation
    ($level:ident, $operation:expr $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "{}", $operation
        );
    };
    // Generic form with additional fields
    ($level:ident, $operation:expr, $($key:ident: $value:expr),+ $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            $($key = ?$value,)*
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "{}", $operation
        );
    };
}

/// Log registry operations with unified Ruby-compatible format
#[macro_export]
macro_rules! log_registry {
    // Full form with namespace and name
    ($level:ident, $operation:expr, namespace: $namespace:expr, name: $name:expr, $($key:ident: $value:expr),* $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            namespace = %$namespace,
            name = %$name,
            $($key = ?$value,)*
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "{} ({}/{})", $operation, $namespace, $name
        );
    };
    // Simple form - just operation
    ($level:ident, $operation:expr $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "{}", $operation
        );
    };
    // Generic form with additional fields
    ($level:ident, $operation:expr, $($key:ident: $value:expr),+ $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            $($key = ?$value,)*
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "{}", $operation
        );
    };
}

/// Legacy function for task operations (maintained for backward compatibility)
pub fn log_task_operation(
    operation: &str,
    task_uuid: Option<Uuid>,
    task_name: Option<&str>,
    namespace: Option<&str>,
    status: &str,
    details: Option<&str>,
) {
    log_task!(info, operation,
        task_uuid: task_uuid,
        task_name: task_name,
        namespace: namespace,
        status: status,
        details: details
    );
}

/// Legacy function for step operations (maintained for backward compatibility)
pub fn log_step_operation(
    operation: &str,
    task_uuid: Option<Uuid>,
    step_uuid: Option<Uuid>,
    step_name: Option<&str>,
    status: &str,
    details: Option<&str>,
) {
    if let (Some(step_uuid), Some(task_uuid)) = (step_uuid, task_uuid) {
        log_step!(info, operation,
            step_uuid: step_uuid,
            task_uuid: task_uuid,
            step_name: step_name,
            status: status,
            details: details
        );
    } else {
        log_step!(info, operation,
            step_name: step_name,
            status: status,
            details: details
        );
    }
}

/// Legacy function for FFI operations (maintained for backward compatibility)
pub fn log_ffi_operation(
    operation: &str,
    component: &str,
    status: &str,
    details: Option<&str>,
    data: Option<&str>,
) {
    log_ffi!(info, operation,
        component: component,
        status: status,
        details: details,
        data: data
    );
}

/// Legacy function for registry operations (maintained for backward compatibility)
pub fn log_registry_operation(
    operation: &str,
    namespace: Option<&str>,
    name: Option<&str>,
    version: Option<&str>,
    status: &str,
    details: Option<&str>,
) {
    if let (Some(namespace), Some(name)) = (namespace, name) {
        log_registry!(info, operation,
            namespace: namespace,
            name: name,
            version: version,
            status: status,
            details: details
        );
    } else {
        log_registry!(info, operation,
            version: version,
            status: status,
            details: details
        );
    }
}

/// Legacy function for database operations (maintained for backward compatibility)
pub fn log_database_operation(
    operation: &str,
    table: Option<&str>,
    record_id: Option<i64>,
    status: &str,
    duration_ms: Option<u64>,
    details: Option<&str>,
) {
    log_database!(info, operation,
        table: table,
        record_id: record_id,
        status: status,
        duration_ms: duration_ms,
        details: details
    );
}

/// Generic error logging with unified format
pub fn log_error(component: &str, operation: &str, error: &str, context: Option<&str>) {
    tracing::error!(
        component = %component,
        operation = %operation,
        error = %error,
        context = context,
        timestamp = %Utc::now().to_rfc3339(),
        "ERROR: {} failed in {}: {}", operation, component, error
    );
}

/// Example usage of unified logging macros that match Ruby patterns
///
/// These examples show how to use the new macros for consistent cross-language logging:
///
/// ```rust
/// use tasker_shared::{log_task, log_queue_worker, log_orchestrator, log_config};
///
/// // Task operations
/// log_task!(info, "Creating task", task_uuid: Some(123), namespace: "fulfillment");
///
/// // Queue worker operations
/// log_queue_worker!(debug, "Processing batch", namespace: "fulfillment", batch_size: 5);
///
/// // Orchestrator operations
/// log_orchestrator!(info, "Starting system", namespaces: vec!["default", "fulfillment"]);
///
/// // Configuration operations
/// log_config!(warn, "Using fallback configuration", reason: "File not found");
/// ```
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_environment_detection() {
        std::env::set_var("TASKER_ENV", "test");
        let env = get_environment();
        assert_eq!(env, "test");
        std::env::remove_var("TASKER_ENV");
    }

    #[test]
    fn test_log_level_mapping() {
        // Remove environment variables first
        std::env::remove_var("LOG_LEVEL");
        std::env::remove_var("RUST_LOG");

        // Test default environment-based levels
        assert_eq!(get_log_level("test"), "debug");
        assert_eq!(get_log_level("development"), "debug");
        assert_eq!(get_log_level("production"), "info");
        assert_eq!(get_log_level("unknown"), "debug");

        // Test LOG_LEVEL environment variable override
        std::env::set_var("LOG_LEVEL", "INFO");
        assert_eq!(get_log_level("test"), "info");
        assert_eq!(get_log_level("development"), "info");

        // Test RUST_LOG environment variable override (lower priority than LOG_LEVEL)
        std::env::remove_var("LOG_LEVEL");
        std::env::set_var("RUST_LOG", "WARN");
        assert_eq!(get_log_level("test"), "warn");

        // Clean up
        std::env::remove_var("LOG_LEVEL");
        std::env::remove_var("RUST_LOG");
    }

    #[test]
    fn test_unified_logging_macros_compile() {
        // These tests verify that the macros compile correctly with different parameter patterns

        // Test log_task macro variations
        log_task!(info, "test operation");
        log_task!(debug, "test with data", task_uuid: Some(123), status: "running");
        log_task!(warn, "task warning", task_uuid: Some(456), namespace: "test_ns", details: Some("test details"));

        // Test log_queue_worker macro variations
        log_queue_worker!(info, "worker test");
        log_queue_worker!(debug, "processing", namespace: "test_ns");
        log_queue_worker!(error, "worker error", namespace: "test_ns", error_count: 5);

        // Test log_orchestrator macro
        log_orchestrator!(info, "orchestrator test", active_tasks: 10);

        // Test log_step macro variations
        log_step!(info, "step test");
        log_step!(debug, "step with ids", step_uuid: Some(789), task_uuid: Some(123));

        // Test log_database macro
        log_database!(warn, "slow query", table: Some("tasks"), duration_ms: Some(5000));

        // Test log_ffi macro variations
        log_ffi!(info, "ffi test");
        log_ffi!(error, "ffi error", component: "ruby_bridge", error_code: 500);

        // Test log_config macro
        log_config!(info, "config loaded", environment: "test");

        // Test log_registry macro variations
        log_registry!(info, "registry test");
        log_registry!(debug, "template registered", namespace: "fulfillment", name: "order_processing");

        // Test should complete without panic - macros compile and execute
    }
}
