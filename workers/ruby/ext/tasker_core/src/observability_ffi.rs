//! # Observability FFI Module
//!
//! TAS-77: Exposes worker observability services (Health, Metrics, Templates, Config)
//! via Ruby FFI, enabling applications to access the same data available through
//! the HTTP API without running a web server.
//!
//! ## Design Approach
//!
//! Complex response types are serialized to JSON strings that Ruby can parse with
//! `JSON.parse`. This provides a clean interface and avoids manual hash building
//! for deeply nested structures.
//!
//! ## Usage
//!
//! ```ruby
//! # Get health status
//! health = JSON.parse(TaskerCore::FFI.health_basic)
//! health = JSON.parse(TaskerCore::FFI.health_detailed)
//! ready = JSON.parse(TaskerCore::FFI.health_ready)
//! live = JSON.parse(TaskerCore::FFI.health_live)
//!
//! # Get metrics
//! metrics = JSON.parse(TaskerCore::FFI.metrics_worker)
//! events = JSON.parse(TaskerCore::FFI.metrics_events)
//! prometheus = TaskerCore::FFI.metrics_prometheus  # Already text format
//!
//! # Query templates
//! templates = JSON.parse(TaskerCore::FFI.templates_list(true))
//! template = JSON.parse(TaskerCore::FFI.template_get("namespace", "name", "version"))
//! validation = JSON.parse(TaskerCore::FFI.template_validate("namespace", "name", "version"))
//!
//! # Query configuration
//! config = JSON.parse(TaskerCore::FFI.config_runtime)
//! env = TaskerCore::FFI.config_environment  # Simple string
//! ```

use crate::bridge::WORKER_SYSTEM;
use magnus::{function, prelude::*, Error, ExceptionClass, RModule, Ruby};
use tracing::{debug, error};

/// Helper to get RuntimeError exception class
fn runtime_error_class() -> ExceptionClass {
    Ruby::get()
        .expect("Ruby runtime should be available")
        .exception_runtime_error()
}

/// Helper to get web_state from worker system, returning error if unavailable
fn with_web_state<F, T>(f: F) -> Result<T, Error>
where
    F: FnOnce(
        &crate::bridge::RubyBridgeHandle,
        &tasker_worker::web::WorkerWebState,
    ) -> Result<T, Error>,
{
    let handle_guard = WORKER_SYSTEM.lock().map_err(|e| {
        error!("Failed to acquire worker system lock: {}", e);
        Error::new(runtime_error_class(), "Lock acquisition failed")
    })?;

    let handle = handle_guard
        .as_ref()
        .ok_or_else(|| Error::new(runtime_error_class(), "Worker system not running"))?;

    let web_state = handle.system_handle.web_state.as_ref().ok_or_else(|| {
        Error::new(
            runtime_error_class(),
            "Web state not available (web API may be disabled)",
        )
    })?;

    f(handle, web_state)
}

/// Helper to serialize a value to JSON, with error conversion
fn to_json<T: serde::Serialize>(value: &T) -> Result<String, Error> {
    serde_json::to_string(value).map_err(|e| {
        Error::new(
            runtime_error_class(),
            format!("Failed to serialize response: {}", e),
        )
    })
}

// =============================================================================
// Health Service FFI Functions
// =============================================================================

/// Get basic health status as JSON string
///
/// Returns JSON with:
/// - `status`: "healthy" or "unhealthy"
/// - `worker_id`: Worker identifier
/// - `timestamp`: ISO-8601 timestamp
pub fn health_basic() -> Result<String, Error> {
    debug!("FFI: health_basic called");

    with_web_state(|_handle, web_state| {
        let response = web_state.health_service().basic_health();
        to_json(&response)
    })
}

/// Get liveness status as JSON (Kubernetes liveness probe equivalent)
pub fn health_live() -> Result<String, Error> {
    debug!("FFI: health_live called");

    with_web_state(|_handle, web_state| {
        let response = web_state.health_service().liveness();
        to_json(&response)
    })
}

/// Get readiness status as JSON (Kubernetes readiness probe equivalent)
///
/// Returns JSON with detailed health checks.
/// Check the `status` field to determine if the worker is ready.
pub fn health_ready() -> Result<String, Error> {
    debug!("FFI: health_ready called");

    with_web_state(|handle, web_state| {
        let response = handle
            .runtime
            .block_on(async { web_state.health_service().readiness().await });

        // Return the response regardless of Ok/Err - both contain DetailedHealthResponse
        match response {
            Ok(detailed) => to_json(&detailed),
            Err(detailed) => to_json(&detailed),
        }
    })
}

/// Get detailed health information as JSON
pub fn health_detailed() -> Result<String, Error> {
    debug!("FFI: health_detailed called");

    with_web_state(|handle, web_state| {
        let response = handle
            .runtime
            .block_on(async { web_state.health_service().detailed_health().await });

        to_json(&response)
    })
}

// =============================================================================
// Metrics Service FFI Functions
// =============================================================================

/// Get worker metrics as JSON string
pub fn metrics_worker() -> Result<String, Error> {
    debug!("FFI: metrics_worker called");

    with_web_state(|handle, web_state| {
        let response = handle
            .runtime
            .block_on(async { web_state.metrics_service().worker_metrics().await });

        to_json(&response)
    })
}

/// Get domain event statistics as JSON
pub fn metrics_events() -> Result<String, Error> {
    debug!("FFI: metrics_events called");

    with_web_state(|handle, web_state| {
        let response = handle
            .runtime
            .block_on(async { web_state.metrics_service().domain_event_stats().await });

        to_json(&response)
    })
}

