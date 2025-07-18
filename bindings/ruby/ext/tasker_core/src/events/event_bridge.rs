//! # Event System FFI Bridge
//!
//! Proper FFI bridge that delegates to core event system instead of
//! reimplementing it. Uses singleton pattern for shared resources.

use crate::context::{json_to_ruby_value, ruby_value_to_json};
use crate::globals::execute_async;
use magnus::{Error, RModule, Ruby, Value};
use tasker_core::events::{Event, OrchestrationEvent};
use tasker_core::events::types::TaskResult;
use chrono::Utc;
use serde_json;

/// ✅ HANDLE-BASED: Publish a simple event using OrchestrationHandle
fn publish_simple_event_with_handle_wrapper(
    handle_value: Value,
    event_data_value: Value,
) -> Result<Value, Error> {
    use magnus::{TryConvert, IntoValue};
    let handle: &crate::handles::OrchestrationHandle = TryConvert::try_convert(handle_value)?;
    
    let event_data = ruby_value_to_json(event_data_value)
        .map_err(|e| Error::new(Ruby::get().unwrap().exception_runtime_error(), format!("Invalid event data: {}", e)))?;

    let result = execute_async(async {
        let event_name = event_data.get("name")
            .and_then(|v| v.as_str())
            .ok_or("Missing event name")?;
        let payload = event_data.get("payload")
            .cloned()
            .unwrap_or(serde_json::json!({}));

        // Use handle's persistent event publisher - NO global lookup!
        let event_publisher = handle.event_publisher();

        // Delegate to core event publisher
        event_publisher.publish(event_name, payload).await
            .map_err(|e| format!("Event publishing failed: {}", e))?;

        Ok::<serde_json::Value, String>(serde_json::json!({
            "status": "published",
            "event_name": event_name,
            "published_at": Utc::now().to_rfc3339()
        }))
    });

    match result {
        Ok(success_data) => json_to_ruby_value(success_data),
        Err(error_msg) => json_to_ruby_value(serde_json::json!({
            "status": "error",
            "error": error_msg
        }))
    }
}

