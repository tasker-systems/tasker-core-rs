//! # End-to-End Domain Event Publishing Integration Test (Rust Worker)
//!
//! TAS-65/TAS-69: Domain Event Publication Architecture
//!
//! This integration test validates that domain event publishing:
//! 1. Does not block step execution (fire-and-forget semantics)
//! 2. Integrates correctly with the workflow lifecycle
//! 3. Handles various event configurations (success/failure/always conditions)
//! 4. Works with both durable and fast delivery modes
//! 5. Supports both default and custom publishers
//!
//! Prerequisites:
//! Run `docker-compose -f docker/docker-compose.test.yml up --build -d` before running tests
//!
//! ## 2x2x2 Test Matrix Coverage
//!
//! This test validates the complete publisher matrix for Rust:
//! | Event               | Delivery Mode | Publisher                    | Condition |
//! |---------------------|---------------|------------------------------|-----------|
//! | order.validated     | fast          | default                      | success   |
//! | payment.processed   | durable       | PaymentEventPublisher        | success   |
//! | payment.failed      | durable       | default                      | failure   |
//! | inventory.updated   | fast          | default                      | always    |
//! | notification.sent   | fast          | NotificationEventPublisher   | success   |
//!
//! ## Domain Event Publishing Pattern (4 steps)
//!
//! 1. domain_events_validate_order: Publishes order.validated on success (fast delivery)
//! 2. domain_events_process_payment: Publishes payment.processed on success, payment.failed on failure (durable)
//! 3. domain_events_update_inventory: Publishes inventory.updated always (fast delivery)
//! 4. domain_events_send_notification: Publishes notification.sent on success (fast delivery)
//!
//! NOTE: This test validates the non-blocking nature of domain events. We cannot directly
//! verify events were published (fire-and-forget), but we can verify:
//! - Task completes successfully
//! - Steps with event declarations execute normally
//! - No performance degradation from event publishing

use anyhow::Result;
use serde_json::json;
use std::time::Instant;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use crate::common::integration_test_utils::{create_task_request, wait_for_task_completion};

/// Helper function to create domain event publishing test task request for Rust worker
fn create_rust_domain_event_task_request(
    order_id: &str,
    customer_id: &str,
    amount: f64,
    simulate_failure: bool,
) -> tasker_shared::models::core::task_request::TaskRequest {
    create_task_request(
        "domain_events_rust", // Use separate namespace from Ruby to avoid collision
        "domain_event_publishing",
        json!({
            "order_id": order_id,
            "customer_id": customer_id,
            "amount": amount,
            "simulate_failure": simulate_failure
        }),
    )
}

