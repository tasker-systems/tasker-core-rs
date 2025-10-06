use anyhow::Result;
use sqlx::PgPool;

use crate::common::lifecycle_test_manager::LifecycleTestManager;

/// Test order fulfillment with validation permanent failure blocking entire workflow
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_order_fulfillment_validation_permanent_failure(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 SORROWFUL FAILURE: Validation permanent failure blocks entire workflow");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "business_workflow",
        "rust_e2e_order_fulfillment",
        serde_json::json!({
            "customer": {"id": -1, "name": "", "email": "invalid"},
            "items": [],
            "payment": {"method": "invalid", "token": ""},
            "shipping": {"method": "invalid", "address": {}}
        }),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Fail validate_order permanently
    tracing::info!("📊 PHASE 1: Failing validate_order permanently");
    for _ in 0..3 {
        manager
            .fail_step(
                task_uuid,
                "validate_order",
                "Critical validation errors: invalid customer, no items, invalid payment",
            )
            .await?;
    }

    // Validate: validate_order in terminal error, entire workflow blocked
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let validate = step_readiness
        .iter()
        .find(|s| s.name == "validate_order")
        .unwrap();
    assert_eq!(validate.current_state.as_deref(), Some("error"));
    assert_eq!(validate.attempts, 3);
    assert!(!validate.retry_eligible);

    // All downstream steps remain pending, never executable
    for step_name in ["reserve_inventory", "process_payment", "ship_order"] {
        let step = step_readiness.iter().find(|s| s.name == step_name).unwrap();
        assert_eq!(step.current_state.as_deref(), Some("pending"));
        assert!(!step.dependencies_satisfied);
        assert!(!step.ready_for_execution);
    }

    manager
        .validate_task_execution_context(task_uuid, 4, 0, 0)
        .await?;

    tracing::info!("✅ Validation permanent failure permanently blocks entire workflow");
    tracing::info!("🎯 SORROWFUL FAILURE: Validation permanent failure test complete");

    Ok(())
}

/// Test order fulfillment with inventory permanent failure
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_order_fulfillment_inventory_permanent_failure(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 SORROWFUL FAILURE: Inventory permanent failure");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "business_workflow",
        "rust_e2e_order_fulfillment",
        serde_json::json!({
            "customer": {"id": 12345, "name": "Jane Doe", "email": "jane.doe@example.com"},
            "items": [{"sku": "DISCONTINUED", "quantity": 1, "price": 99.99}],
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
            serde_json::json!({"customer_valid": true, "items_valid": true, "total": 99.99}),
        )
        .await?;

    // **PHASE 2**: Fail reserve_inventory permanently
    tracing::info!("📊 PHASE 2: Failing reserve_inventory permanently");
    for _ in 0..3 {
        manager
            .fail_step(
                task_uuid,
                "reserve_inventory",
                "Item discontinued - no inventory available",
            )
            .await?;
    }

    // Validate: reserve_inventory failed, downstream blocked
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let reserve = step_readiness
        .iter()
        .find(|s| s.name == "reserve_inventory")
        .unwrap();
    assert_eq!(reserve.current_state.as_deref(), Some("error"));
    assert!(!reserve.retry_eligible);

    // Payment and shipping blocked
    for step_name in ["process_payment", "ship_order"] {
        let step = step_readiness.iter().find(|s| s.name == step_name).unwrap();
        assert_eq!(step.current_state.as_deref(), Some("pending"));
        assert!(!step.dependencies_satisfied);
    }

    manager
        .validate_task_execution_context(task_uuid, 4, 1, 0)
        .await?;

    tracing::info!("✅ Inventory permanent failure blocks payment and shipping");
    tracing::info!("🎯 SORROWFUL FAILURE: Inventory permanent failure test complete");

    Ok(())
}

/// Test order fulfillment with payment permanent failure
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_order_fulfillment_payment_permanent_failure(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 SORROWFUL FAILURE: Payment permanent failure");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "business_workflow",
        "rust_e2e_order_fulfillment",
        serde_json::json!({
            "customer": {"id": 12345, "name": "Jane Doe", "email": "jane.doe@example.com"},
            "items": [{"sku": "WIDGET-001", "quantity": 2, "price": 29.99}],
            "payment": {"method": "credit_card", "token": "tok_declined"},
            "shipping": {"method": "standard", "address": {}}
        }),
    );

    let init_result = manager.initialize_task(task_request).await?;
    let task_uuid = init_result.task_uuid;

    // **PHASE 1**: Complete validate and reserve
    tracing::info!("📊 PHASE 1: Completing validate_order and reserve_inventory");
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

    // **PHASE 2**: Fail process_payment permanently (retry limit of 2)
    tracing::info!("📊 PHASE 2: Failing process_payment permanently");
    for _ in 0..2 {
        manager
            .fail_step(
                task_uuid,
                "process_payment",
                "Payment declined - insufficient funds",
            )
            .await?;
    }

    // Validate: payment in terminal error, shipping permanently blocked
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    let payment = step_readiness
        .iter()
        .find(|s| s.name == "process_payment")
        .unwrap();
    assert_eq!(payment.current_state.as_deref(), Some("error"));
    assert_eq!(payment.attempts, 2); // Payment has retry limit of 2
    assert!(!payment.retry_eligible);

    let ship = step_readiness
        .iter()
        .find(|s| s.name == "ship_order")
        .unwrap();
    assert!(!ship.dependencies_satisfied);
    assert!(!ship.ready_for_execution);

    manager
        .validate_task_execution_context(task_uuid, 4, 2, 0)
        .await?;

    tracing::info!("✅ Payment permanent failure blocks shipping");
    tracing::info!("🎯 SORROWFUL FAILURE: Payment permanent failure test complete");

    Ok(())
}

