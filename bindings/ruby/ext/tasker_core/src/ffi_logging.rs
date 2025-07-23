//! FFI-specific logging module to debug data flow across the FFI boundary
//! 
//! This module provides structured logging that writes to a file to help debug
//! issues where data appears to be lost when crossing the FFI boundary.

use std::fs::{create_dir_all, OpenOptions};
use std::io::Write;
use std::sync::{Arc, Mutex, OnceLock};
use structured_logger::Builder;
use tracing::{info, debug};

static FFI_LOGGER: OnceLock<Arc<Mutex<Box<dyn Write + Send>>>> = OnceLock::new();

/// Initialize the FFI logger that writes to a log file
pub fn init_ffi_logger() -> Result<(), Box<dyn std::error::Error>> {
    // Get environment from Ruby's ENV or default to development
    let environment = std::env::var("RAILS_ENV")
        .or_else(|_| std::env::var("RACK_ENV"))
        .unwrap_or_else(|_| "development".to_string());

    // Create logs directory if it doesn't exist
    let log_dir = "log";
    create_dir_all(log_dir)?;

    // Create/open log file with append mode
    let log_path = format!("{}/tasker_ffi_{}.log", log_dir, environment);
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)?;

    // Wrap the file writer in Arc<Mutex<>> for thread safety
    let writer: Box<dyn Write + Send> = Box::new(file);
    let shared_writer = Arc::new(Mutex::new(writer));
    
    // Store a clone for direct writing
    let writer_clone = shared_writer.clone();
    FFI_LOGGER.set(writer_clone).ok();

    // Initialize the structured logger - using simple initialization
    // Ignore errors if logger is already initialized
    let _ = Builder::with_level("debug")
        .try_init();

    info!("FFI Logger initialized - writing to {}", log_path);
    Ok(())
}

/// Log a debug message directly to the FFI log file
pub fn log_ffi_debug(component: &str, message: &str) {
    debug!(target: "ffi", component = component, "{}", message);
    
    // Also write directly to ensure it's captured
    if let Some(logger) = FFI_LOGGER.get() {
        if let Ok(mut writer) = logger.lock() {
            let timestamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f");
            let _ = writeln!(writer, "[{}] [DEBUG] [{}] {}", timestamp, component, message);
            let _ = writer.flush();
        }
    }
}

/// Log task initialization data for debugging
pub fn log_task_init_data(stage: &str, data: &serde_json::Value) {
    let message = format!("Task initialization - {}: {}", stage, serde_json::to_string_pretty(data).unwrap_or_else(|_| "SERIALIZATION_ERROR".to_string()));
    log_ffi_debug("initialize_task", &message);
}

/// Log the result data before returning to Ruby
pub fn log_task_init_result(result: &crate::types::TaskHandlerInitializeResult) {
    let data = serde_json::json!({
        "task_id": result.task_id,
        "step_count": result.step_count,
        "step_mapping": result.step_mapping,
        "handler_config_name": result.handler_config_name,
    });
    log_task_init_data("RESULT", &data);
}

/// Log Magnus value inspection
pub fn log_magnus_value(stage: &str, value: magnus::Value) {
    use magnus::value::ReprValue;
    
    let inspect_result: Result<String, _> = value.funcall("inspect", ());
    let class_name: Result<String, _> = value.funcall("class", ()).and_then(|c: magnus::Value| c.funcall("name", ()));
    
    let message = format!(
        "Magnus value at {}: class={}, inspect={}", 
        stage,
        class_name.unwrap_or_else(|_| "UNKNOWN".to_string()),
        inspect_result.unwrap_or_else(|_| "INSPECT_FAILED".to_string())
    );
    
    log_ffi_debug("magnus_inspection", &message);
}

/// Log raw FFI boundary crossing
pub fn log_ffi_boundary(direction: &str, component: &str, data: &str) {
    let message = format!("FFI {} - {}: {}", direction, component, data);
    log_ffi_debug("ffi_boundary", &message);
}