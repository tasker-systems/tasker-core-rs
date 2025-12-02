//! # Domain Event Workflow Happy Path Integration Tests
//!
//! TAS-65: Tests the happy path for domain event publishing workflow.
//!
//! This workflow tests a linear 4-step sequence where each step declares
//! domain events to be published. The steps are:
//!
//! 1. `domain_events_validate_order` - Publishes `order.validated` (fast, success)
//! 2. `domain_events_process_payment` - Publishes `payment.processed` (durable, success)
//!    or `payment.failed` (durable, failure)
//! 3. `domain_events_update_inventory` - Publishes `inventory.updated` (fast, always)
//! 4. `domain_events_send_notification` - Publishes `notification.sent` (fast, success)
//!
//! These tests validate the orchestration layer behavior without actually
//! publishing events (which happens at the worker level).

use anyhow::Result;
use sqlx::PgPool;

use crate::common::lifecycle_test_manager::LifecycleTestManager;

/// Test domain event workflow initialization
///
/// Validates that a 4-step workflow with domain event declarations
/// is correctly initialized with proper step structure.
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_domain_event_workflow_initialization(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DOMAIN EVENT WORKFLOW: Initialization and setup");

    // Use Ruby template path for domain_events namespace
    let manager =
        LifecycleTestManager::with_template_path(pool, "tests/fixtures/task_templates/ruby")
            .await?;

    let task_request = manager.create_task_request_for_template(
        "domain_event_publishing",
        "domain_events",
        serde_json::json!({
            "order_id": "test-order-001",
            "customer_id": "customer-001",
            "amount": 99.99,
            "simulate_failure": false
        }),
    );

    let init_result = manager.initialize_task(task_request).await?;

    // Validate: 4 steps total, 0 completed, 1 ready (validate_order)
    manager
        .validate_task_execution_context(
            init_result.task_uuid,
            4, // total_steps
            0, // completed_steps
            1, // ready_steps (only validate_order)
        )
        .await?;

    let ready_count = manager.count_ready_steps(init_result.task_uuid).await?;
    assert_eq!(
        ready_count, 1,
        "Only domain_events_validate_order should be ready"
    );

    tracing::info!("🎯 DOMAIN EVENT WORKFLOW: Initialization test complete");

    Ok(())
}

/// Test domain event workflow dependency chain
///
/// Validates that the 4-step linear dependency chain is correctly
/// established and that steps become ready in sequence.
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_domain_event_workflow_dependency_chain(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DOMAIN EVENT WORKFLOW: Dependency chain validation");

    // Use Ruby template path for domain_events namespace
    let manager =
        LifecycleTestManager::with_template_path(pool, "tests/fixtures/task_templates/ruby")
            .await?;

    let task_request = manager.create_task_request_for_template(
        "domain_event_publishing",
        "domain_events",
        serde_json::json!({
            "order_id": "test-order-002",
            "customer_id": "customer-002",
            "amount": 149.99
        }),
    );

    let init_result = manager.initialize_task(task_request).await?;

    // Get all step readiness information
    let step_readiness = manager
        .get_step_readiness_status(init_result.task_uuid)
        .await?;

    assert_eq!(step_readiness.len(), 4, "Should have 4 steps");

    // Validate dependency structure
    for step in &step_readiness {
        tracing::info!(
            step = %step.name,
            ready = step.ready_for_execution,
            total_parents = step.total_parents,
            completed_parents = step.completed_parents,
            dependencies_satisfied = step.dependencies_satisfied,
            current_state = ?step.current_state,
            "📊 Step analysis"
        );

        match step.name.as_str() {
            "domain_events_validate_order" => {
                assert_eq!(step.total_parents, 0, "validate_order has no dependencies");
                assert!(
                    step.dependencies_satisfied,
                    "validate_order dependencies satisfied"
                );
                assert!(step.ready_for_execution, "validate_order should be ready");
            }
            "domain_events_process_payment" => {
                assert_eq!(
                    step.total_parents, 1,
                    "process_payment depends on validate_order"
                );
                assert!(
                    !step.dependencies_satisfied,
                    "process_payment dependencies not satisfied yet"
                );
                assert!(!step.ready_for_execution, "process_payment not ready yet");
            }
            "domain_events_update_inventory" => {
                assert_eq!(
                    step.total_parents, 1,
                    "update_inventory depends on process_payment"
                );
                assert!(
                    !step.dependencies_satisfied,
                    "update_inventory dependencies not satisfied yet"
                );
                assert!(!step.ready_for_execution, "update_inventory not ready yet");
            }
            "domain_events_send_notification" => {
                assert_eq!(
                    step.total_parents, 1,
                    "send_notification depends on update_inventory"
                );
                assert!(
                    !step.dependencies_satisfied,
                    "send_notification dependencies not satisfied yet"
                );
                assert!(!step.ready_for_execution, "send_notification not ready yet");
            }
            _ => panic!("Unexpected step: {}", step.name),
        }
    }

    tracing::info!("🎯 DOMAIN EVENT WORKFLOW: Dependency chain test complete");

    Ok(())
}