/// Test order fulfillment with mixed states and SQL accuracy
#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_order_fulfillment_mixed_states_sql_accuracy(pool: PgPool) -> Result<()> {
    tracing::info!("🔍 SORROWFUL FAILURE: Mixed states SQL accuracy");

    let manager = LifecycleTestManager::new(pool.clone()).await?;

    let task_request = manager.create_task_request_for_template(
        "business_workflow",
        "rust_e2e_order_fulfillment",
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
    manager
        .complete_step(
            task_uuid,
            "validate_order",
            serde_json::json!({"customer_valid": true, "items_valid": true, "total": 59.98}),
        )
        .await?;

    // **PHASE 2**: Fail reserve_inventory (transient error)
    manager
        .fail_step(
            task_uuid,
            "reserve_inventory",
            "Temporary inventory service outage",
        )
        .await?;

    // **VALIDATION**: Mixed state verification
    let step_readiness = manager.get_step_readiness_status(task_uuid).await?;

    // validate_order: complete
    let validate = step_readiness
        .iter()
        .find(|s| s.name == "validate_order")
        .unwrap();
    assert_eq!(validate.current_state.as_deref(), Some("complete"));

    // reserve_inventory: error (with retry eligibility)
    let reserve = step_readiness
        .iter()
        .find(|s| s.name == "reserve_inventory")
        .unwrap();
    assert_eq!(reserve.current_state.as_deref(), Some("error"));
    assert!(reserve.retry_eligible);

    // process_payment: pending (blocked by reserve_inventory error)
    let payment = step_readiness
        .iter()
        .find(|s| s.name == "process_payment")
        .unwrap();
    assert_eq!(payment.current_state.as_deref(), Some("pending"));
    assert_eq!(payment.completed_parents, 1); // only validate_order
    assert!(!payment.dependencies_satisfied);

    // ship_order: pending (blocked by incomplete dependencies)
    let ship = step_readiness
        .iter()
        .find(|s| s.name == "ship_order")
        .unwrap();
    assert_eq!(ship.current_state.as_deref(), Some("pending"));
    assert_eq!(ship.completed_parents, 1); // only validate_order
    assert!(!ship.dependencies_satisfied);

    manager
        .validate_task_execution_context(task_uuid, 4, 1, 0)
        .await?;

    tracing::info!("✅ SQL functions accurately reflect mixed state business workflow");
    tracing::info!("🎯 SORROWFUL FAILURE: Mixed states SQL accuracy test complete");

    Ok(())
}
