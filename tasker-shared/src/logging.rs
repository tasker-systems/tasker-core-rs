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
//! - OpenTelemetry integration for distributed tracing and log forwarding
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
//!    - correlation_id stored in tasker.tasks table
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
//! 2. **Log Forwarding**: All tracing logs are automatically forwarded to the OTLP
//!    collector as OpenTelemetry logs, enabling unified observability
//!
//! 3. **correlation_id in Spans and Logs**: Since correlation_id is always the first field in
//!    structured logs, it's automatically captured in span attributes and log records
//!
//! 4. **Trace Context Propagation**: The TraceContextPropagator ensures trace context
//!    flows across service boundaries following W3C Trace Context specification
//!
//! 5. **Service Mesh Integration**: correlation_id enables correlation between:
//!    - Orchestration service spans and logs
//!    - Worker service spans and logs
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
//! Different observability backends can be configured via environment variables:
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

use crate::metrics;

use opentelemetry::{trace::TracerProvider as _, KeyValue};
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::{
    logs::SdkLoggerProvider,
    propagation::TraceContextPropagator,
    trace::{Sampler, SdkTracerProvider},
    Resource,
};
use tracing_opentelemetry::OpenTelemetryLayer;

static TRACING_INITIALIZED: OnceLock<()> = OnceLock::new();
/// Store TracerProvider handle for proper shutdown
/// TAS-29: Enables graceful shutdown with span flushing
static TRACER_PROVIDER: OnceLock<SdkTracerProvider> = OnceLock::new();
/// Store LoggerProvider handle for proper shutdown
/// Enables graceful shutdown with log flushing
static LOGGER_PROVIDER: OnceLock<SdkLoggerProvider> = OnceLock::new();

/// Configuration for OpenTelemetry (loaded from environment or defaults)
struct TelemetryConfig {
    enabled: bool,
    /// Whether to forward logs to OTEL (in addition to stdout)
    /// Defaults to false to reduce telemetry volume - logs stay on stdout,
    /// traces and metrics go to OTEL. Set OTEL_LOGS_ENABLED=true to enable.
    logs_enabled: bool,
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
            // Log forwarding disabled by default to reduce OTEL volume
            // Logs go to stdout (Docker captures them), traces/metrics go to OTEL
            // This prevents collector OOM from high log volume (TAS-94)
            logs_enabled: std::env::var("OTEL_LOGS_ENABLED")
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
fn init_opentelemetry_tracer(
    config: &TelemetryConfig,
) -> Result<SdkTracerProvider, Box<dyn std::error::Error>> {
    // Set global propagator for context propagation
    opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());

    // Build resource attributes
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

    // Configure tracer with sampling
    let sampler = if config.sample_rate >= 1.0 {
        Sampler::AlwaysOn
    } else if config.sample_rate <= 0.0 {
        Sampler::AlwaysOff
    } else {
        Sampler::TraceIdRatioBased(config.sample_rate)
    };

    // Build OTLP exporter (OpenTelemetry 0.27+ API)
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(config.otlp_endpoint.clone())
        .build()?;

    // Build tracer provider with sampler and resource
    // OpenTelemetry 0.31+: Sampler is set directly on builder
    // TAS-65: Use batch processor for non-blocking async export (not simple/sync)
    let tracer_provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource)
        .with_sampler(sampler)
        .build();

    // Store provider handle for graceful shutdown (TAS-29 Phase 3.3)
    let _ = TRACER_PROVIDER.set(tracer_provider.clone());

    Ok(tracer_provider)
}

/// Initialize OpenTelemetry logger provider for log forwarding
///
/// Creates a LoggerProvider configured with OTLP exporter to send logs
/// to the observability backend.
fn init_opentelemetry_logger(
    config: &TelemetryConfig,
) -> Result<SdkLoggerProvider, Box<dyn std::error::Error>> {
    // Build resource attributes (same as tracer)
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

    // Build OTLP log exporter
    let exporter = opentelemetry_otlp::LogExporter::builder()
        .with_tonic()
        .with_endpoint(config.otlp_endpoint.clone())
        .build()?;

    // Build logger provider with batch processor
    let logger_provider = SdkLoggerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource)
        .build();

    // Store provider handle for graceful shutdown
    let _ = LOGGER_PROVIDER.set(logger_provider.clone());

    Ok(logger_provider)
}

