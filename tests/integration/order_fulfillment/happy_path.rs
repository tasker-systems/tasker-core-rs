use anyhow::Result;
use sqlx::PgPool;

use crate::common::lifecycle_test_manager::LifecycleTestManager;

/// Test order fulfillment workflow initialization and setup
#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_order_fulfillment_initialization(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 ORDER FULFILLMENT: Initialization and setup");

    let manager = LifecycleTestManager::new(pool).await?;

    let task_request = manager.create_task_request_for_template(
        "business_workflow",
        "order_fulfillment",
        serde_json::json!({
            "customer": {
                "id": 12345,
                "name": "Jane Doe",
                "email": "jane.doe@example.com"
            },
            "items": [
                {"sku": "WIDGET-001", "quantity": 2, "price": 29.99},
                {"sku": "GADGET-002", "quantity": 1, "price": 49.99}
            ],
            "payment": {
                "method": "credit_card",
                "token": "tok_test_12345"
            },
            "shipping": {
                "method": "standard",
                "address": {
                    "street": "123 Main St",
                    "city": "Springfield",
                    "state": "IL",
                    "zip": "62701"
                }
            }
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
    assert_eq!(ready_count, 1, "Only validate_order should be ready");

    tracing::info!("🎯 ORDER FULFILLMENT: Initialization test complete");

    Ok(())
}

/// Test order fulfillment workflow with linear dependency chain
#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_order_fulfillment_linear_dependency_chain(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 ORDER FULFILLMENT: Linear dependency chain validation");

    let manager = LifecycleTestManager::new(pool).await?;

    let task_request = manager.create_task_request_for_template(
        "business_workflow",
        "order_fulfillment",
        serde_json::json!({
            "customer": {"id": 12345, "name": "Jane Doe", "email": "jane.doe@example.com"},
            "items": [{"sku": "WIDGET-001", "quantity": 2, "price": 29.99}],
            "payment": {"method": "credit_card", "token": "tok_test_12345"},
            "shipping": {"method": "standard", "address": {}}
        }),
    );

    let init_result = manager.initialize_task(task_request).await?;

    // Get all step readiness information
    let step_readiness = manager
        .get_step_readiness_status(init_result.task_uuid)
        .await?;

    assert_eq!(step_readiness.len(), 4, "Should have 4 steps");

    // Validate linear dependency structure:
    // validate_order: no parents
    // reserve_inventory: depends on validate_order (1 parent)
    // process_payment: depends on validate_order + reserve_inventory (2 parents)
    // ship_order: depends on all previous (3 parents)

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
            "validate_order" => {
                assert_eq!(step.total_parents, 0, "Validate has no dependencies");
                assert!(
                    step.dependencies_satisfied,
                    "Validate dependencies satisfied"
                );
                assert!(step.ready_for_execution, "Validate should be ready");
            }
            "reserve_inventory" => {
                assert_eq!(step.total_parents, 1, "Reserve depends on validate_order");
                assert!(
                    !step.dependencies_satisfied,
                    "Reserve dependencies not satisfied yet"
                );
                assert!(!step.ready_for_execution, "Reserve not ready yet");
            }
            "process_payment" => {
                assert_eq!(
                    step.total_parents, 2,
                    "Payment depends on validate_order and reserve_inventory"
                );
                assert!(
                    !step.dependencies_satisfied,
                    "Payment dependencies not satisfied yet"
                );
                assert!(!step.ready_for_execution, "Payment not ready yet");
            }
            "ship_order" => {
                assert_eq!(step.total_parents, 3, "Ship depends on all previous steps");
                assert!(
                    !step.dependencies_satisfied,
                    "Ship dependencies not satisfied yet"
                );
                assert!(!step.ready_for_execution, "Ship not ready yet");
            }
            _ => panic!("Unexpected step: {}", step.name),
        }
    }

    tracing::info!("🎯 ORDER FULFILLMENT: Linear dependency chain test complete");

    Ok(())
}

