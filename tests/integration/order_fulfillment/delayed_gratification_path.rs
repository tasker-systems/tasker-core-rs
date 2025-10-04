use anyhow::Result;
use sqlx::PgPool;

use crate::common::lifecycle_test_manager::LifecycleTestManager;

/// Test order fulfillment with validation retry exhaustion
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_order_fulfillment_validation_retry_exhaustion(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DELAYED GRATIFICATION: Validation retry exhaustion");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "business_workflow",
        "order_fulfillment",
        serde_json::json!({
            "customer": {"id": 12345, "name": "Jane Doe", "email": "invalid-email"},
            "items": [{"sku": "WIDGET-001", "quantity": 2, "price": 29.99}],
            "payment": {"method": "credit_card", "token": "tok_test_12345"},
            "shipping": {"method": "standard", "address": {}}
        }),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Fail validate_order 3 times (retry limit)
    tracing::info!("📊 PHASE 1: Failing validate_order 3 times");
    for attempt in 1..=3 {
        manager
            .fail_step(
                task_uuid,
                "validate_order",
                "Customer email validation failed",
            )
            .await?;
        tracing::info!("Failed validate_order attempt {}/3", attempt);
    }

    // Validate: validate_order in terminal error state, entire workflow blocked
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let validate = step_readiness
        .iter()
        .find(|s| s.name == "validate_order")
        .unwrap();
    assert_eq!(validate.current_state.as_deref(), Some("error"));
    assert_eq!(validate.attempts, 3);
    assert!(!validate.retry_eligible, "Should not be retry eligible");

    // All downstream steps remain pending
    for step_name in ["reserve_inventory", "process_payment", "ship_order"] {
        let step = step_readiness.iter().find(|s| s.name == step_name).unwrap();
        assert_eq!(step.current_state.as_deref(), Some("pending"));
        assert!(!step.dependencies_satisfied);
        assert!(!step.ready_for_execution);
    }

    manager
        .validate_task_execution_context(task_uuid, 4, 0, 0)
        .await?;

    tracing::info!("✅ Validation retry exhaustion blocks entire workflow");
    tracing::info!("🎯 DELAYED GRATIFICATION: Validation retry exhaustion test complete");

    Ok(())
}

/// Test order fulfillment with inventory reservation retry and recovery
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_order_fulfillment_inventory_retry_recovery(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DELAYED GRATIFICATION: Inventory reservation retry and recovery");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

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
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Complete validate_order
    tracing::info!("📊 PHASE 1: Completing validate_order");
    manager
        .complete_step(
            task_uuid,
            "validate_order",
            serde_json::json!({"customer_valid": true, "items_valid": true, "total": 59.98}),
        )
        .await?;

    // **PHASE 2**: Fail reserve_inventory twice, succeed on third attempt
    tracing::info!("📊 PHASE 2: Failing reserve_inventory twice, then recovering");
    manager
        .fail_step(
            task_uuid,
            "reserve_inventory",
            "Temporary inventory service unavailable",
        )
        .await?;
    manager
        .fail_step(
            task_uuid,
            "reserve_inventory",
            "Temporary inventory service unavailable",
        )
        .await?;

    // Validate: reserve_inventory still retry eligible after 2 failures
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;
    let reserve = step_readiness
        .iter()
        .find(|s| s.name == "reserve_inventory")
        .unwrap();
    assert_eq!(reserve.current_state.as_deref(), Some("error"));
    assert_eq!(reserve.attempts, 2);
    assert!(reserve.retry_eligible);

    // **PHASE 3**: Recover reserve_inventory on third attempt
    tracing::info!("📊 PHASE 3: Recovering reserve_inventory");
    manager
        .complete_step(
            task_uuid,
            "reserve_inventory",
            serde_json::json!({
                "reservations": [{"sku": "WIDGET-001", "quantity": 2, "reservation_id": "RES-001"}],
                "all_reserved": true
            }),
        )
        .await?;

    // Validate: process_payment now ready
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;
    let payment = step_readiness
        .iter()
        .find(|s| s.name == "process_payment")
        .unwrap();
    assert!(payment.dependencies_satisfied);
    assert!(payment.ready_for_execution);

    manager
        .validate_task_execution_context(task_uuid, 4, 2, 1)
        .await?;

    tracing::info!("✅ Inventory reservation recovery unblocks payment processing");
    tracing::info!("🎯 DELAYED GRATIFICATION: Inventory retry recovery test complete");

    Ok(())
}