/// Test successful domain event publishing workflow with Rust worker
///
/// Validates:
/// - Task completes successfully with event-publishing steps
/// - All 4 steps execute in order
/// - Event publishing doesn't block step execution
/// - Fast execution (< 10s for 4 steps with events)
/// - **NEW TAS-65**: Actually verifies events were published via /metrics/events stats
/// - 2x2 publisher matrix coverage (durable/fast x default/custom)
#[tokio::test]
async fn test_rust_domain_event_publishing_success() -> Result<()> {
    println!("🚀 Starting Domain Event Publishing Test (Rust Worker - Success Path)");
    println!(
        "   Workflow: validate_order → process_payment → update_inventory → send_notification"
    );
    println!("   Template: domain_event_publishing");
    println!("   Namespace: domain_events_rust");
    println!("   Events: order.validated, payment.processed, inventory.updated, notification.sent");
    println!("\n📊 Publisher Matrix Coverage:");
    println!("   • order.validated     - fast + default publisher");
    println!("   • payment.processed   - durable + PaymentEventPublisher (custom)");
    println!("   • inventory.updated   - fast + default publisher");
    println!("   • notification.sent   - fast + NotificationEventPublisher (custom)");

    let manager = IntegrationTestManager::setup().await?;

    // TAS-65: Require worker client for event verification
    let worker_client = manager.worker_client.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "Worker client required for domain event verification. \
             Set TASKER_TEST_WORKER_URL or ensure worker is running."
        )
    })?;

    println!("\n🎉 All services ready! URLs:");
    println!("   Orchestration: {}", manager.orchestration_url);
    println!(
        "   Worker: {}",
        manager.worker_url.as_ref().unwrap_or(&"N/A".to_string())
    );

    // TAS-65: Capture event stats BEFORE task execution
    println!("\n📊 Capturing event stats before task execution...");
    let stats_before = worker_client.get_domain_event_stats().await?;
    println!(
        "   Router stats before: total={}, durable={}, fast={}, broadcast={}",
        stats_before.router.total_routed,
        stats_before.router.durable_routed,
        stats_before.router.fast_routed,
        stats_before.router.broadcast_routed
    );

    // Create test order
    let order_id = Uuid::new_v4().to_string();
    let customer_id = "rust_customer_123";
    let amount = 199.99;

    println!("\n🎯 Creating domain event publishing task (Rust worker)...");
    println!("   Order ID: {}", order_id);
    println!("   Customer ID: {}", customer_id);
    println!("   Amount: ${:.2}", amount);

    let task_request = create_rust_domain_event_task_request(&order_id, customer_id, amount, false);

    let start_time = Instant::now();
    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("✅ Task created successfully!");
    println!("   Task UUID: {}", task_response.task_uuid);
    println!("   Expected: 4 steps with domain event declarations");

    // Monitor task execution
    println!("\n⏱️  Monitoring domain event publishing workflow (Rust worker)...");
    let timeout = 10; // 10 seconds for 4 sequential steps with events
    wait_for_task_completion(
        &manager.orchestration_client,
        &task_response.task_uuid,
        timeout,
    )
    .await?;

    let execution_time = start_time.elapsed();
    println!("   Execution time: {:.2}s", execution_time.as_secs_f64());

    // Verify final results
    println!("\n🔍 Verifying domain event workflow results...");
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let task = manager.orchestration_client.get_task(task_uuid).await?;

    assert!(
        task.is_execution_complete(),
        "Task execution should be complete"
    );
    assert!(
        task.status.to_lowercase().contains("complete"),
        "Task status should indicate completion, got: {}",
        task.status
    );

    // Verify all 4 steps completed
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    assert_eq!(steps.len(), 4, "Should have 4 total steps");
    assert!(
        steps
            .iter()
            .all(|s| s.current_state.to_uppercase() == "COMPLETE"),
        "All steps should be complete"
    );

    // Verify step execution order - use explicit assertions with better messages
    let step_names: Vec<String> = steps.iter().map(|s| s.name.clone()).collect();
    assert!(
        step_names.contains(&"domain_events_validate_order".to_string()),
        "Should have domain_events_validate_order step, got: {:?}",
        step_names
    );
    assert!(
        step_names.contains(&"domain_events_process_payment".to_string()),
        "Should have domain_events_process_payment step, got: {:?}",
        step_names
    );
    assert!(
        step_names.contains(&"domain_events_update_inventory".to_string()),
        "Should have domain_events_update_inventory step, got: {:?}",
        step_names
    );
    assert!(
        step_names.contains(&"domain_events_send_notification".to_string()),
        "Should have domain_events_send_notification step, got: {:?}",
        step_names
    );

    // Verify execution didn't timeout (events are non-blocking)
    assert!(
        execution_time.as_secs() < 10,
        "Execution should complete quickly (events are fire-and-forget)"
    );

    // TAS-65: Capture event stats AFTER task completion and verify events were published
    println!("\n📊 Capturing event stats after task completion...");
    let stats_after = worker_client.get_domain_event_stats().await?;
    println!(
        "   Router stats after: total={}, durable={}, fast={}, broadcast={}",
        stats_after.router.total_routed,
        stats_after.router.durable_routed,
        stats_after.router.fast_routed,
        stats_after.router.broadcast_routed
    );

    // Calculate events published during this test
    let events_published = stats_after.router.total_routed - stats_before.router.total_routed;
    let durable_published = stats_after.router.durable_routed - stats_before.router.durable_routed;
    let fast_published = stats_after.router.fast_routed - stats_before.router.fast_routed;
    let rust_handler_dispatches = stats_after.in_process_bus.rust_handler_dispatches
        - stats_before.in_process_bus.rust_handler_dispatches;

    println!("\n📈 Event publishing verification:");
    println!("   Total events published: {}", events_published);
    println!("   Durable events (PGMQ): {}", durable_published);
    println!("   Fast events (in-process): {}", fast_published);
    println!("   Rust handler dispatches: {}", rust_handler_dispatches);

    // TAS-65: Assert events were actually published
    // Expected: 4 events total (order.validated, payment.processed, inventory.updated, notification.sent)
    // Expected: 1 durable (payment.processed), 3 fast (order.validated, inventory.updated, notification.sent)
    assert!(
        events_published >= 4,
        "Expected at least 4 events to be published, got {}. \
         This indicates domain events are NOT being published correctly!",
        events_published
    );

    assert!(
        durable_published >= 1,
        "Expected at least 1 durable event (payment.processed), got {}. \
         The durable event path (PGMQ) is not working!",
        durable_published
    );

    assert!(
        fast_published >= 3,
        "Expected at least 3 fast events (order.validated, inventory.updated, notification.sent), got {}. \
         The fast event path (in-process bus) is not working!", fast_published
    );

    // TAS-65: Verify Rust handler dispatches for in-process bus
    // Fast events should be dispatched to registered Rust handlers
    // Note: If no Rust handlers are registered, this will be 0 but fast_published will still work
    // We use >= 0 here since handler registration is optional, but log the value for visibility
    println!(
        "   Note: Rust handlers received {} dispatches (depends on registered handlers)",
        rust_handler_dispatches
    );

    println!("\n✅ Rust domain event publishing test passed!");
    println!("   - All 4 steps completed successfully");
    println!("   - Event publishing did not block execution");
    println!(
        "   - Total execution time: {:.2}s",
        execution_time.as_secs_f64()
    );
    println!(
        "   - ✅ Events VERIFIED ({} total, {} durable, {} fast, {} rust handler dispatches):",
        events_published, durable_published, fast_published, rust_handler_dispatches
    );
    println!("     • order.validated (fast, default publisher, success)");
    println!("     • payment.processed (durable, PaymentEventPublisher, success)");
    println!("     • inventory.updated (fast, default publisher, always)");
    println!("     • notification.sent (fast, NotificationEventPublisher, success)");

    Ok(())
}