/// Initialize console-only logging (FFI-safe, no Tokio runtime required)
///
/// This function sets up structured console logging without OpenTelemetry,
/// making it safe to call from FFI initialization contexts where no Tokio
/// runtime exists yet.
///
/// # Use Cases
///
/// - Ruby FFI: Called during Magnus initialization before runtime creation
/// - Python FFI: Called during PyO3 initialization before runtime creation
/// - WASM FFI: Called during WASM initialization before runtime creation
///
/// # Pattern
///
/// ```rust
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Phase 1: FFI initialization (no Tokio runtime)
/// tasker_shared::logging::init_console_only();
///
/// // Phase 2: After creating runtime (Tokio context)
/// let runtime = tokio::runtime::Runtime::new()?;
/// runtime.block_on(async {
///     tasker_shared::logging::init_tracing();
/// });
/// # Ok(())
/// # }
/// ```
pub fn init_console_only() {
    TRACING_INITIALIZED.get_or_init(|| {
        let environment = get_environment();
        let log_level = get_log_level(&environment);

        // Determine if we're in a TTY for ANSI color support
        let use_ansi = IsTerminal::is_terminal(&std::io::stdout());

        // Create base console layer
        let console_layer = fmt::layer()
            .with_target(true)
            .with_thread_ids(true)
            .with_level(true)
            .with_ansi(use_ansi)
            .with_filter(EnvFilter::new(&log_level));

        // Build subscriber with console layer only (no telemetry)
        let subscriber = tracing_subscriber::registry().with(console_layer);

        if subscriber.try_init().is_err() {
            tracing::debug!(
                "Global tracing subscriber already initialized - continuing with existing subscriber"
            );
        } else {
            tracing::info!(
                environment = %environment,
                ansi_colors = use_ansi,
                opentelemetry_enabled = false,
                context = "ffi_initialization",
                "Console-only logging initialized (FFI-safe mode)"
            );
        }

        // Initialize basic metrics (no OpenTelemetry exporters)
        metrics::init_metrics();

        // Initialize domain-specific metrics
        metrics::orchestration::init();
        metrics::worker::init();
        metrics::database::init();
        metrics::messaging::init();
    });
}

