//! # Conditional Approval Rust Handlers
//!
//! Native Rust implementation of the conditional approval workflow pattern with decision point
//! routing. This demonstrates TAS-53 Dynamic Workflow Decision Points in pure Rust.
//!
//! **Note**: Uses namespace `conditional_approval_rust` to avoid conflicts with the Ruby
//! implementation which uses `conditional_approval`.
//!
//! ## Workflow Pattern
//!
//! ```text
//!    validate_request
//!           |
//!    routing_decision (DECISION POINT)
//!           |
//!      +----+----+
//!      |    |    |
//!   auto  mgr  mgr+fin
//!      |    |    |
//!      +----+----+
//!           |
//!    finalize_approval
//! ```
//!
//! ## Routing Logic
//!
//! - **amount < $1,000**: `auto_approve` step created
//! - **$1,000 ≤ amount < $5,000**: `manager_approval` step created
//! - **amount ≥ $5,000**: `manager_approval` + `finance_review` steps created
//!
//! ## Performance Benefits
//!
//! Native Rust implementation provides:
//! - Zero-cost abstractions for decision point logic
//! - Compile-time guarantees for step creation safety
//! - Memory-safe dynamic workflow construction
//! - Predictable performance for approval routing

use super::{error_result, success_result, RustStepHandler, StepHandlerConfig};
use anyhow::Result;
use async_trait::async_trait;
use chrono::Utc;
use serde_json::json;
use std::collections::HashMap;
use tasker_shared::messaging::{DecisionPointOutcome, StepExecutionResult};
use tasker_shared::types::TaskSequenceStep;
use tracing::{error, info};

const SMALL_AMOUNT_THRESHOLD: i64 = 1000;
const LARGE_AMOUNT_THRESHOLD: i64 = 5000;