/// Get Prometheus-formatted metrics string
///
/// This returns the raw Prometheus text format, not JSON.
pub fn metrics_prometheus() -> Result<String, Error> {
    debug!("FFI: metrics_prometheus called");

    with_web_state(|handle, web_state| {
        let response = handle
            .runtime
            .block_on(async { web_state.metrics_service().prometheus_format().await });

        Ok(response)
    })
}

// =============================================================================
// Template Query Service FFI Functions
// =============================================================================

/// List all available templates as JSON
///
/// # Arguments
/// * `include_cache_stats` - Whether to include cache statistics
pub fn templates_list(include_cache_stats: bool) -> Result<String, Error> {
    debug!("FFI: templates_list called");

    with_web_state(|handle, web_state| {
        let response = handle.runtime.block_on(async {
            web_state
                .template_query_service()
                .list_templates(include_cache_stats)
                .await
        });

        to_json(&response)
    })
}

/// Get a specific template by namespace/name/version as JSON
pub fn template_get(namespace: String, name: String, version: String) -> Result<String, Error> {
    debug!(
        "FFI: template_get called for {}/{}/{}",
        namespace, name, version
    );

    with_web_state(|handle, web_state| {
        let response = handle.runtime.block_on(async {
            web_state
                .template_query_service()
                .get_template(&namespace, &name, &version)
                .await
        });

        match response {
            Ok(template) => to_json(&template),
            Err(e) => Err(Error::new(runtime_error_class(), e.to_string())),
        }
    })
}

/// Validate a template for worker execution as JSON
pub fn template_validate(
    namespace: String,
    name: String,
    version: String,
) -> Result<String, Error> {
    debug!(
        "FFI: template_validate called for {}/{}/{}",
        namespace, name, version
    );

    with_web_state(|handle, web_state| {
        let response = handle.runtime.block_on(async {
            web_state
                .template_query_service()
                .validate_template(&namespace, &name, &version)
                .await
        });

        match response {
            Ok(validation) => to_json(&validation),
            Err(e) => Err(Error::new(runtime_error_class(), e.to_string())),
        }
    })
}

/// Get template cache statistics as JSON
pub fn templates_cache_stats() -> Result<String, Error> {
    debug!("FFI: templates_cache_stats called");

    with_web_state(|handle, web_state| {
        let stats = handle
            .runtime
            .block_on(async { web_state.template_query_service().cache_stats().await });

        to_json(&stats)
    })
}

/// Clear the template cache, returns JSON with operation result
pub fn templates_cache_clear() -> Result<String, Error> {
    debug!("FFI: templates_cache_clear called");

    with_web_state(|handle, web_state| {
        let response = handle
            .runtime
            .block_on(async { web_state.template_query_service().clear_cache().await });

        to_json(&response)
    })
}

/// Refresh a specific template in the cache, returns JSON with operation result
pub fn template_refresh(namespace: String, name: String, version: String) -> Result<String, Error> {
    debug!(
        "FFI: template_refresh called for {}/{}/{}",
        namespace, name, version
    );

    with_web_state(|handle, web_state| {
        let response = handle.runtime.block_on(async {
            web_state
                .template_query_service()
                .refresh_template(&namespace, &name, &version)
                .await
        });

        match response {
            Ok(op) => to_json(&op),
            Err(e) => Err(Error::new(runtime_error_class(), e.to_string())),
        }
    })
}

// =============================================================================
// Config Query Service FFI Functions
// =============================================================================

/// Get runtime configuration as JSON (safe fields only, no secrets)
pub fn config_runtime() -> Result<String, Error> {
    debug!("FFI: config_runtime called");

    with_web_state(|_handle, web_state| {
        let response = web_state.config_query_service().runtime_config();

        match response {
            Ok(config) => to_json(&config),
            Err(e) => Err(Error::new(runtime_error_class(), e.to_string())),
        }
    })
}

/// Get the current environment name (simple string, not JSON)
pub fn config_environment() -> Result<String, Error> {
    debug!("FFI: config_environment called");

    with_web_state(|_handle, web_state| {
        Ok(web_state.config_query_service().environment().to_string())
    })
}

// =============================================================================
// Module Initialization
// =============================================================================

/// Initialize the observability FFI module
pub fn init_observability_ffi(module: &RModule) -> Result<(), Error> {
    tracing::info!("🔍 Initializing observability FFI module (TAS-77)");

    // Health endpoints
    module.define_singleton_method("health_basic", function!(health_basic, 0))?;
    module.define_singleton_method("health_live", function!(health_live, 0))?;
    module.define_singleton_method("health_ready", function!(health_ready, 0))?;
    module.define_singleton_method("health_detailed", function!(health_detailed, 0))?;

    // Metrics endpoints
    module.define_singleton_method("metrics_worker", function!(metrics_worker, 0))?;
    module.define_singleton_method("metrics_events", function!(metrics_events, 0))?;
    module.define_singleton_method("metrics_prometheus", function!(metrics_prometheus, 0))?;

    // Template endpoints
    module.define_singleton_method("templates_list", function!(templates_list, 1))?;
    module.define_singleton_method("template_get", function!(template_get, 3))?;
    module.define_singleton_method("template_validate", function!(template_validate, 3))?;
    module.define_singleton_method("templates_cache_stats", function!(templates_cache_stats, 0))?;
    module.define_singleton_method("templates_cache_clear", function!(templates_cache_clear, 0))?;
    module.define_singleton_method("template_refresh", function!(template_refresh, 3))?;

    // Config endpoints
    module.define_singleton_method("config_runtime", function!(config_runtime, 0))?;
    module.define_singleton_method("config_environment", function!(config_environment, 0))?;

    tracing::info!("✅ Observability FFI module initialized");
    Ok(())
}