/// Test order fulfillment with payment retry and backoff
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_order_fulfillment_payment_retry_backoff(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DELAYED GRATIFICATION: Payment retry with backoff");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

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
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Complete validate_order and reserve_inventory
    tracing::info!("📊 PHASE 1: Completing validate and reserve steps");
    manager
        .complete_step(
            task_uuid,
            "validate_order",
            serde_json::json!({"customer_valid": true, "items_valid": true, "total": 59.98}),
        )
        .await?;
    manager
        .complete_step(
            task_uuid,
            "reserve_inventory",
            serde_json::json!({"reservations": [], "all_reserved": true}),
        )
        .await?;

    // **PHASE 2**: Fail process_payment once (has retry limit of 2)
    tracing::info!("📊 PHASE 2: Failing process_payment once");
    manager
        .fail_step(task_uuid, "process_payment", "Payment gateway timeout")
        .await?;

    // Validate: SQL shows backoff timing and retry eligibility
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;
    let payment = step_readiness
        .iter()
        .find(|s| s.name == "process_payment")
        .unwrap();

    assert_eq!(payment.current_state.as_deref(), Some("error"));
    assert_eq!(payment.attempts, 1);
    assert!(payment.retry_eligible);
    assert!(payment.next_retry_at.is_some(), "Should have backoff time");

    // ship_order blocked by payment failure
    let ship = step_readiness
        .iter()
        .find(|s| s.name == "ship_order")
        .unwrap();
    assert!(!ship.dependencies_satisfied);

    manager
        .validate_task_execution_context(task_uuid, 4, 2, 0)
        .await?;

    tracing::info!("✅ Payment failure applies backoff, blocks shipping");
    tracing::info!("🎯 DELAYED GRATIFICATION: Payment retry backoff test complete");

    Ok(())
}

/// Test order fulfillment with mid-workflow permanent failure
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_order_fulfillment_mid_workflow_permanent_failure(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DELAYED GRATIFICATION: Mid-workflow permanent failure");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "business_workflow",
        "order_fulfillment",
        serde_json::json!({
            "customer": {"id": 12345, "name": "Jane Doe", "email": "jane.doe@example.com"},
            "items": [{"sku": "OUT-OF-STOCK", "quantity": 100, "price": 29.99}],
            "payment": {"method": "credit_card", "token": "tok_test_12345"},
            "shipping": {"method": "standard", "address": {}}
        }),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Complete validate_order
    tracing::info!("📊 PHASE 1: Completing validate_order");
    manager
        .complete_step(
            task_uuid,
            "validate_order",
            serde_json::json!({"customer_valid": true, "items_valid": true, "total": 2999.00}),
        )
        .await?;

    // **PHASE 2**: Fail reserve_inventory permanently (3 attempts)
    tracing::info!("📊 PHASE 2: Failing reserve_inventory permanently");
    for _ in 0..3 {
        manager
            .fail_step(
                task_uuid,
                "reserve_inventory",
                "Insufficient inventory - item out of stock",
            )
            .await?;
    }

    // Validate: reserve_inventory in terminal error, downstream blocked
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let reserve = step_readiness
        .iter()
        .find(|s| s.name == "reserve_inventory")
        .unwrap();
    assert_eq!(reserve.current_state.as_deref(), Some("error"));
    assert_eq!(reserve.attempts, 3);
    assert!(!reserve.retry_eligible);

    // Payment and shipping blocked
    for step_name in ["process_payment", "ship_order"] {
        let step = step_readiness.iter().find(|s| s.name == step_name).unwrap();
        assert!(!step.dependencies_satisfied);
        assert!(!step.ready_for_execution);
    }

    manager
        .validate_task_execution_context(task_uuid, 4, 1, 0)
        .await?;

    tracing::info!("✅ Inventory reservation permanent failure blocks payment and shipping");
    tracing::info!("🎯 DELAYED GRATIFICATION: Mid-workflow permanent failure test complete");

    Ok(())
}

