//! Unified Logging Demo
//!
//! This example demonstrates how Rust and Ruby now use the same logging format
//! for cross-language consistency in the TaskerCore system.

use tasker_core::{
    log_config, log_database, log_ffi, log_orchestrator, log_queue_worker, log_registry, log_step,
    log_task, logging::init_structured_logging,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize structured logging
    init_structured_logging();

    println!("🎯 Unified Logging Demo - Rust Side");
    println!("===================================\n");

    // Task operations - matches Ruby: logger.log_task(:info, "Creating task", task_id: 123)
    log_task!(
        info,
        "Creating new order fulfillment task",
        task_id: Some(123),
        namespace: "fulfillment",
        priority: "high"
    );

    log_task!(warn, "Task validation warning", task_id: Some(123), validation_errors: vec!["missing customer_id"]);

    // Queue worker operations - matches Ruby: logger.log_queue_worker(:debug, "Processing batch", namespace: "fulfillment")
    log_queue_worker!(
        info,
        "Starting worker",
        namespace: "fulfillment",
        batch_size: 5,
        poll_interval_ms: 250
    );

    log_queue_worker!(
        debug,
        "Processing message batch",
        namespace: "fulfillment",
        message_count: 3,
        processing_time_ms: 45
    );

    // Orchestrator operations - matches Ruby: logger.log_orchestrator(:info, "System startup", active_namespaces: ["fulfillment"])
    log_orchestrator!(
        info,
        "Embedded orchestrator starting",
        namespaces: vec!["fulfillment", "inventory", "notifications"],
        mode: "embedded"
    );

    // Step operations - matches Ruby: logger.log_step(:debug, "Step completed", step_id: 789, task_id: 123)
    log_step!(
        info,
        "Executing payment processing step",
        step_id: Some(789),
        task_id: Some(123),
        step_name: "process_payment",
        amount: "$99.99"
    );

    log_step!(debug, "Step validation passed", step_name: "validate_inventory", validation_time_ms: 12);

    // Database operations - matches Ruby: logger.log_database(:warn, "Slow query detected", table: "tasks")
    log_database!(
        warn,
        "Slow query detected",
        table: Some("tasker_tasks"),
        duration_ms: Some(3500),
        query_type: "SELECT"
    );

    log_database!(info, "Migration completed", affected_rows: 1250, migration_version: "20250808001");

    // FFI operations - matches Ruby: logger.log_ffi(:info, "Ruby bridge call", component: "step_handler")
    log_ffi!(
        info,
        "Calling Ruby step handler",
        component: "step_handler_bridge",
        method: "execute_step",
        duration_ms: 45
    );

    log_ffi!(debug, "FFI data exchange", component: "orchestration_bridge", data_size_bytes: 1024);

    // Configuration operations - matches Ruby: logger.log_config(:info, "Config loaded", environment: "production")
    log_config!(
        info,
        "Configuration reloaded",
        environment: "development",
        config_file: "tasker-config.yaml",
        validation_passed: true
    );

    log_config!(warn, "Using fallback configuration", reason: "Primary config file not found");

    // Registry operations - matches Ruby: logger.log_registry(:debug, "Handler registered", namespace: "fulfillment", name: "order_processor")
    log_registry!(
        info,
        "Task template registered",
        namespace: "fulfillment",
        name: "order_processing",
        version: "1.0.0"
    );

    log_registry!(debug, "Handler resolution completed", resolution_time_ms: 8, cache_hit: true);

    println!("\n✅ Demo completed! Check the logs above - they match the Ruby format:");
    println!("   - Same emoji prefixes (📋, 🔄, 🚀, 🔧, 💾, 🌉, ⚙️, 📚)");
    println!("   - Same component names (TASK_OPERATION, QUEUE_WORKER, etc.)");
    println!("   - Structured data in JSON format");
    println!("   - ISO8601 timestamps");
    println!("\nRuby equivalent calls:");
    println!("   logger.log_task(:info, 'Creating new order fulfillment task', task_id: 123, namespace: 'fulfillment')");
    println!("   logger.log_queue_worker(:info, 'Starting worker', namespace: 'fulfillment', batch_size: 5)");
    println!("   logger.log_orchestrator(:info, 'Embedded orchestrator starting', namespaces: ['fulfillment'])");

    Ok(())
}
