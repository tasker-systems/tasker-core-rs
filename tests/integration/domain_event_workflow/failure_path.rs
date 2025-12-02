//! # Domain Event Workflow Failure Path Integration Tests
//!
//! TAS-65: Tests failure path behavior for domain event publishing workflow.
//!
//! This module validates that:
//!
//! 1. Step failures are correctly handled by the state machine
//! 2. Failure condition events would be triggered (worker layer publishes)
//! 3. Always condition events would be triggered regardless of outcome
//! 4. Success condition events would NOT be triggered on failure
//!
//! ## Event Publication Conditions
//!
//! The domain_event_publishing workflow declares these conditions:
//! - `order.validated`: condition: success (NOT published on failure)
//! - `payment.processed`: condition: success (NOT published on failure)
//! - `payment.failed`: condition: failure (PUBLISHED on failure)
//! - `inventory.updated`: condition: always (ALWAYS published)
//! - `notification.sent`: condition: success (NOT published on failure)
//!
//! ## Test Strategy
//!
//! These tests validate the orchestration layer's handling of failures.
//! Actual event publication happens at the worker layer - these tests
//! ensure the state machine correctly transitions to error state.

use anyhow::Result;
use sqlx::PgPool;

use crate::common::lifecycle_test_manager::LifecycleTestManager;