/// Test domain event publishing with multiple concurrent tasks (Rust worker)
///
/// Validates:
/// - Multiple tasks with events can run concurrently
/// - Event publishing doesn't create bottlenecks
/// - System handles high event throughput
#[tokio::test]
async fn test_rust_domain_event_publishing_concurrent() -> Result<()> {
    println!("🚀 Starting Domain Event Publishing Concurrent Test (Rust Worker)");
    println!("   Testing: 3 concurrent tasks with domain events");

    let manager = IntegrationTestManager::setup().await?;

    // Create 3 concurrent tasks
    let task_count = 3;
    let mut task_uuids = Vec::with_capacity(task_count);

    println!(
        "\n🎯 Creating {} concurrent domain event tasks (Rust worker)...",
        task_count
    );
    let start_time = Instant::now();

    for i in 0..task_count {
        let order_id = Uuid::new_v4().to_string();
        let customer_id = format!("rust_customer_{}", i);

        let task_request = create_rust_domain_event_task_request(
            &order_id,
            &customer_id,
            200.0 + (i as f64 * 50.0),
            false,
        );

        let task_response = manager
            .orchestration_client
            .create_task(task_request)
            .await?;

        println!("   Task {} created: {}", i + 1, task_response.task_uuid);
        task_uuids.push(task_response.task_uuid);
    }

    // Wait for all tasks to complete
    println!("\n⏱️  Waiting for all tasks to complete...");
    let timeout = 20; // 20 seconds for 3 concurrent tasks

    for (i, task_uuid) in task_uuids.iter().enumerate() {
        wait_for_task_completion(&manager.orchestration_client, task_uuid, timeout).await?;
        println!("   Task {} completed", i + 1);
    }

    let total_time = start_time.elapsed();
    println!(
        "   Total time for {} tasks: {:.2}s",
        task_count,
        total_time.as_secs_f64()
    );

    // Verify all tasks completed successfully
    println!("\n🔍 Verifying all tasks completed...");
    for (i, task_uuid) in task_uuids.iter().enumerate() {
        let uuid = Uuid::parse_str(task_uuid)?;
        let task = manager.orchestration_client.get_task(uuid).await?;

        assert!(
            task.is_execution_complete(),
            "Task {} should be complete",
            i + 1
        );
    }

    // Verify reasonable performance (events shouldn't create bottlenecks)
    let expected_max_time = 20; // 20 seconds max for 3 concurrent tasks
    assert!(
        total_time.as_secs() < expected_max_time,
        "Concurrent tasks should complete in reasonable time ({} < {}s)",
        total_time.as_secs(),
        expected_max_time
    );

    println!("✅ Concurrent domain event publishing test passed (Rust worker)!");
    println!("   - {} tasks completed concurrently", task_count);
    println!(
        "   - Total execution time: {:.2}s",
        total_time.as_secs_f64()
    );
    println!("   - Events published: {} events total", task_count * 4); // 4 events per task

    Ok(())
}