/// Test complete order fulfillment workflow execution
#[sqlx::test(migrator = "tasker_core::test_helpers::MIGRATOR")]
async fn test_order_fulfillment_complete_execution(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 ORDER FULFILLMENT: Complete execution");

    let manager = LifecycleTestManager::new(pool).await?;

    let task_request = manager.create_task_request_for_template(
        "business_workflow",
        "order_fulfillment",
        serde_json::json!({
            "customer": {"id": 12345, "name": "Jane Doe", "email": "jane.doe@example.com"},
            "items": [
                {"sku": "WIDGET-001", "quantity": 2, "price": 29.99},
                {"sku": "GADGET-002", "quantity": 1, "price": 49.99}
            ],
            "payment": {"method": "credit_card", "token": "tok_test_12345"},
            "shipping": {"method": "express", "address": {}}
        }),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Complete validate_order
    tracing::info!("📊 PHASE 1: Validating order");
    manager
        .complete_step(
            task_uuid,
            "validate_order",
            serde_json::json!({
                "customer_valid": true,
                "items_valid": true,
                "subtotal": 109.97,
                "tax": 9.90,
                "total": 119.87
            }),
        )
        .await?;

    // Validate: reserve_inventory should now be ready
    manager
        .validate_task_execution_context(task_uuid, 4, 1, 1)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "reserve_inventory", true, 1, 1)
        .await?;

    tracing::info!("✅ Order validated, inventory reservation ready");

    // **PHASE 2**: Complete reserve_inventory
    tracing::info!("📊 PHASE 2: Reserving inventory");
    manager
        .complete_step(
            task_uuid,
            "reserve_inventory",
            serde_json::json!({
                "reservations": [
                    {"sku": "WIDGET-001", "quantity": 2, "warehouse": "WH-CENTRAL", "reservation_id": "RES-001"},
                    {"sku": "GADGET-002", "quantity": 1, "warehouse": "WH-WEST", "reservation_id": "RES-002"}
                ],
                "all_reserved": true
            }),
        )
        .await?;

    // Validate: process_payment should now be ready
    manager
        .validate_task_execution_context(task_uuid, 4, 2, 1)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "process_payment", true, 2, 2)
        .await?;

    tracing::info!("✅ Inventory reserved, payment processing ready");

    // **PHASE 3**: Complete process_payment
    tracing::info!("📊 PHASE 3: Processing payment");
    manager
        .complete_step(
            task_uuid,
            "process_payment",
            serde_json::json!({
                "payment_approved": true,
                "transaction_id": "TXN-12345-ABC",
                "amount": 119.87,
                "method": "credit_card",
                "last_four": "4242"
            }),
        )
        .await?;

    // Validate: ship_order should now be ready
    manager
        .validate_task_execution_context(task_uuid, 4, 3, 1)
        .await?;
    manager
        .validate_step_readiness(task_uuid, "ship_order", true, 3, 3)
        .await?;

    tracing::info!("✅ Payment processed, shipping ready");

    // **PHASE 4**: Complete ship_order
    tracing::info!("📊 PHASE 4: Shipping order");
    manager
        .complete_step(
            task_uuid,
            "ship_order",
            serde_json::json!({
                "shipment_created": true,
                "tracking_number": "TRK-2025-EXPR-001",
                "carrier": "FedEx",
                "estimated_delivery": "2025-10-03",
                "shipment_id": "SHIP-12345"
            }),
        )
        .await?;

    // Validate: All steps complete
    manager
        .validate_task_execution_context(task_uuid, 4, 4, 0)
        .await?;
    assert_eq!(manager.count_ready_steps(task_uuid).await?, 0);

    tracing::info!("✅ All steps complete - order fulfillment successful");

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

    tracing::info!("🎯 ORDER FULFILLMENT: Complete execution test successful");

    Ok(())
}
