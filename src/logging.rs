//! # Structured Logging Module
//!
//! Environment-aware structured logging that outputs to both console and files
//! for debugging complex async workflows and FFI operations.

use std::fs;
use std::path::PathBuf;
use std::process;
use std::sync::OnceLock;
use chrono::Utc;
use tracing_subscriber::{
    fmt,
    layer::SubscriberExt,
    util::SubscriberInitExt,
    Layer,
    EnvFilter,
};

static LOGGER_INITIALIZED: OnceLock<()> = OnceLock::new();

/// Initialize structured logging with environment-specific configuration
pub fn init_structured_logging() {
    LOGGER_INITIALIZED.get_or_init(|| {
        let environment = get_environment();
        let log_level = get_log_level(&environment);
        
        // Create log directory if it doesn't exist
        let log_dir = PathBuf::from("log");
        if !log_dir.exists() {
            fs::create_dir_all(&log_dir).expect("Failed to create log directory");
        }

        // Generate log file name with environment, PID, and timestamp
        let pid = process::id();
        let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
        let log_filename = format!("{}.{}.{}.log", environment, pid, timestamp);
        let log_path = log_dir.join(log_filename);

        // Initialize tracing with both console and file output
        let file_appender = tracing_appender::rolling::never(&log_dir, format!("{}.{}.{}.log", environment, pid, timestamp));
        let (file_writer, _guard) = tracing_appender::non_blocking(file_appender);

        // Try to initialize tracing subscriber, but don't panic if one already exists
        let subscriber = tracing_subscriber::registry()
            .with(
                fmt::layer()
                    .with_target(true)
                    .with_thread_ids(true)
                    .with_level(true)
                    .with_ansi(true)
                    .with_filter(EnvFilter::new(log_level.clone()))
            )
            .with(
                fmt::layer()
                    .with_writer(file_writer)
                    .with_target(true)
                    .with_thread_ids(true)
                    .with_level(true)
                    .with_ansi(false)
                    .json()
                    .with_filter(EnvFilter::new(log_level))
            );
            
        // Use try_init to avoid panic if global subscriber already set
        if let Err(_) = subscriber.try_init() {
            // A global subscriber is already set (likely from FFI bindings)
            // This is not an error - continue normally
            tracing::debug!("Global tracing subscriber already initialized - continuing with existing subscriber");
        }

        tracing::info!(
            pid = pid,
            environment = %environment,
            log_file = %log_path.display(),
            "🔧 STRUCTURED LOGGING: Initialized with file output"
        );

        // Store the guard to prevent it from being dropped
        std::mem::forget(_guard);
    });
}

/// Get current environment from environment variables
fn get_environment() -> String {
    std::env::var("TASKER_ENV")
        .or_else(|_| std::env::var("RAILS_ENV"))
        .or_else(|_| std::env::var("RACK_ENV"))
        .or_else(|_| std::env::var("APP_ENV"))
        .unwrap_or_else(|_| "development".to_string())
}

/// Get log level based on environment
fn get_log_level(environment: &str) -> String {
    match environment {
        "test" => "debug".to_string(),
        "development" => "debug".to_string(),
        "production" => "info".to_string(),
        _ => "debug".to_string(),
    }
}

/// Log structured data for task operations
pub fn log_task_operation(
    operation: &str,
    task_id: Option<i64>,
    task_name: Option<&str>,
    namespace: Option<&str>,
    status: &str,
    details: Option<&str>,
) {
    tracing::info!(
        operation = %operation,
        task_id = task_id,
        task_name = task_name,
        namespace = namespace,
        status = %status,
        details = details,
        timestamp = %Utc::now().to_rfc3339(),
        "📋 TASK_OPERATION"
    );
}

/// Log structured data for step operations
pub fn log_step_operation(
    operation: &str,
    task_id: Option<i64>,
    step_id: Option<i64>,
    step_name: Option<&str>,
    status: &str,
    details: Option<&str>,
) {
    tracing::info!(
        operation = %operation,
        task_id = task_id,
        step_id = step_id,
        step_name = step_name,
        status = %status,
        details = details,
        timestamp = %Utc::now().to_rfc3339(),
        "🔧 STEP_OPERATION"
    );
}

/// Log structured data for FFI operations
pub fn log_ffi_operation(
    operation: &str,
    component: &str,
    status: &str,
    details: Option<&str>,
    data: Option<&str>,
) {
    tracing::info!(
        operation = %operation,
        component = %component,
        status = %status,
        details = details,
        data = data,
        timestamp = %Utc::now().to_rfc3339(),
        "🌉 FFI_OPERATION"
    );
}

/// Log structured data for registry operations
pub fn log_registry_operation(
    operation: &str,
    namespace: Option<&str>,
    name: Option<&str>,
    version: Option<&str>,
    status: &str,
    details: Option<&str>,
) {
    tracing::info!(
        operation = %operation,
        namespace = namespace,
        name = name,
        version = version,
        status = %status,
        details = details,
        timestamp = %Utc::now().to_rfc3339(),
        "📚 REGISTRY_OPERATION"
    );
}

/// Log structured data for database operations
pub fn log_database_operation(
    operation: &str,
    table: Option<&str>,
    record_id: Option<i64>,
    status: &str,
    duration_ms: Option<u64>,
    details: Option<&str>,
) {
    tracing::info!(
        operation = %operation,
        table = table,
        record_id = record_id,
        status = %status,
        duration_ms = duration_ms,
        details = details,
        timestamp = %Utc::now().to_rfc3339(),
        "💾 DATABASE_OPERATION"
    );
}

/// Log error with full context
pub fn log_error(
    component: &str,
    operation: &str,
    error: &str,
    context: Option<&str>,
) {
    tracing::error!(
        component = %component,
        operation = %operation,
        error = %error,
        context = context,
        timestamp = %Utc::now().to_rfc3339(),
        "❌ ERROR"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_environment_detection() {
        std::env::set_var("TASKER_ENV", "test_override");
        let env = get_environment();
        assert_eq!(env, "test_override");
        std::env::remove_var("TASKER_ENV");
    }

    #[test]
    fn test_log_level_mapping() {
        assert_eq!(get_log_level("test"), "debug");
        assert_eq!(get_log_level("development"), "debug");
        assert_eq!(get_log_level("production"), "info");
        assert_eq!(get_log_level("unknown"), "debug");
    }
}