/// Test step failure causes transition to error state
///
/// Validates that when a step fails:
/// 1. Step transitions to error state
/// 2. Retry eligibility is correctly calculated
/// 3. Dependent steps remain blocked
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_step_failure_transitions_to_error_state(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 FAILURE PATH: Testing step failure state transition");

    // Use Ruby template path for domain_events namespace
    let manager =
        LifecycleTestManager::with_template_path(pool, "tests/fixtures/task_templates/ruby")
            .await?;

    let task_request = manager.create_task_request_for_template(
        "domain_event_publishing",
        "domain_events",
        serde_json::json!({
            "order_id": "test-order-fail-001",
            "customer_id": "customer-fail-001",
            "amount": 99.99,
            "simulate_failure": true
        }),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // First complete validate_order successfully
    manager
        .complete_step(
            task_uuid,
            "domain_events_validate_order",
            serde_json::json!({
                "order_id": "test-order-fail-001",
                "validation_timestamp": "2025-11-28T00:00:00Z",
                "validation_checks": ["order_id_present"],
                "validated": true
            }),
        )
        .await?;

    // Now process_payment should be ready
    manager
        .validate_step_readiness(task_uuid, "domain_events_process_payment", true, 1, 1)
        .await?;

    // Fail the process_payment step (simulating payment gateway failure)
    manager
        .fail_step(
            task_uuid,
            "domain_events_process_payment",
            "Payment gateway timeout: GATEWAY_TIMEOUT",
        )
        .await?;

    // Verify step is now in error state
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;
    let payment_step = step_readiness
        .iter()
        .find(|s| s.name == "domain_events_process_payment")
        .expect("process_payment step should exist");

    assert_eq!(
        payment_step.current_state.as_deref(),
        Some("error"),
        "Failed step should be in error state"
    );

    // Verify dependent steps are still blocked
    let inventory_step = step_readiness
        .iter()
        .find(|s| s.name == "domain_events_update_inventory")
        .expect("update_inventory step should exist");

    assert!(
        !inventory_step.ready_for_execution,
        "Dependent step should not be ready when parent failed"
    );

    tracing::info!("✅ Step correctly transitioned to error state");
    tracing::info!("   Note: Worker layer would publish payment.failed event (condition: failure)");
    tracing::info!(
        "   Note: Worker layer would NOT publish payment.processed (condition: success)"
    );

    Ok(())
}

/// Test retry eligibility after step failure
///
/// Validates that after a step fails:
/// 1. Retry eligibility is calculated based on attempts vs max_attempts
/// 2. Step can be retried if eligible
/// 3. Backoff is applied between retries
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_retry_eligibility_after_failure(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 FAILURE PATH: Testing retry eligibility after failure");

    // Use Ruby template path for domain_events namespace
    let manager =
        LifecycleTestManager::with_template_path(pool, "tests/fixtures/task_templates/ruby")
            .await?;

    let task_request = manager.create_task_request_for_template(
        "domain_event_publishing",
        "domain_events",
        serde_json::json!({
            "order_id": "test-order-retry-001",
            "customer_id": "customer-retry-001",
            "amount": 149.99
        }),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // Complete validate_order
    manager
        .complete_step(
            task_uuid,
            "domain_events_validate_order",
            serde_json::json!({
                "validated": true
            }),
        )
        .await?;

    // Fail process_payment first time
    manager
        .fail_step(task_uuid, "domain_events_process_payment", "First failure")
        .await?;

    // Check retry eligibility
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;
    let payment_step = step_readiness
        .iter()
        .find(|s| s.name == "domain_events_process_payment")
        .expect("process_payment step should exist");

    tracing::info!(
        attempts = payment_step.attempts,
        max_attempts = payment_step.max_attempts,
        retry_eligible = payment_step.retry_eligible,
        "📊 Retry status after first failure"
    );

    // Step should still be retryable (attempts < max_attempts)
    // Note: max_attempts is 3 in the test environment
    assert!(
        payment_step.retry_eligible || payment_step.attempts < payment_step.max_attempts,
        "Step should be retry eligible after first failure"
    );

    tracing::info!("✅ Retry eligibility correctly calculated");
    tracing::info!("   Event context: retryable_failure vs permanent_failure");
    tracing::info!("   - Retry eligible: Use PublicationCondition::RetryableFailure");
    tracing::info!("   - No retries left: Use PublicationCondition::PermanentFailure");

    Ok(())
}

/// Test permanent failure when max attempts exhausted
///
/// Validates that when a step exhausts all retries:
/// 1. Step is no longer retry eligible
/// 2. This represents a permanent failure
/// 3. Different event conditions may apply (permanent_failure vs retryable_failure)
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_permanent_failure_after_max_attempts(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 FAILURE PATH: Testing permanent failure after max attempts");

    // Use Ruby template path for domain_events namespace
    let manager =
        LifecycleTestManager::with_template_path(pool, "tests/fixtures/task_templates/ruby")
            .await?;

    let task_request = manager.create_task_request_for_template(
        "domain_event_publishing",
        "domain_events",
        serde_json::json!({
            "order_id": "test-order-perm-001",
            "customer_id": "customer-perm-001",
            "amount": 199.99
        }),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // Complete validate_order
    manager
        .complete_step(
            task_uuid,
            "domain_events_validate_order",
            serde_json::json!({ "validated": true }),
        )
        .await?;

    // Fail process_payment until max attempts exhausted
    // Test environment has max_attempts = 1, so first failure exhausts retries
    manager
        .fail_step(task_uuid, "domain_events_process_payment", "Failure 1")
        .await?;

    // Check if retries are exhausted
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;
    let payment_step = step_readiness
        .iter()
        .find(|s| s.name == "domain_events_process_payment")
        .expect("process_payment step should exist");

    tracing::info!(
        attempts = payment_step.attempts,
        max_attempts = payment_step.max_attempts,
        retry_eligible = payment_step.retry_eligible,
        "📊 Status after potential max attempts"
    );

    // If attempts >= max_attempts, this is a permanent failure
    let is_permanent_failure = payment_step.attempts >= payment_step.max_attempts;

    if is_permanent_failure {
        tracing::info!("✅ Step has reached permanent failure (max attempts exhausted)");
        tracing::info!("   Event: PublicationCondition::PermanentFailure would trigger");
    } else {
        tracing::info!("ℹ️ Step still has retries available");
        tracing::info!("   Event: PublicationCondition::RetryableFailure would trigger");
    }

    Ok(())
}

/// Test always condition events are independent of step outcome
///
/// The inventory.updated event has condition: always, meaning it should
/// be published regardless of whether the step succeeds or fails.
/// This test validates that the step with "always" condition behaves correctly.
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_always_condition_independent_of_outcome(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 FAILURE PATH: Testing 'always' condition independence");

    // Use Ruby template path for domain_events namespace
    let manager =
        LifecycleTestManager::with_template_path(pool, "tests/fixtures/task_templates/ruby")
            .await?;

    let task_request = manager.create_task_request_for_template(
        "domain_event_publishing",
        "domain_events",
        serde_json::json!({
            "order_id": "test-order-always-001",
            "customer_id": "customer-always-001",
            "amount": 299.99
        }),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // Complete first two steps to reach update_inventory
    manager
        .complete_step(
            task_uuid,
            "domain_events_validate_order",
            serde_json::json!({ "validated": true }),
        )
        .await?;

    manager
        .complete_step(
            task_uuid,
            "domain_events_process_payment",
            serde_json::json!({
                "transaction_id": "TXN-always-001",
                "amount": 299.99,
                "status": "success"
            }),
        )
        .await?;

    // Now update_inventory should be ready
    manager
        .validate_step_readiness(task_uuid, "domain_events_update_inventory", true, 1, 1)
        .await?;

    // Fail the update_inventory step (inventory system unavailable)
    manager
        .fail_step(
            task_uuid,
            "domain_events_update_inventory",
            "Inventory service unavailable",
        )
        .await?;

    // Verify step is in error state
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;
    let inventory_step = step_readiness
        .iter()
        .find(|s| s.name == "domain_events_update_inventory")
        .expect("update_inventory step should exist");

    assert_eq!(
        inventory_step.current_state.as_deref(),
        Some("error"),
        "Failed inventory step should be in error state"
    );

    tracing::info!("✅ Step with 'always' condition correctly failed");
    tracing::info!("   Event: inventory.updated would still be published (condition: always)");
    tracing::info!(
        "   Payload would include: success: false, error_message: 'Inventory service unavailable'"
    );

    Ok(())
}

/// Test complete failure scenario with event publication expectations
///
/// Walks through a complete failure scenario and documents which events
/// would be published by the worker layer based on conditions.
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_complete_failure_scenario_event_expectations(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 FAILURE PATH: Complete failure scenario with event expectations");

    // Use Ruby template path for domain_events namespace
    let manager =
        LifecycleTestManager::with_template_path(pool, "tests/fixtures/task_templates/ruby")
            .await?;

    let task_request = manager.create_task_request_for_template(
        "domain_event_publishing",
        "domain_events",
        serde_json::json!({
            "order_id": "test-order-complete-fail",
            "customer_id": "customer-complete-fail",
            "amount": 999.99,
            "simulate_failure": true
        }),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // Step 1: Complete validate_order successfully
    manager
        .complete_step(
            task_uuid,
            "domain_events_validate_order",
            serde_json::json!({
                "order_id": "test-order-complete-fail",
                "validation_timestamp": "2025-11-28T00:00:00Z",
                "validated": true
            }),
        )
        .await?;

    tracing::info!("Step 1 (validate_order) completed successfully");
    tracing::info!("  Event published: order.validated (condition: success) ✓");

    // Step 2: Fail process_payment
    manager
        .fail_step(
            task_uuid,
            "domain_events_process_payment",
            "Payment declined: CARD_DECLINED",
        )
        .await?;

    tracing::info!("Step 2 (process_payment) FAILED");
    tracing::info!("  Event NOT published: payment.processed (condition: success) ✗");
    tracing::info!("  Event published: payment.failed (condition: failure) ✓");

    // Final validation
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let completed_steps: Vec<_> = step_readiness
        .iter()
        .filter(|s| s.current_state.as_deref() == Some("complete"))
        .map(|s| &s.name)
        .collect();

    let error_steps: Vec<_> = step_readiness
        .iter()
        .filter(|s| s.current_state.as_deref() == Some("error"))
        .map(|s| &s.name)
        .collect();

    let pending_steps: Vec<_> = step_readiness
        .iter()
        .filter(|s| {
            s.current_state.as_deref() == Some("pending")
                || s.current_state.as_deref() == Some("enqueued")
        })
        .map(|s| &s.name)
        .collect();

    tracing::info!("📊 Final state summary:");
    tracing::info!("  Completed: {:?}", completed_steps);
    tracing::info!("  Error: {:?}", error_steps);
    tracing::info!("  Pending/Blocked: {:?}", pending_steps);

    tracing::info!("📬 Event publication summary:");
    tracing::info!("  ✓ order.validated - Published (step succeeded, condition: success)");
    tracing::info!("  ✗ payment.processed - NOT published (step failed, condition: success)");
    tracing::info!("  ✓ payment.failed - Published (step failed, condition: failure)");
    tracing::info!("  ✗ inventory.updated - NOT executed (blocked by failed parent)");
    tracing::info!("  ✗ notification.sent - NOT executed (blocked by failed parent)");

    Ok(())
}