/// Validate Request: Initial step that validates the approval request
#[derive(Debug)]
pub struct ValidateRequestHandler {
    #[expect(
        dead_code,
        reason = "API compatibility - config available for future handler enhancements"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for ValidateRequestHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Extract and validate required fields from task context
        let amount: i64 = match step_data.get_context_field("amount") {
            Ok(value) => value,
            Err(e) => {
                error!("Missing amount in task context: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "Task context must contain amount".to_string(),
                    Some("MISSING_AMOUNT".to_string()),
                    Some("ValidationError".to_string()),
                    false, // Not retryable - data validation error
                    start_time.elapsed().as_millis() as i64,
                    Some(HashMap::from([("field".to_string(), json!("amount"))])),
                ));
            }
        };

        if amount <= 0 {
            error!("Amount {} is not positive", amount);
            return Ok(error_result(
                step_uuid,
                "Amount must be positive".to_string(),
                Some("INVALID_AMOUNT".to_string()),
                Some("ValidationError".to_string()),
                false, // Not retryable - data validation error
                start_time.elapsed().as_millis() as i64,
                Some(HashMap::from([("amount".to_string(), json!(amount))])),
            ));
        }

        let requester: String = match step_data.get_context_field("requester") {
            Ok(value) => value,
            Err(e) => {
                error!("Missing requester in task context: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "Task context must contain requester".to_string(),
                    Some("MISSING_REQUESTER".to_string()),
                    Some("ValidationError".to_string()),
                    false,
                    start_time.elapsed().as_millis() as i64,
                    Some(HashMap::from([("field".to_string(), json!("requester"))])),
                ));
            }
        };

        if requester.is_empty() {
            error!("Requester is empty");
            return Ok(error_result(
                step_uuid,
                "Requester cannot be empty".to_string(),
                Some("EMPTY_REQUESTER".to_string()),
                Some("ValidationError".to_string()),
                false,
                start_time.elapsed().as_millis() as i64,
                None,
            ));
        }

        let purpose: String = match step_data.get_context_field("purpose") {
            Ok(value) => value,
            Err(e) => {
                error!("Missing purpose in task context: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "Task context must contain purpose".to_string(),
                    Some("MISSING_PURPOSE".to_string()),
                    Some("ValidationError".to_string()),
                    false,
                    start_time.elapsed().as_millis() as i64,
                    Some(HashMap::from([("field".to_string(), json!("purpose"))])),
                ));
            }
        };

        if purpose.is_empty() {
            error!("Purpose is empty");
            return Ok(error_result(
                step_uuid,
                "Purpose cannot be empty".to_string(),
                Some("EMPTY_PURPOSE".to_string()),
                Some("ValidationError".to_string()),
                false,
                start_time.elapsed().as_millis() as i64,
                None,
            ));
        }

        info!(
            "Validating approval request: {} requesting ${} for {}",
            requester, amount, purpose
        );

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("validate"));
        metadata.insert("step_type".to_string(), json!("initial"));
        metadata.insert(
            "validation_checks".to_string(),
            json!(["amount_positive", "requester_present", "purpose_present"]),
        );

        Ok(success_result(
            step_uuid,
            json!({
                "amount": amount,
                "requester": requester,
                "purpose": purpose,
                "validated_at": Utc::now().to_rfc3339()
            }),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &'static str {
        "validate_request"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// Routing Decision: DECISION POINT that routes approval based on amount
///
/// This is a TAS-53 Decision Point step that makes runtime decisions about
/// which approval workflow steps to create dynamically.
#[derive(Debug)]
pub struct RoutingDecisionHandler {
    #[expect(
        dead_code,
        reason = "API compatibility - config available for future handler enhancements"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for RoutingDecisionHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        // Get amount from task context
        let amount: i64 = match step_data.get_context_field("amount") {
            Ok(value) => value,
            Err(e) => {
                error!("Missing amount for routing decision: {}", e);
                return Ok(error_result(
                    step_uuid,
                    "Amount is required for routing decision".to_string(),
                    Some("MISSING_AMOUNT".to_string()),
                    Some("DecisionError".to_string()),
                    false,
                    start_time.elapsed().as_millis() as i64,
                    None,
                ));
            }
        };

        // Determine routing based on amount thresholds
        let (route_type, steps, reasoning) = if amount < SMALL_AMOUNT_THRESHOLD {
            (
                "auto_approval",
                vec!["auto_approve"],
                format!(
                    "Amount ${} below ${} threshold - auto-approval",
                    amount, SMALL_AMOUNT_THRESHOLD
                ),
            )
        } else if amount < LARGE_AMOUNT_THRESHOLD {
            (
                "manager_only",
                vec!["manager_approval"],
                format!(
                    "Amount ${} requires manager approval (between ${} and ${})",
                    amount, SMALL_AMOUNT_THRESHOLD, LARGE_AMOUNT_THRESHOLD
                ),
            )
        } else {
            (
                "dual_approval",
                vec!["manager_approval", "finance_review"],
                format!(
                    "Amount ${} >= ${} - requires both manager and finance approval",
                    amount, LARGE_AMOUNT_THRESHOLD
                ),
            )
        };

        info!(
            "Routing decision for ${}: {} -> creating steps: {}",
            amount,
            reasoning,
            steps.join(", ")
        );

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("routing_decision"));
        metadata.insert(
            "route_thresholds".to_string(),
            json!({
                "small": SMALL_AMOUNT_THRESHOLD,
                "large": LARGE_AMOUNT_THRESHOLD
            }),
        );

        // Create TAS-53 Decision Point Outcome
        let outcome =
            DecisionPointOutcome::create_steps(steps.iter().map(|s| (*s).to_string()).collect());

        // Embed decision_point_outcome in result JSON for orchestration processing
        Ok(success_result(
            step_uuid,
            json!({
                "route_type": route_type,
                "reasoning": reasoning,
                "amount": amount,
                "decision_point_outcome": outcome.to_value()
            }),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &'static str {
        "routing_decision"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// Auto Approve: Automatically approves small requests
///
/// This step is dynamically created by the `routing_decision` decision point
/// when the amount is below the small threshold ($1,000).
#[derive(Debug)]
pub struct AutoApproveHandler {
    #[expect(
        dead_code,
        reason = "API compatibility - config available for future handler enhancements"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for AutoApproveHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        let amount: i64 = step_data.get_context_field("amount").unwrap_or(0);
        let requester: String = step_data
            .get_context_field("requester")
            .unwrap_or_else(|_| "unknown".to_string());

        info!("Auto-approving request: {} for ${}", requester, amount);

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("auto_approve"));
        metadata.insert("step_type".to_string(), json!("dynamic_branch"));
        metadata.insert("approval_method".to_string(), json!("automated"));

        Ok(success_result(
            step_uuid,
            json!({
                "approved": true,
                "approval_type": "automatic",
                "approved_amount": amount,
                "approved_by": "system",
                "approved_at": Utc::now().to_rfc3339(),
                "notes": "Automatically approved - below manual review threshold"
            }),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &'static str {
        "auto_approve"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// Manager Approval: Manual manager review for medium/large requests
///
/// Dynamically created by `routing_decision` for amounts >= $1,000.
#[derive(Debug)]
pub struct ManagerApprovalHandler {
    #[expect(
        dead_code,
        reason = "API compatibility - config available for future handler enhancements"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for ManagerApprovalHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        let amount: i64 = step_data.get_context_field("amount").unwrap_or(0);
        let requester: String = step_data
            .get_context_field("requester")
            .unwrap_or_else(|_| "unknown".to_string());

        info!("Manager approving request: {} for ${}", requester, amount);

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("manager_approval"));
        metadata.insert("step_type".to_string(), json!("dynamic_branch"));
        metadata.insert("approval_level".to_string(), json!("manager"));

        Ok(success_result(
            step_uuid,
            json!({
                "approved": true,
                "approval_type": "manager",
                "approved_amount": amount,
                "approved_by": "manager_rust_example",
                "approved_at": Utc::now().to_rfc3339(),
                "notes": "Approved by manager - medium/large amount"
            }),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &'static str {
        "manager_approval"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// Finance Review: Additional finance approval for large requests
///
/// Dynamically created by `routing_decision` for amounts >= $5,000.
#[derive(Debug)]
pub struct FinanceReviewHandler {
    #[expect(
        dead_code,
        reason = "API compatibility - config available for future handler enhancements"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for FinanceReviewHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        let amount: i64 = step_data.get_context_field("amount").unwrap_or(0);
        let requester: String = step_data
            .get_context_field("requester")
            .unwrap_or_else(|_| "unknown".to_string());

        info!("Finance reviewing request: {} for ${}", requester, amount);

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("finance_review"));
        metadata.insert("step_type".to_string(), json!("dynamic_branch"));
        metadata.insert("approval_level".to_string(), json!("finance"));

        Ok(success_result(
            step_uuid,
            json!({
                "approved": true,
                "approval_type": "finance",
                "approved_amount": amount,
                "approved_by": "finance_rust_example",
                "approved_at": Utc::now().to_rfc3339(),
                "notes": "Approved by finance - large amount requiring dual approval"
            }),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &'static str {
        "finance_review"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}

/// Finalize Approval: Final convergence step that processes all approvals
///
/// This step receives results from whichever approval path was taken:
/// - `auto_approve` (for small amounts)
/// - `manager_approval` (for medium amounts)
/// - `manager_approval` + `finance_review` (for large amounts)
#[derive(Debug)]
pub struct FinalizeApprovalHandler {
    #[expect(
        dead_code,
        reason = "API compatibility - config available for future handler enhancements"
    )]
    config: StepHandlerConfig,
}

#[async_trait]
impl RustStepHandler for FinalizeApprovalHandler {
    async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
        let start_time = std::time::Instant::now();
        let step_uuid = step_data.workflow_step.workflow_step_uuid;

        let amount: i64 = step_data.get_context_field("amount").unwrap_or(0);
        let requester: String = step_data
            .get_context_field("requester")
            .unwrap_or_else(|_| "unknown".to_string());
        let purpose: String = step_data
            .get_context_field("purpose")
            .unwrap_or_else(|_| "unknown".to_string());

        // Collect approval results from all possible approval steps
        let approval_steps = vec!["auto_approve", "manager_approval", "finance_review"];
        let mut approvals = Vec::new();
        let mut approved_by = Vec::new();
        let mut approval_types = Vec::new();

        for step_name in approval_steps {
            if let Ok(result) =
                step_data.get_dependency_result_column_value::<serde_json::Value>(step_name)
            {
                if let Some(approved) = result.get("approved").and_then(serde_json::Value::as_bool)
                {
                    if approved {
                        approvals.push(step_name.to_string());
                        if let Some(approval_type) =
                            result.get("approval_type").and_then(|v| v.as_str())
                        {
                            approval_types.push(approval_type.to_string());
                        }
                        if let Some(by) = result.get("approved_by").and_then(|v| v.as_str()) {
                            approved_by.push(by.to_string());
                        }
                    }
                }
            }
        }

        info!(
            "Finalizing approval for {}: ${} - {} approval(s) received",
            requester,
            amount,
            approvals.len()
        );

        // Determine approval path based on collected approvals
        approval_types.sort();
        let approval_path = match approval_types.as_slice() {
            types if types == ["automatic"] => "auto",
            types if types == ["manager"] => "manager_only",
            types if types == ["finance", "manager"] || types == ["manager", "finance"] => {
                "dual_approval"
            }
            _ => "unknown",
        };

        info!(
            "Approval finalized: {} for ${} via {} path",
            requester, amount, approval_path
        );

        let mut metadata = HashMap::new();
        metadata.insert("operation".to_string(), json!("finalize_approval"));
        metadata.insert("step_type".to_string(), json!("convergence"));
        metadata.insert("approval_count".to_string(), json!(approvals.len()));

        Ok(success_result(
            step_uuid,
            json!({
                "approved": true,
                "final_amount": amount,
                "requester": requester,
                "purpose": purpose,
                "approval_chain": approval_types,
                "approved_by": approved_by,
                "finalized_at": Utc::now().to_rfc3339(),
                "approval_path": approval_path
            }),
            start_time.elapsed().as_millis() as i64,
            Some(metadata),
        ))
    }

    fn name(&self) -> &'static str {
        "finalize_approval"
    }

    fn new(config: StepHandlerConfig) -> Self {
        Self { config }
    }
}