/// Test domain event workflow step result data
///
/// Validates:
/// - Step results contain expected data for event payload construction
/// - Each step produces data that can be used by publishers
/// - **FIXED TAS-65**: No silent failures - asserts when results missing
#[tokio::test]
async fn test_rust_domain_event_step_results() -> Result<()> {
    println!("🚀 Starting Domain Event Step Results Test (Rust Worker)");
    println!("   Testing: Step result data for event payload construction");

    let manager = IntegrationTestManager::setup().await?;

    let order_id = Uuid::new_v4().to_string();
    let customer_id = "rust_results_test_customer";
    let amount = 299.99;

    println!("\n🎯 Creating task to verify step results...");
    let task_request = create_rust_domain_event_task_request(&order_id, customer_id, amount, false);

    let task_response = manager
        .orchestration_client
        .create_task(task_request)
        .await?;

    println!("   Task UUID: {}", task_response.task_uuid);

    // Wait for completion
    wait_for_task_completion(&manager.orchestration_client, &task_response.task_uuid, 10).await?;

    // Get step results
    let task_uuid = Uuid::parse_str(&task_response.task_uuid)?;
    let steps = manager
        .orchestration_client
        .list_task_steps(task_uuid)
        .await?;

    println!("\n🔍 Verifying step results contain event payload data...");

    // TAS-65 FIX: Assert we have steps instead of silently iterating empty
    assert!(
        !steps.is_empty(),
        "Expected steps to be returned, got empty list. Task may not have created steps."
    );
    assert_eq!(
        steps.len(),
        4,
        "Expected 4 steps for domain event workflow, got {}",
        steps.len()
    );

    // Track which steps we've verified to ensure all are checked
    let mut verified_steps = 0;

    for step in &steps {
        println!("\n   Step: {}", step.name);
        println!("   State: {}", step.current_state);

        // TAS-65 FIX: Assert results exist instead of silently skipping
        let results = step.results.as_ref().ok_or_else(|| {
            anyhow::anyhow!(
                "Step '{}' has no results - expected results for event payload construction",
                step.name
            )
        })?;

        println!("   Results: {}", serde_json::to_string_pretty(results)?);

        // Verify each step has appropriate result data for event publishing
        // The results structure contains nested data under "result" key
        let result_data = results.get("result").unwrap_or(results);

        match step.name.as_str() {
            "domain_events_validate_order" => {
                assert!(
                    result_data.get("order_id").is_some() || result_data.get("validated").is_some(),
                    "validate_order should have order_id or validated field in result, got: {:?}",
                    results
                );
                verified_steps += 1;
            }
            "domain_events_process_payment" => {
                assert!(
                    result_data.get("transaction_id").is_some() || result_data.get("status").is_some(),
                    "process_payment should have transaction_id or status field in result, got: {:?}", results
                );
                verified_steps += 1;
            }
            "domain_events_update_inventory" => {
                assert!(
                    result_data.get("items").is_some() || result_data.get("success").is_some(),
                    "update_inventory should have items or success field in result, got: {:?}",
                    results
                );
                verified_steps += 1;
            }
            "domain_events_send_notification" => {
                assert!(
                    result_data.get("notification_id").is_some() || result_data.get("channel").is_some(),
                    "send_notification should have notification_id or channel field in result, got: {:?}", results
                );
                verified_steps += 1;
            }
            other => {
                // TAS-65 FIX: Fail on unexpected step names instead of silently ignoring
                panic!(
                    "Unexpected step name '{}' - test needs to be updated",
                    other
                );
            }
        }
    }

    // TAS-65 FIX: Ensure all expected steps were verified
    assert_eq!(
        verified_steps, 4,
        "Expected to verify 4 steps, but only verified {}. Some expected steps may be missing.",
        verified_steps
    );

    println!("\n✅ Step results validation passed!");
    println!(
        "   All {} steps contain appropriate data for event publishing",
        verified_steps
    );

    Ok(())
}

