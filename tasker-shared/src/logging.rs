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

use std::sync::OnceLock;
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};
use chrono::Utc;
use uuid::Uuid;

static TRACING_INITIALIZED: OnceLock<()> = OnceLock::new();

/// Initialize tracing with console output only
/// 
/// This function sets up structured logging that outputs to stdout/stderr,
/// which is appropriate for containerized applications and follows modern
/// observability practices.
pub fn init_tracing() {
    TRACING_INITIALIZED.get_or_init(|| {
        let environment = get_environment();
        let log_level = get_log_level(&environment);
        
        // Determine if we're in a TTY for ANSI color support
        let use_ansi = atty::is(atty::Stream::Stdout);
        
        let subscriber = tracing_subscriber::registry()
            .with(
                fmt::layer()
                    .with_target(true)
                    .with_thread_ids(true)
                    .with_level(true)
                    .with_ansi(use_ansi)
                    .with_filter(EnvFilter::new(log_level))
            );

        // Use try_init to avoid panic if global subscriber already set
        if subscriber.try_init().is_err() {
            // A global subscriber is already set (likely from tests or FFI)
            tracing::debug!("Global tracing subscriber already initialized - continuing with existing subscriber");
        }

        tracing::info!(
            environment = %environment,
            ansi_colors = use_ansi,
            "🔧 TRACING: Console logging initialized"
        );
    });
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
            "📋 TASK_OPERATION: {}", $operation
        );
    };
    // Simple form - just operation
    ($level:ident, $operation:expr $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "📋 TASK_OPERATION: {}", $operation
        );
    };
    // Generic form with additional fields
    ($level:ident, $operation:expr, $($key:ident: $value:expr),+ $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            $($key = ?$value,)*
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "📋 TASK_OPERATION: {}", $operation
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
            "🔄 QUEUE_WORKER: {} (namespace: {})", $operation, $namespace
        );
    };
    // Simple form - just operation
    ($level:ident, $operation:expr $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "🔄 QUEUE_WORKER: {}", $operation
        );
    };
    // Generic form with additional fields
    ($level:ident, $operation:expr, $($key:ident: $value:expr),+ $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            $($key = ?$value,)*
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "🔄 QUEUE_WORKER: {}", $operation
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
            "🚀 ORCHESTRATOR: {}", $operation
        );
    };
    // Generic form with additional fields
    ($level:ident, $operation:expr, $($key:ident: $value:expr),+ $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            $($key = ?$value,)*
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "🚀 ORCHESTRATOR: {}", $operation
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
            "🔧 STEP_OPERATION: {}", $operation
        );
    };
    // Simple form - just operation
    ($level:ident, $operation:expr $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "🔧 STEP_OPERATION: {}", $operation
        );
    };
    // Generic form with additional fields
    ($level:ident, $operation:expr, $($key:ident: $value:expr),+ $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            $($key = ?$value,)*
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "🔧 STEP_OPERATION: {}", $operation
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
            "💾 DATABASE: {}", $operation
        );
    };
    // Generic form with additional fields
    ($level:ident, $operation:expr, $($key:ident: $value:expr),+ $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            $($key = ?$value,)*
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "💾 DATABASE: {}", $operation
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
            "🌉 FFI: {} ({})", $operation, $component
        );
    };
    // Simple form - just operation
    ($level:ident, $operation:expr $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "🌉 FFI: {}", $operation
        );
    };
    // Generic form with additional fields
    ($level:ident, $operation:expr, $($key:ident: $value:expr),+ $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            $($key = ?$value,)*
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "🌉 FFI: {}", $operation
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
            "⚙️ CONFIG: {}", $operation
        );
    };
    // Generic form with additional fields
    ($level:ident, $operation:expr, $($key:ident: $value:expr),+ $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            $($key = ?$value,)*
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "⚙️ CONFIG: {}", $operation
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
            "📚 REGISTRY: {} ({}/{})", $operation, $namespace, $name
        );
    };
    // Simple form - just operation
    ($level:ident, $operation:expr $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "📚 REGISTRY: {}", $operation
        );
    };
    // Generic form with additional fields
    ($level:ident, $operation:expr, $($key:ident: $value:expr),+ $(,)?) => {
        tracing::$level!(
            operation = %$operation,
            $($key = ?$value,)*
            timestamp = %chrono::Utc::now().to_rfc3339(),
            "📚 REGISTRY: {}", $operation
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
        "❌ ERROR: {} failed in {}: {}", operation, component, error
    );
}

/// Example usage of unified logging macros that match Ruby patterns
///
/// These examples show how to use the new macros for consistent cross-language logging:
///
/// ```rust
/// use tasker_shared::{log_task, log_queue_worker, log_orchestrator, log_config};
///
/// // Task operations - matches Ruby: "📋 TASK_OPERATION: Creating task"
/// log_task!(info, "Creating task", task_uuid: Some(123), namespace: "fulfillment");
///
/// // Queue worker operations - matches Ruby: "🔄 QUEUE_WORKER: Processing batch"
/// log_queue_worker!(debug, "Processing batch", namespace: "fulfillment", batch_size: 5);
///
/// // Orchestrator operations - matches Ruby: "🚀 ORCHESTRATOR: Starting system"
/// log_orchestrator!(info, "Starting system", namespaces: vec!["default", "fulfillment"]);
///
/// // Configuration operations - matches Ruby format with ⚙️ emoji
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