/// Initialize tracing with console output and optional OpenTelemetry
///
/// This function sets up structured logging that outputs to stdout/stderr,
/// which is appropriate for containerized applications and follows modern
/// observability practices.
///
/// When OpenTelemetry is enabled (via TELEMETRY_ENABLED=true), it also
/// configures distributed tracing with OTLP exporter.
///
/// # Safety
///
/// **IMPORTANT**: When telemetry is enabled, this function MUST be called from
/// a Tokio runtime context because the batch exporter requires async I/O.
///
/// For FFI contexts (Ruby, Python, WASM), use the two-phase initialization pattern:
/// 1. Call `init_console_only()` during FFI initialization (no runtime required)
/// 2. Call `init_tracing()` after creating the Tokio runtime
///
/// # Example: FFI Two-Phase Initialization
///
/// ```rust
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// // Phase 1: During FFI initialization (Magnus, PyO3, WASM)
/// tasker_shared::logging::init_console_only();
///
/// // Phase 2: After runtime creation
/// let runtime = tokio::runtime::Runtime::new()?;
/// runtime.block_on(async {
///     tasker_shared::logging::init_tracing();
/// });
/// # Ok(())
/// # }
/// ```
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
            // Initialize OpenTelemetry tracer (always when telemetry enabled)
            match init_opentelemetry_tracer(&telemetry_config) {
                Ok(tracer_provider) => {
                    let tracer = tracer_provider.tracer("tasker-core");
                    let telemetry_layer = OpenTelemetryLayer::new(tracer);

                    // Optionally add log forwarding to OTEL (disabled by default - TAS-94)
                    // When disabled, logs go to stdout only, traces go to OTEL
                    if telemetry_config.logs_enabled {
                        match init_opentelemetry_logger(&telemetry_config) {
                            Ok(logger_provider) => {
                                let log_layer = OpenTelemetryTracingBridge::new(&logger_provider);
                                let subscriber = subscriber.with(telemetry_layer).with(log_layer);

                                if subscriber.try_init().is_err() {
                                    tracing::debug!("Global tracing subscriber already initialized - continuing with existing subscriber");
                                } else {
                                    tracing::info!(
                                        environment = %environment,
                                        ansi_colors = use_ansi,
                                        opentelemetry_enabled = true,
                                        otel_logs_enabled = true,
                                        otlp_endpoint = %telemetry_config.otlp_endpoint,
                                        service_name = %telemetry_config.service_name,
                                        "Console logging with OpenTelemetry (traces + logs) initialized"
                                    );
                                }
                            }
                            Err(log_err) => {
                                // Logger failed but tracer succeeded - use traces only
                                let subscriber = subscriber.with(telemetry_layer);
                                if subscriber.try_init().is_err() {
                                    tracing::debug!("Global tracing subscriber already initialized - continuing with existing subscriber");
                                } else {
                                    tracing::warn!(
                                        environment = %environment,
                                        ansi_colors = use_ansi,
                                        opentelemetry_enabled = true,
                                        otel_logs_enabled = false,
                                        error = %log_err,
                                        otlp_endpoint = %telemetry_config.otlp_endpoint,
                                        service_name = %telemetry_config.service_name,
                                        "OpenTelemetry traces enabled, log forwarding failed - logs to stdout only"
                                    );
                                }
                            }
                        }
                    } else {
                        // Traces only mode (default) - logs go to stdout, traces to OTEL
                        let subscriber = subscriber.with(telemetry_layer);
                        if subscriber.try_init().is_err() {
                            tracing::debug!("Global tracing subscriber already initialized - continuing with existing subscriber");
                        } else {
                            tracing::info!(
                                environment = %environment,
                                ansi_colors = use_ansi,
                                opentelemetry_enabled = true,
                                otel_logs_enabled = false,
                                otlp_endpoint = %telemetry_config.otlp_endpoint,
                                service_name = %telemetry_config.service_name,
                                "Console logging with OpenTelemetry traces initialized (logs to stdout only)"
                            );
                        }
                    }
                }
                Err(trace_err) => {
                    // Tracer failed - fall back to console only
                    if subscriber.try_init().is_err() {
                        tracing::debug!("Global tracing subscriber already initialized - continuing with existing subscriber");
                    } else {
                        tracing::warn!(
                            environment = %environment,
                            ansi_colors = use_ansi,
                            error = %trace_err,
                            "Failed to initialize OpenTelemetry tracer - falling back to console-only logging"
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

        // Initialize OpenTelemetry metrics (TAS-29 Phase 3.3)
        metrics::init_metrics();

        // Initialize domain-specific metrics
        metrics::orchestration::init();
        metrics::worker::init();
        metrics::database::init();
        metrics::messaging::init();
    });
}

/// Shutdown OpenTelemetry gracefully
///
/// This should be called before application exit to ensure all pending
/// spans, logs, and metrics are exported.
///
/// # Shutdown Ordering
///
/// Proper shutdown sequence prevents "channel closed" errors:
/// 1. Flush TracerProvider to export pending spans
/// 2. Flush LoggerProvider to export pending logs
/// 3. Shutdown metrics to flush pending metrics
/// 4. Shutdown tracer and logger providers
pub fn shutdown_telemetry() {
    // Flush any pending spans before shutdown
    if let Some(provider) = TRACER_PROVIDER.get() {
        // Force flush to ensure all spans are exported
        // OpenTelemetry 0.31+: force_flush() returns Result<(), TraceError>
        if let Err(e) = provider.force_flush() {
            tracing::warn!("Failed to flush tracer provider: {}", e);
        }
    }

    // Flush any pending logs before shutdown
    if let Some(provider) = LOGGER_PROVIDER.get() {
        // Force flush to ensure all logs are exported
        if let Err(e) = provider.force_flush() {
            tracing::warn!("Failed to flush logger provider: {}", e);
        }
    }

    // Shutdown metrics (will flush any pending metrics)
    metrics::shutdown_metrics();

    // Shutdown tracer provider if it was initialized
    // OpenTelemetry 0.31+: Call shutdown() on the provider instance
    if let Some(provider) = TRACER_PROVIDER.get() {
        if let Err(e) = provider.shutdown() {
            tracing::error!("Failed to shutdown tracer provider: {}", e);
        }
    }

    // Shutdown logger provider if it was initialized
    if let Some(provider) = LOGGER_PROVIDER.get() {
        if let Err(e) = provider.shutdown() {
            tracing::error!("Failed to shutdown logger provider: {}", e);
        }
    }
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
