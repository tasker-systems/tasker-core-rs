//! TAS-93 Phase 5: Multi-Method Handlers for Resolver Chain E2E Testing.
//!
//! This module demonstrates the method dispatch feature of the resolver chain.
//! For Rust handlers, method dispatch is handled by looking up the method name
//! from the step definition and routing internally.
//!
//! ## Method Dispatch in Rust
//!
//! Unlike dynamic languages, Rust handlers receive the method name in the step
//! definition and dispatch internally. The handler checks `step_definition.handler.method`
//! and calls the appropriate implementation.
//!
//! ## Example YAML configuration:
//!
//! ```yaml
//! handler:
//!   callable: resolver_tests_multi_method
//!   method: validate  # Invokes validate logic instead of default call
//! ```

use super::{error_result, success_result, RustStepHandler, StepHandlerConfig};
use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use std::collections::HashMap;
use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::TaskSequenceStep;
use tracing::info;

/// Multi-method handler demonstrating method dispatch.
///
/// Available methods (dispatched via `step_definition.handler.method`):
/// - call: Default entry point (standard processing)
/// - validate: Validation-only path
/// - process: Processing-specific path
/// - refund: Refund-specific path (payment domain example)
///
/// Each method returns a result with `invoked_method` so tests can verify
/// which method was actually called.
#[derive(Debug)]
pub struct MultiMethodHandler {
    #[expect(dead_code, reason = "config available for future use")]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for MultiMethodHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_name = step_data.workflow_step.name.clone();

        // Get the method from step definition (defaults to "call")
        let method = step_data
            .step_definition
            .handler
            .method
            .as_deref()
            .unwrap_or("call");

        info!(
            "MultiMethodHandler dispatching to method: {} for step: {}",
            method, step_name
        );

