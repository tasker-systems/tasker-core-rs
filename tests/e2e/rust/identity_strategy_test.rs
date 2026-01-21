//! # Identity Strategy E2E Tests (TAS-154)
//!
//! Tests the task identity strategy pattern behavior via the API.
//!
//! ## What This Tests
//!
//! - **409 Conflict**: Duplicate identity hash correctly returns 409 (not 500)
//! - **STRICT Strategy**: Same context + named_task produces same identity hash
//! - **Unique Contexts**: Different contexts produce different identity hashes
//!
//! ## Prerequisites
//!
//! Run `docker-compose -f docker/docker-compose.test.yml up --build -d` before running tests

use anyhow::Result;
use uuid::Uuid;

use crate::common::integration_test_manager::IntegrationTestManager;
use tasker_shared::models::core::task_request::TaskRequest;

/// Create a TaskRequest WITHOUT the unique test_run_id injection.
/// This allows us to test identity hash deduplication behavior.
fn create_deterministic_task_request(
    namespace: &str,
    name: &str,
    context: serde_json::Value,
) -> TaskRequest {
    TaskRequest {
        namespace: namespace.to_string(),
        name: name.to_string(),
        version: "1.0.0".to_string(),
        context,
        correlation_id: Uuid::now_v7(),
        parent_correlation_id: None,
        initiator: "identity-strategy-test".to_string(),
        source_system: "test".to_string(),
        reason: "Identity strategy E2E test".to_string(),
        tags: vec!["identity-test".to_string()],
        requested_at: chrono::Utc::now().naive_utc(),
        options: None,
        priority: Some(5),
        idempotency_key: None,
    }
}

/// Test that duplicate identity hash returns 409 Conflict (TAS-154)
///
/// This verifies the security-conscious behavior: when a task with the same
/// identity hash already exists, the API returns 409 Conflict rather than:
/// - Returning the existing task UUID (data leakage risk)
/// - Returning 500 Internal Server Error (poor UX)
///
/// The STRICT strategy (default) computes identity hash from:
/// - named_task_uuid (from namespace + name + version lookup)
/// - normalized context JSON
#[tokio::test]
async fn test_duplicate_identity_hash_returns_409_conflict() -> Result<()> {
    println!("🔒 Testing 409 Conflict on Duplicate Identity Hash (TAS-154)");

    let manager = IntegrationTestManager::setup_orchestration_only().await?;

    // Use a unique context for this test run to avoid conflicts with other tests
    let unique_test_id = Uuid::now_v7().to_string();
    let context = serde_json::json!({
        "test_id": unique_test_id,
        "even_number": 6
    });

    // Create first task - should succeed
    println!("\n  📋 Creating first task with deterministic context...");
    let request1 = create_deterministic_task_request(
        "rust_e2e_linear",
        "mathematical_sequence",
        context.clone(),
    );

    let response1 = manager.orchestration_client.create_task(request1).await?;
    println!("     ✅ First task created: {}", response1.task_uuid);

    // Create second task with IDENTICAL context - should get 409 Conflict
    println!("\n  📋 Creating second task with identical context...");
    let request2 = create_deterministic_task_request(
        "rust_e2e_linear",
        "mathematical_sequence",
        context.clone(),
    );

    let result = manager.orchestration_client.create_task(request2).await;

    match result {
        Ok(response) => {
            panic!(
                "Expected 409 Conflict but got success with task_uuid: {}. \
                 This is a security concern - duplicate identity should be rejected.",
                response.task_uuid
            );
        }
        Err(e) => {
            let error_string = e.to_string();
            println!("     ✅ Received expected error: {}", error_string);

            // Verify it's a 409 Conflict (not 500 Internal Server Error)
            assert!(
                error_string.contains("409"),
                "Expected HTTP 409 status code, got: {}",
                error_string
            );

            // Verify the error message indicates it's a duplicate identity issue
            assert!(
                error_string.contains("identity") || error_string.contains("exists"),
                "Error should mention identity or existence, got: {}",
                error_string
            );

            println!("     ✅ Correctly received 409 Conflict for duplicate identity hash");
        }
    }

    println!("\n🎉 409 Conflict test passed!");
    Ok(())
}