/// Test order fulfillment with transient failure and eventual completion
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_order_fulfillment_transient_failure_eventual_completion(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 DELAYED GRATIFICATION: Transient failure eventual completion");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "business_workflow",
        "order_fulfillment",
        serde_json::json!({
            "customer": {"id": 12345, "name": "Jane Doe", "email": "jane.doe@example.com"},
            "items": [{"sku": "WIDGET-001", "quantity": 2, "price": 29.99}],
            "payment": {"method": "credit_card", "token": "tok_test_12345"},
            "shipping": {"method": "express", "address": {}}
        }),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Complete validate and reserve, fail payment (transient)
    tracing::info!("📊 PHASE 1: Completing validate/reserve, failing payment");
    manager
        .complete_step(
            task_uuid,
            "validate_order",
            serde_json::json!({"customer_valid": true, "items_valid": true, "total": 59.98}),
        )
        .await?;
    manager
        .complete_step(
            task_uuid,
            "reserve_inventory",
            serde_json::json!({"reservations": [], "all_reserved": true}),
        )
        .await?;
    manager
        .fail_step(
            task_uuid,
            "process_payment",
            "Temporary payment gateway unavailable",
        )
        .await?;

    // **VALIDATION 1**: SQL shows failure blocks shipping
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let payment = step_readiness
        .iter()
        .find(|s| s.name == "process_payment")
        .unwrap();
    assert_eq!(payment.current_state.as_deref(), Some("error"));
    assert!(payment.retry_eligible);

    let ship = step_readiness
        .iter()
        .find(|s| s.name == "ship_order")
        .unwrap();
    assert!(!ship.dependencies_satisfied);
    assert!(!ship.ready_for_execution);

    manager
        .validate_task_execution_context(task_uuid, 4, 2, 0)
        .await?;

    tracing::info!("✅ Payment failure blocks shipping");

    // **PHASE 2**: Recover process_payment
    tracing::info!("📊 PHASE 2: Recovering process_payment");
    manager
        .complete_step(
            task_uuid,
            "process_payment",
            serde_json::json!({
                "payment_approved": true,
                "transaction_id": "TXN-12345-ABC",
                "amount": 59.98
            }),
        )
        .await?;

    // **VALIDATION 2**: SQL shows recovery unblocks shipping
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let ship = step_readiness
        .iter()
        .find(|s| s.name == "ship_order")
        .unwrap();
    assert!(ship.dependencies_satisfied);
    assert!(ship.ready_for_execution);

    manager
        .validate_task_execution_context(task_uuid, 4, 3, 1)
        .await?;

    tracing::info!("✅ Payment recovery unblocks shipping");

    // **PHASE 3**: Complete ship_order
    tracing::info!("📊 PHASE 3: Completing ship_order");
    manager
        .complete_step(
            task_uuid,
            "ship_order",
            serde_json::json!({
                "shipment_created": true,
                "tracking_number": "TRK-2025-EXPR-001"
            }),
        )
        .await?;

    // **VALIDATION 3**: All steps complete
    manager
        .validate_task_execution_context(task_uuid, 4, 4, 0)
        .await?;

    let final_readiness = manager.get_step_readiness_status(task_uuid).await?;
    for step in &final_readiness {
        assert_eq!(step.current_state.as_deref(), Some("complete"));
    }

    tracing::info!("✅ Transient failure recovered, order fulfillment completed successfully");
    tracing::info!("🎯 DELAYED GRATIFICATION: Transient failure eventual completion test complete");

    Ok(())
}