        // Dispatch to appropriate method based on step definition
        match method {
            "validate" => self.handle_validate(step_data, start_time).await,
            "process" => self.handle_process(step_data, start_time).await,
            "refund" => self.handle_refund(step_data, start_time).await,
            "call" | _ => self.handle_call(step_data, start_time).await,
        }
    }

    fn name(&self) -> &str {
        "resolver_tests_multi_method"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

impl MultiMethodHandler {
    /// Default entry point - standard processing.
    async fn handle_call(
        &self,
        step_data: &TaskSequenceStep,
        start_time: std::time::Instant,
    ) -> Result<StepExecutionResult> {
        let step_uuid = step_data.workflow_step.workflow_step_uuid;
        let step_name = &step_data.workflow_step.name;
        let context = step_data.task.task.context.clone().unwrap_or(json!({}));
        let input = context.get("data").cloned().unwrap_or(json!({}));

        let mut metadata = HashMap::new();
        metadata.insert("handler".to_string(), json!("MultiMethodHandler"));
        metadata.insert("message".to_string(), json!("Default call method invoked"));

        Ok(success_result(
            step_uuid,
            json!({
                "invoked_method": "call",
                "handler": "MultiMethodHandler",
                "message": "Default call method invoked",
                "input_received": input,
                "step_name": step_name
            }),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    /// Validation-only entry point.
    async fn handle_validate(
        &self,
        step_data: &TaskSequenceStep,
        start_time: std::time::Instant,
    ) -> Result<StepExecutionResult> {
        let step_uuid = step_data.workflow_step.workflow_step_uuid;
        let step_name = &step_data.workflow_step.name;
        let context = step_data.task.task.context.clone().unwrap_or(json!({}));
        let input = context.get("data").cloned().unwrap_or(json!({}));

        // Simple validation logic for testing
        let has_required_fields = input.get("amount").is_some();

        if !has_required_fields {
            return Ok(error_result(
                step_uuid,
                "Validation failed: missing required field \"amount\"".to_string(),
                Some("VALIDATION_ERROR".to_string()),
                Some("validation_error".to_string()),
                false,
                start_time.elapsed().as_millis() as i64,
                None,
            ));
        }

        let mut metadata = HashMap::new();
        metadata.insert("handler".to_string(), json!("MultiMethodHandler"));
        metadata.insert("validated".to_string(), json!(true));

        Ok(success_result(
            step_uuid,
            json!({
                "invoked_method": "validate",
                "handler": "MultiMethodHandler",
                "message": "Validation completed successfully",
                "validated": true,
                "input_validated": input,
                "step_name": step_name
            }),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    /// Processing entry point.
    async fn handle_process(
        &self,
        step_data: &TaskSequenceStep,
        start_time: std::time::Instant,
    ) -> Result<StepExecutionResult> {
        let step_uuid = step_data.workflow_step.workflow_step_uuid;
        let step_name = &step_data.workflow_step.name;
        let context = step_data.task.task.context.clone().unwrap_or(json!({}));
        let input = context.get("data").cloned().unwrap_or(json!({}));
        let amount = input.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);

        // Simple processing logic for testing - add 10% processing fee
        let processed_amount = amount * 1.1;
        let processing_fee = processed_amount - amount;

        let mut metadata = HashMap::new();
        metadata.insert("handler".to_string(), json!("MultiMethodHandler"));
        metadata.insert("processing_fee".to_string(), json!(processing_fee));

        Ok(success_result(
            step_uuid,
            json!({
                "invoked_method": "process",
                "handler": "MultiMethodHandler",
                "message": "Processing completed",
                "original_amount": amount,
                "processed_amount": processed_amount,
                "processing_fee": processing_fee,
                "step_name": step_name
            }),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    /// Refund entry point.
    async fn handle_refund(
        &self,
        step_data: &TaskSequenceStep,
        start_time: std::time::Instant,
    ) -> Result<StepExecutionResult> {
        let step_uuid = step_data.workflow_step.workflow_step_uuid;
        let step_name = &step_data.workflow_step.name;
        let context = step_data.task.task.context.clone().unwrap_or(json!({}));
        let input = context.get("data").cloned().unwrap_or(json!({}));
        let amount = input.get("amount").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let reason = input
            .get("reason")
            .and_then(|v| v.as_str())
            .unwrap_or("not_specified");

        let refund_id = format!(
            "refund_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
        );

        let mut metadata = HashMap::new();
        metadata.insert("handler".to_string(), json!("MultiMethodHandler"));
        metadata.insert("refund_id".to_string(), json!(refund_id.clone()));

        Ok(success_result(
            step_uuid,
            json!({
                "invoked_method": "refund",
                "handler": "MultiMethodHandler",
                "message": "Refund processed",
                "refund_amount": amount,
                "refund_reason": reason,
                "refund_id": refund_id,
                "step_name": step_name
            }),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }
}

/// Second multi-method handler for testing resolver chain with different handlers.
///
/// This handler is used to verify that the resolver chain can find
/// handlers by different callable addresses.
#[derive(Debug)]
pub struct AlternateMethodHandler {
    #[expect(dead_code, reason = "config available for future use")]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for AlternateMethodHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_name = step_data.workflow_step.name.clone();

        // Get the method from step definition (defaults to "call")
        let method = step_data
            .step_definition
            .handler
            .method
            .as_deref()
            .unwrap_or("call");

        info!(
            "AlternateMethodHandler dispatching to method: {} for step: {}",
            method, step_name
        );

        match method {
            "execute_action" => self.handle_execute_action(step_data, start_time).await,
            "call" | _ => self.handle_call(step_data, start_time).await,
        }
    }

    fn name(&self) -> &str {
        "resolver_tests_alternate_method"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

impl AlternateMethodHandler {
    /// Default entry point.
    async fn handle_call(
        &self,
        step_data: &TaskSequenceStep,
        start_time: std::time::Instant,
    ) -> Result<StepExecutionResult> {
        let step_uuid = step_data.workflow_step.workflow_step_uuid;
        let step_name = &step_data.workflow_step.name;

        let mut metadata = HashMap::new();
        metadata.insert("handler".to_string(), json!("AlternateMethodHandler"));

        Ok(success_result(
            step_uuid,
            json!({
                "invoked_method": "call",
                "handler": "AlternateMethodHandler",
                "message": "Alternate handler default method",
                "step_name": step_name
            }),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    /// Custom action method.
    async fn handle_execute_action(
        &self,
        step_data: &TaskSequenceStep,
        start_time: std::time::Instant,
    ) -> Result<StepExecutionResult> {
        let step_uuid = step_data.workflow_step.workflow_step_uuid;
        let step_name = &step_data.workflow_step.name;
        let context = step_data.task.task.context.clone().unwrap_or(json!({}));
        let action = context
            .get("action_type")
            .and_then(|v| v.as_str())
            .unwrap_or("default_action");

        let mut metadata = HashMap::new();
        metadata.insert("handler".to_string(), json!("AlternateMethodHandler"));
        metadata.insert("action_type".to_string(), json!(action));

        Ok(success_result(
            step_uuid,
            json!({
                "invoked_method": "execute_action",
                "handler": "AlternateMethodHandler",
                "message": "Custom action executed",
                "action_type": action,
                "step_name": step_name
            }),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }
}