/// Test complete domain event workflow execution
///
/// Validates that all 4 steps can be executed sequentially and that
/// each step's completion enables the next step to become ready.
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_domain_event_workflow_complete_execution(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DOMAIN EVENT WORKFLOW: Complete execution with state transitions");

    // Use Ruby template path for domain_events namespace
    let manager =
        LifecycleTestManager::with_template_path(pool, "tests/fixtures/task_templates/ruby")
            .await?;

    let task_request = manager.create_task_request_for_template(
        "domain_event_publishing",
        "domain_events",
        serde_json::json!({
            "order_id": "test-order-003",
            "customer_id": "customer-003",
            "amount": 299.99,
            "simulate_failure": false
        }),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Complete domain_events_validate_order
    tracing::info!("📊 PHASE 1: Completing domain_events_validate_order");
    manager
        .complete_step(
            task_uuid,
            "domain_events_validate_order",
            serde_json::json!({
                "order_id": "test-order-003",
                "validation_timestamp": "2025-11-28T00:00:00Z",
                "validation_checks": ["order_id_present", "customer_id_present", "amount_positive"],
                "validated": true
            }),
        )
        .await?;

    // Validate: process_payment should now be ready
    manager
        .validate_task_execution_context(task_uuid, 4, 1, 1)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "domain_events_process_payment", true, 1, 1)
        .await?;

    tracing::info!("✅ Step 1 (validate_order) complete, step 2 (process_payment) now ready");

    // **PHASE 2**: Complete domain_events_process_payment
    tracing::info!("📊 PHASE 2: Completing domain_events_process_payment");
    manager
        .complete_step(
            task_uuid,
            "domain_events_process_payment",
            serde_json::json!({
                "transaction_id": "TXN-test-001",
                "amount": 299.99,
                "payment_method": "credit_card",
                "processed_at": "2025-11-28T00:00:01Z",
                "status": "success"
            }),
        )
        .await?;

    // Validate: update_inventory should now be ready
    manager
        .validate_task_execution_context(task_uuid, 4, 2, 1)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "domain_events_update_inventory", true, 1, 1)
        .await?;

    tracing::info!("✅ Step 2 (process_payment) complete, step 3 (update_inventory) now ready");

    // **PHASE 3**: Complete domain_events_update_inventory
    tracing::info!("📊 PHASE 3: Completing domain_events_update_inventory");
    manager
        .complete_step(
            task_uuid,
            "domain_events_update_inventory",
            serde_json::json!({
                "order_id": "test-order-003",
                "items": [
                    {"sku": "ITEM-001", "quantity": 1},
                    {"sku": "ITEM-002", "quantity": 2}
                ],
                "success": true,
                "updated_at": "2025-11-28T00:00:02Z"
            }),
        )
        .await?;

    // Validate: send_notification should now be ready
    manager
        .validate_task_execution_context(task_uuid, 4, 3, 1)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "domain_events_send_notification", true, 1, 1)
        .await?;

    tracing::info!("✅ Step 3 (update_inventory) complete, step 4 (send_notification) now ready");

    // **PHASE 4**: Complete domain_events_send_notification
    tracing::info!("📊 PHASE 4: Completing domain_events_send_notification (final step)");
    manager
        .complete_step(
            task_uuid,
            "domain_events_send_notification",
            serde_json::json!({
                "notification_id": "NOTIF-test-001",
                "channel": "email",
                "recipient": "customer-003",
                "sent_at": "2025-11-28T00:00:03Z",
                "status": "delivered"
            }),
        )
        .await?;

    // Validate: All steps complete
    manager
        .validate_task_execution_context(task_uuid, 4, 4, 0)
        .await?;
    assert_eq!(manager.count_ready_steps(task_uuid).await?, 0);

    tracing::info!("✅ All steps complete - domain event workflow execution successful");

    // **PHASE 5**: Final state validation
    tracing::info!("📊 PHASE 5: Final state validation");
    let final_readiness = manager.get_step_readiness_status(task_uuid).await?;

    for step in &final_readiness {
        assert_eq!(
            step.current_state.as_deref(),
            Some("complete"),
            "Step {} should be in complete state",
            step.name
        );
    }

    tracing::info!("🎯 DOMAIN EVENT WORKFLOW: Complete execution test successful");
    tracing::info!("   Note: Domain events would be published by the worker layer");
    tracing::info!("   Events (when worker executes):");
    tracing::info!("   • order.validated (fast, success)");
    tracing::info!("   • payment.processed (durable, success, PaymentEventPublisher)");
    tracing::info!("   • inventory.updated (fast, always)");
    tracing::info!("   • notification.sent (fast, success, NotificationEventPublisher)");

    Ok(())
}