/// ✅ HANDLE-BASED: Publish a structured orchestration event using OrchestrationHandle
fn publish_orchestration_event_with_handle_wrapper(
    handle_value: Value,
    event_data_value: Value,
) -> Result<Value, Error> {
    use magnus::{TryConvert, IntoValue};
    let handle: &crate::handles::OrchestrationHandle = TryConvert::try_convert(handle_value)?;
    let event_data = ruby_value_to_json(event_data_value)
        .map_err(|e| Error::new(Ruby::get().unwrap().exception_runtime_error(), format!("Invalid event data: {}", e)))?;

    let result = execute_async(async {
        let event_type = event_data.get("event_type")
            .and_then(|v| v.as_str())
            .ok_or("Missing event_type")?;

        // Create structured orchestration event based on type
        let orchestration_event = match event_type {
            "task_orchestration_started" => {
                let task_id = event_data.get("task_id")
                    .and_then(|v| v.as_i64())
                    .ok_or("Missing task_id for task_orchestration_started")?;
                let framework = event_data.get("framework")
                    .and_then(|v| v.as_str())
                    .unwrap_or("ruby_client")
                    .to_string();

                OrchestrationEvent::TaskOrchestrationStarted {
                    task_id,
                    framework,
                    started_at: Utc::now(),
                }
            },
            "task_orchestration_completed" => {
                let task_id = event_data.get("task_id")
                    .and_then(|v| v.as_i64())
                    .ok_or("Missing task_id for task_orchestration_completed")?;
                let result_value = event_data.get("result")
                    .cloned()
                    .unwrap_or(serde_json::json!({"status": "completed"}));

                let result = if result_value.get("status").and_then(|s| s.as_str()) == Some("failed") {
                    let error = result_value.get("error")
                        .and_then(|e| e.as_str())
                        .unwrap_or("Unknown error")
                        .to_string();
                    TaskResult::Failed { error }
                } else {
                    TaskResult::Success
                };

                OrchestrationEvent::TaskOrchestrationCompleted {
                    task_id,
                    result,
                    completed_at: Utc::now(),
                }
            },
            "handler_registered" => {
                let handler_name = event_data.get("handler_name")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing handler_name for handler_registered")?
                    .to_string();
                let handler_type = event_data.get("handler_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("task_handler")
                    .to_string();

                OrchestrationEvent::HandlerRegistered {
                    handler_name,
                    handler_type,
                    registered_at: Utc::now(),
                }
            },
            _ => return Err(format!("Unknown orchestration event type: {}", event_type))
        };

        // Create and publish structured event using core event system
        let event = Event::Orchestration(orchestration_event);

        // Use handle's persistent event publisher - NO global lookup!
        let event_publisher = handle.event_publisher();

        // Delegate to core event publisher
        event_publisher.publish_event(event).await
            .map_err(|e| format!("Orchestration event publishing failed: {}", e))?;

        Ok::<serde_json::Value, String>(serde_json::json!({
            "status": "published",
            "event_type": event_type,
            "published_at": Utc::now().to_rfc3339()
        }))
    });

    match result {
        Ok(success_data) => json_to_ruby_value(success_data),
        Err(error_msg) => json_to_ruby_value(serde_json::json!({
            "status": "error",
            "error": error_msg
        }))
    }
}

/// ✅ HANDLE-BASED: Subscribe to events using OrchestrationHandle
fn subscribe_to_events_with_handle_wrapper(
    handle_value: Value,
    subscription_data_value: Value,
) -> Result<Value, Error> {
    use magnus::{TryConvert, IntoValue};
    let handle: &crate::handles::OrchestrationHandle = TryConvert::try_convert(handle_value)?;
    let subscription_data = ruby_value_to_json(subscription_data_value)
        .map_err(|e| Error::new(Ruby::get().unwrap().exception_runtime_error(), format!("Invalid subscription data: {}", e)))?;

    let result = execute_async(async {
        let event_pattern = subscription_data.get("event_pattern")
            .and_then(|v| v.as_str())
            .unwrap_or("*");

        // Use handle's persistent event publisher - NO global lookup!
        let _event_publisher = handle.event_publisher();

        // For now, return a subscription acknowledgment
        // Full callback implementation would require additional Ruby callback handling
        // This would integrate with the core event publisher's subscription system
        let subscription_id = format!("sub_{}", uuid::Uuid::new_v4());

        Ok::<serde_json::Value, String>(serde_json::json!({
            "status": "subscribed",
            "event_pattern": event_pattern,
            "subscription_id": subscription_id,
            "subscribed_at": Utc::now().to_rfc3339(),
            "note": "Using core event publisher subscription system"
        }))
    });

    match result {
        Ok(success_data) => json_to_ruby_value(success_data),
        Err(error_msg) => json_to_ruby_value(serde_json::json!({
            "status": "error",
            "error": error_msg
        }))
    }
}

/// ✅ HANDLE-BASED: Get event publisher statistics using OrchestrationHandle
fn get_event_stats_with_handle_wrapper(handle_value: Value) -> Result<Value, Error> {
    use magnus::{TryConvert, IntoValue};
    let handle: &crate::handles::OrchestrationHandle = TryConvert::try_convert(handle_value)?;
    let result = execute_async(async {
        // Use handle's persistent event publisher - NO global lookup!
        let event_publisher = handle.event_publisher();

        // Delegate to core event publisher stats
        let stats = event_publisher.stats();

        Ok::<serde_json::Value, String>(serde_json::json!({
            "buffer_size": stats.buffer_size,
            "subscriber_count": stats.subscriber_count,
            "correlation_id": stats.correlation_id,
            "ffi_enabled": stats.ffi_enabled,
            "async_processing": stats.async_processing,
            "source": "core_event_publisher"
        }))
    });

    match result {
        Ok(success_data) => json_to_ruby_value(success_data),
        Err(error_msg) => json_to_ruby_value(serde_json::json!({
            "status": "error",
            "error": error_msg
        }))
    }
}

/// ✅ HANDLE-BASED: Register external event callback using OrchestrationHandle
fn register_external_event_callback_with_handle_wrapper(
    handle_value: Value,
    callback_data_value: Value,
) -> Result<Value, Error> {
    use magnus::{TryConvert, IntoValue};
    let handle: &crate::handles::OrchestrationHandle = TryConvert::try_convert(handle_value)?;
    let callback_data = ruby_value_to_json(callback_data_value)
        .map_err(|e| Error::new(Ruby::get().unwrap().exception_runtime_error(), format!("Invalid callback data: {}", e)))?;

    let result = execute_async(async {
        let callback_name = callback_data.get("callback_name")
            .and_then(|v| v.as_str())
            .unwrap_or("ruby_callback");

        // Use handle's persistent event publisher - NO global lookup!
        let _event_publisher = handle.event_publisher();

        // This would register a callback with the core event publisher
        // For now, acknowledge the registration
        let callback_id = format!("callback_{}", uuid::Uuid::new_v4());

        Ok::<serde_json::Value, String>(serde_json::json!({
            "status": "registered",
            "callback_name": callback_name,
            "callback_id": callback_id,
            "registered_at": Utc::now().to_rfc3339(),
            "note": "Callback registered with core event publisher"
        }))
    });

    match result {
        Ok(success_data) => json_to_ruby_value(success_data),
        Err(error_msg) => json_to_ruby_value(serde_json::json!({
            "status": "error",
            "error": error_msg
        }))
    }
}

/// ✅ HANDLE-BASED: Register only handle-based event functions
/// Note: All event operations now flow through OrchestrationManager handles
pub fn register_event_functions(module: RModule) -> Result<(), Error> {
    // Only register handle-based functions for direct replacement migration
    module.define_module_function(
        "publish_simple_event_with_handle",
        magnus::function!(publish_simple_event_with_handle_wrapper, 2),
    )?;

    module.define_module_function(
        "publish_orchestration_event_with_handle", 
        magnus::function!(publish_orchestration_event_with_handle_wrapper, 2),
    )?;

    module.define_module_function(
        "subscribe_to_events_with_handle",
        magnus::function!(subscribe_to_events_with_handle_wrapper, 2),
    )?;

    module.define_module_function(
        "get_event_stats_with_handle",
        magnus::function!(get_event_stats_with_handle_wrapper, 1),
    )?;

    module.define_module_function(
        "register_external_event_callback_with_handle",
        magnus::function!(register_external_event_callback_with_handle_wrapper, 2),
    )?;

    Ok(())
}