/// Test that different contexts produce different identity hashes
///
/// This verifies that two tasks with different contexts (but same namespace/name)
/// can be created successfully because they have different identity hashes.
#[tokio::test]
async fn test_different_contexts_succeed() -> Result<()> {
    println!("🔐 Testing Different Contexts Produce Different Identity Hashes (TAS-154)");

    let manager = IntegrationTestManager::setup_orchestration_only().await?;

    // Use unique test IDs to ensure these don't conflict with other tests
    let base_test_id = Uuid::now_v7().to_string();

    // Create first task with context A
    println!("\n  📋 Creating task with context A...");
    let context_a = serde_json::json!({
        "test_id": format!("{}-A", base_test_id),
        "even_number": 6
    });
    let request_a =
        create_deterministic_task_request("rust_e2e_linear", "mathematical_sequence", context_a);
    let response_a = manager.orchestration_client.create_task(request_a).await?;
    println!("     ✅ Task A created: {}", response_a.task_uuid);

    // Create second task with context B (different test_id)
    println!("\n  📋 Creating task with context B (different context)...");
    let context_b = serde_json::json!({
        "test_id": format!("{}-B", base_test_id),
        "even_number": 6
    });
    let request_b =
        create_deterministic_task_request("rust_e2e_linear", "mathematical_sequence", context_b);
    let response_b = manager.orchestration_client.create_task(request_b).await?;
    println!("     ✅ Task B created: {}", response_b.task_uuid);

    // Verify different UUIDs
    assert_ne!(
        response_a.task_uuid, response_b.task_uuid,
        "Different contexts should produce different task UUIDs"
    );
    println!("\n     ✅ Confirmed: Different contexts = different task UUIDs");

    println!("\n🎉 Different contexts test passed!");
    Ok(())
}

/// Test that JSON key ordering doesn't affect identity hash (normalization)
///
/// The identity hash uses normalized JSON, so {"a":1,"b":2} and {"b":2,"a":1}
/// should produce the same identity hash.
#[tokio::test]
async fn test_json_key_order_normalization() -> Result<()> {
    println!("🔄 Testing JSON Key Order Normalization (TAS-154)");

    let manager = IntegrationTestManager::setup_orchestration_only().await?;

    // Use a unique base test ID
    let unique_test_id = Uuid::now_v7().to_string();

    // Create first task with keys in order A, B
    println!("\n  📋 Creating task with keys {{\"a\":..., \"b\":...}}...");
    let context_ab = serde_json::json!({
        "a_key": format!("{}-normalization", unique_test_id),
        "b_key": "value_b",
        "even_number": 6
    });
    let request_ab =
        create_deterministic_task_request("rust_e2e_linear", "mathematical_sequence", context_ab);
    let response_ab = manager.orchestration_client.create_task(request_ab).await?;
    println!(
        "     ✅ Task (a,b order) created: {}",
        response_ab.task_uuid
    );

    // Create second task with keys in order B, A (same semantic content)
    println!("\n  📋 Creating task with keys {{\"b\":..., \"a\":...}} (same content, different order)...");

    // Build the JSON with a different key order using a map
    let mut map = serde_json::Map::new();
    map.insert("b_key".to_string(), serde_json::json!("value_b"));
    map.insert(
        "a_key".to_string(),
        serde_json::json!(format!("{}-normalization", unique_test_id)),
    );
    map.insert("even_number".to_string(), serde_json::json!(6));
    let context_ba = serde_json::Value::Object(map);

    let request_ba =
        create_deterministic_task_request("rust_e2e_linear", "mathematical_sequence", context_ba);
    let result = manager.orchestration_client.create_task(request_ba).await;

    match result {
        Ok(response) => {
            // If we got here, the normalization is NOT working - this is unexpected
            // but let's check if they're different UUIDs
            if response.task_uuid == response_ab.task_uuid {
                panic!(
                    "Both requests returned the same task UUID, but the second should have failed with 409"
                );
            } else {
                // This means key order affected the hash - normalization bug
                panic!(
                    "Key order affected identity hash. First: {}, Second: {} (expected 409 Conflict)",
                    response_ab.task_uuid, response.task_uuid
                );
            }
        }
        Err(e) => {
            let error_string = e.to_string();
            // Should be 409 because key order should NOT matter
            assert!(
                error_string.contains("409"),
                "Expected 409 (same content, different key order), got: {}",
                error_string
            );
            println!("     ✅ Got 409 Conflict - JSON key order correctly normalized!");
        }
    }

    println!("\n🎉 JSON key order normalization test passed!");
    Ok(())
}