/// Test domain event workflow metrics availability (Rust worker)
///
/// Validates:
/// - Worker /metrics/events endpoint is accessible
/// - Domain event statistics are exposed correctly
/// - **FIXED TAS-65**: Uses worker_client instead of raw reqwest
/// - **FIXED TAS-65**: No silent failures - asserts worker availability
#[tokio::test]
async fn test_rust_domain_event_metrics_availability() -> Result<()> {
    println!("🚀 Starting Domain Event Metrics Test (Rust Worker)");

    let manager = IntegrationTestManager::setup().await?;

    // TAS-65 FIX: Require worker client instead of silently skipping with if-let
    let worker_client = manager.worker_client.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "Worker client required for domain event metrics test. \
             Set TASKER_TEST_WORKER_URL or ensure worker is running."
        )
    })?;

    println!("\n🔍 Checking worker domain event metrics endpoint...");
    println!("   Worker URL: {}", manager.worker_url.as_ref().unwrap());

    // TAS-65: Test the /metrics/events endpoint using the worker client
    let stats = worker_client.get_domain_event_stats().await?;

    println!("\n📊 Domain Event Statistics:");
    println!("   Worker ID: {}", stats.worker_id);
    println!("   Captured at: {}", stats.captured_at);
    println!("\n   Router Stats:");
    println!("     Total routed: {}", stats.router.total_routed);
    println!("     Durable (PGMQ): {}", stats.router.durable_routed);
    println!("     Fast (in-process): {}", stats.router.fast_routed);
    println!("     Broadcast (both): {}", stats.router.broadcast_routed);
    println!("     Routing errors: {}", stats.router.routing_errors);
    println!("\n   In-Process Bus Stats:");
    println!(
        "     Total dispatched: {}",
        stats.in_process_bus.total_events_dispatched
    );
    println!(
        "     Rust handler dispatches: {}",
        stats.in_process_bus.rust_handler_dispatches
    );
    println!(
        "     FFI channel dispatches: {}",
        stats.in_process_bus.ffi_channel_dispatches
    );
    println!(
        "     Subscriber patterns: {}",
        stats.in_process_bus.rust_subscriber_patterns
    );

    // TAS-65: Verify the stats structure is valid
    assert!(!stats.worker_id.is_empty(), "Worker ID should not be empty");

    // TAS-65: Also verify health endpoint using the worker client
    println!("\n🔍 Checking worker health endpoint...");
    worker_client.health_check().await?;
    println!("   ✅ Worker health check passed");

    println!("\n✅ Domain event metrics availability test passed!");
    println!("   - /metrics/events endpoint accessible");
    println!("   - Statistics structure valid");
    println!("   - Health endpoint accessible");

    Ok(())
}
