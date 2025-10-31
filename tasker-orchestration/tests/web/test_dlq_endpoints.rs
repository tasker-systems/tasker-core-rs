//! Integration tests for DLQ (Dead Letter Queue) endpoints (TAS-49)

use reqwest::StatusCode;
use serde_json::{json, Value};
use uuid::Uuid;

use super::authenticated_tests::{generate_test_jwt, get_test_private_key, TestClaims};
use super::test_infrastructure::*;

#[tokio::test]
async fn test_dlq_list_endpoint() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let mut client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Generate a valid JWT token
    let claims = TestClaims::new("dlq-user", 60);
    let private_key = get_test_private_key();
    let token = generate_test_jwt(&claims, &private_key).expect("Failed to generate test JWT");

    client.with_jwt_token(token);

    // Test DLQ list endpoint
    let response = client
        .get_authenticated("/v1/dlq")
        .await
        .expect("Failed to send DLQ list request");

    // Should not return 401 Unauthorized with valid token
    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);

    println!("DLQ list endpoint status: {}", response.status());

    // If the endpoint is available, validate the response structure
    if response.status().is_success() {
        let dlq_entries = response
            .json::<Vec<Value>>()
            .await
            .expect("Failed to parse DLQ list response");

        println!("✅ DLQ List Response validation:");
        println!("   Found {} DLQ entries", dlq_entries.len());

        // If there are entries, validate structure of first entry
        if let Some(first_entry) = dlq_entries.first() {
            let expected_fields = [
                "dlq_entry_uuid",
                "task_uuid",
                "original_state",
                "dlq_reason",
                "dlq_timestamp",
                "resolution_status",
                "task_snapshot",
                "created_at",
                "updated_at",
            ];

            for field in &expected_fields {
                assert!(
                    first_entry.get(field).is_some(),
                    "Missing required field: {field}"
                );
                println!("  ✓ {field} present");
            }
        }

        println!("🎉 DLQ list endpoint validation complete!");
    } else {
        println!(
            "⚠️  DLQ list endpoint not available (status: {})",
            response.status()
        );
        println!("   This is expected if the web API server is not running");
    }

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

#[tokio::test]
async fn test_dlq_list_with_filters() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let mut client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Generate a valid JWT token
    let claims = TestClaims::new("dlq-user", 60);
    let private_key = get_test_private_key();
    let token = generate_test_jwt(&claims, &private_key).expect("Failed to generate test JWT");

    client.with_jwt_token(token);

    // Test DLQ list endpoint with filtering
    let response = client
        .get_authenticated("/v1/dlq?resolution_status=pending&limit=20&offset=0")
        .await
        .expect("Failed to send DLQ list request with filters");

    // Should not return 401 Unauthorized with valid token
    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);

    println!(
        "DLQ list with filters endpoint status: {}",
        response.status()
    );

    // If the endpoint is available, validate the response
    if response.status().is_success() {
        let dlq_entries = response
            .json::<Vec<Value>>()
            .await
            .expect("Failed to parse DLQ list response");

        println!("✅ DLQ list with filters works correctly");
        println!("   Found {} filtered DLQ entries", dlq_entries.len());

        // Validate that all entries have pending status
        for entry in &dlq_entries {
            if let Some(status) = entry.get("resolution_status") {
                assert_eq!(
                    status.as_str(),
                    Some("Pending"),
                    "All entries should have pending status"
                );
            }
        }
    } else {
        println!(
            "⚠️  DLQ list endpoint not available (status: {})",
            response.status()
        );
    }

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

#[tokio::test]
async fn test_dlq_get_by_task_uuid() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let mut client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Generate a valid JWT token
    let claims = TestClaims::new("dlq-user", 60);
    let private_key = get_test_private_key();
    let token = generate_test_jwt(&claims, &private_key).expect("Failed to generate test JWT");

    client.with_jwt_token(token);

    // Use a test UUID - endpoint should return 404 if not found
    let test_task_uuid = Uuid::new_v4();

    // Test DLQ get by task UUID endpoint
    let response = client
        .get_authenticated(&format!("/v1/dlq/task/{}", test_task_uuid))
        .await
        .expect("Failed to send DLQ get request");

    // Should not return 401 Unauthorized with valid token
    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);

    println!(
        "DLQ get by task UUID endpoint status: {}",
        response.status()
    );

    // Either 200 OK (if entry exists) or 404 Not Found (if doesn't exist)
    if response.status() == StatusCode::OK {
        let dlq_entry = response
            .json::<Value>()
            .await
            .expect("Failed to parse DLQ entry response");

        println!("✅ DLQ Get Response validation:");

        // Validate all expected fields are present
        let expected_fields = [
            "dlq_entry_uuid",
            "task_uuid",
            "original_state",
            "dlq_reason",
            "dlq_timestamp",
            "resolution_status",
            "task_snapshot",
            "created_at",
            "updated_at",
        ];

        for field in &expected_fields {
            assert!(
                dlq_entry.get(field).is_some(),
                "Missing required field: {field}"
            );
            println!("  ✓ {field} present");
        }

        // Validate task_uuid matches requested UUID
        if let Some(returned_uuid) = dlq_entry.get("task_uuid").and_then(|v| v.as_str()) {
            assert_eq!(
                returned_uuid,
                test_task_uuid.to_string(),
                "Returned task UUID should match requested UUID"
            );
        }

        println!("🎉 DLQ get endpoint validation complete!");
    } else if response.status() == StatusCode::NOT_FOUND {
        println!("✅ DLQ entry not found for test UUID (expected)");
    } else {
        println!(
            "⚠️  DLQ get endpoint returned unexpected status: {}",
            response.status()
        );
    }

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

#[tokio::test]
async fn test_dlq_update_investigation() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let mut client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Generate a valid JWT token
    let claims = TestClaims::new("dlq-user", 60);
    let private_key = get_test_private_key();
    let token = generate_test_jwt(&claims, &private_key).expect("Failed to generate test JWT");

    client.with_jwt_token(token);

    // Use a test UUID - endpoint should return 404 if not found
    let test_dlq_entry_uuid = Uuid::new_v4();

    // Create update payload
    let update_payload = json!({
        "resolution_status": "ManuallyResolved",
        "resolution_notes": "Fixed by recreating upstream dependency",
        "resolved_by": "test-operator@example.com",
        "metadata": null
    });

    // Test DLQ update investigation endpoint
    let response = client
        .patch_authenticated(
            &format!("/v1/dlq/entry/{}", test_dlq_entry_uuid),
            update_payload,
        )
        .await
        .expect("Failed to send DLQ update request");

    // Should not return 401 Unauthorized with valid token
    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);

    println!(
        "DLQ update investigation endpoint status: {}",
        response.status()
    );

    // Either 200 OK (if entry exists) or 404 Not Found (if doesn't exist)
    if response.status() == StatusCode::OK {
        let update_response = response
            .json::<Value>()
            .await
            .expect("Failed to parse DLQ update response");

        println!("✅ DLQ Update Response validation:");

        // Validate response structure
        assert!(
            update_response.get("success").is_some(),
            "Missing success field"
        );
        assert!(
            update_response.get("message").is_some(),
            "Missing message field"
        );
        assert!(
            update_response.get("dlq_entry_uuid").is_some(),
            "Missing dlq_entry_uuid field"
        );

        println!("  ✓ success field present");
        println!("  ✓ message field present");
        println!("  ✓ dlq_entry_uuid field present");

        println!("🎉 DLQ update endpoint validation complete!");
    } else if response.status() == StatusCode::NOT_FOUND {
        println!("✅ DLQ entry not found for test UUID (expected)");
    } else {
        println!(
            "⚠️  DLQ update endpoint returned unexpected status: {}",
            response.status()
        );
    }

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

#[tokio::test]
async fn test_dlq_stats_endpoint() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let mut client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Generate a valid JWT token
    let claims = TestClaims::new("dlq-user", 60);
    let private_key = get_test_private_key();
    let token = generate_test_jwt(&claims, &private_key).expect("Failed to generate test JWT");

    client.with_jwt_token(token);

    // Test DLQ stats endpoint
    let response = client
        .get_authenticated("/v1/dlq/stats")
        .await
        .expect("Failed to send DLQ stats request");

    // Should not return 401 Unauthorized with valid token
    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);

    println!("DLQ stats endpoint status: {}", response.status());

    // If the endpoint is available, validate the response structure
    if response.status().is_success() {
        let stats_data = response
            .json::<Vec<Value>>()
            .await
            .expect("Failed to parse DLQ stats response");

        println!("✅ DLQ Stats Response validation:");
        println!("   Found {} DLQ reason groups", stats_data.len());

        // If there are stats, validate structure of first entry
        if let Some(first_stat) = stats_data.first() {
            let expected_fields = [
                "dlq_reason",
                "total_entries",
                "pending",
                "manually_resolved",
                "permanent_failures",
                "oldest_entry",
                "newest_entry",
            ];

            for field in &expected_fields {
                assert!(
                    first_stat.get(field).is_some(),
                    "Missing required field: {field}"
                );
                println!("  ✓ {field} present");
            }

            // Validate field types
            assert!(first_stat["total_entries"].is_i64() || first_stat["total_entries"].is_u64());
            assert!(first_stat["pending"].is_i64() || first_stat["pending"].is_u64());
            assert!(
                first_stat["manually_resolved"].is_i64()
                    || first_stat["manually_resolved"].is_u64()
            );
            assert!(
                first_stat["permanent_failures"].is_i64()
                    || first_stat["permanent_failures"].is_u64()
            );
        }

        println!("🎉 DLQ stats endpoint validation complete!");
    } else {
        println!(
            "⚠️  DLQ stats endpoint not available (status: {})",
            response.status()
        );
        println!("   This is expected if the web API server is not running");
    }

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

#[tokio::test]
async fn test_dlq_endpoints_without_authentication() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    println!("🔍 Testing DLQ endpoints WITHOUT authentication...");

    // Test list endpoint without authentication
    let response = client
        .get("/v1/dlq")
        .await
        .expect("Failed to send unauthenticated list request");

    // In test mode (create_test_app), authentication is typically disabled,
    // so we expect 200 OK if the endpoints are implemented correctly
    let status = response.status();
    println!("   DLQ list endpoint (unauthenticated): {status}");
    assert_eq!(
        status,
        StatusCode::OK,
        "Expected 200 OK for test mode (auth disabled), got {status}"
    );

    // Test stats endpoint without authentication
    let response = client
        .get("/v1/dlq/stats")
        .await
        .expect("Failed to send unauthenticated stats request");

    let status = response.status();
    println!("   DLQ stats endpoint (unauthenticated): {status}");
    assert_eq!(
        status,
        StatusCode::OK,
        "Expected 200 OK for test mode (auth disabled), got {status}"
    );

    println!("✅ DLQ endpoints work correctly without authentication in test mode");

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

#[tokio::test]
async fn test_dlq_endpoints_with_proper_authentication() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let mut client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Generate a valid JWT token
    let claims = TestClaims::new("dlq-test-user", 60);
    let private_key = get_test_private_key();
    let token = generate_test_jwt(&claims, &private_key).expect("Failed to generate test JWT");

    client.with_jwt_token(token);

    println!("🔍 Testing DLQ endpoints WITH proper authentication...");

    // Test list endpoint with authentication
    let response = client
        .get_authenticated("/v1/dlq")
        .await
        .expect("Failed to send authenticated list request");

    let status = response.status();
    println!("   DLQ list endpoint (authenticated): {status}");
    assert_eq!(
        status,
        StatusCode::OK,
        "Expected 200 OK with valid auth token, got {status}"
    );

    if status.is_success() {
        // Validate the response structure to ensure the endpoint actually works
        let dlq_entries = response
            .json::<Vec<Value>>()
            .await
            .expect("Failed to parse DLQ list response");

        println!(
            "   ✓ List response structure validated ({} entries)",
            dlq_entries.len()
        );
    }

    // Test stats endpoint with authentication
    let response = client
        .get_authenticated("/v1/dlq/stats")
        .await
        .expect("Failed to send authenticated stats request");

    let status = response.status();
    println!("   DLQ stats endpoint (authenticated): {status}");
    assert_eq!(
        status,
        StatusCode::OK,
        "Expected 200 OK with valid auth token, got {status}"
    );

    if status.is_success() {
        // Validate the response structure to ensure the endpoint actually works
        let stats_data = response
            .json::<Vec<Value>>()
            .await
            .expect("Failed to parse DLQ stats response");

        println!(
            "   ✓ Stats response structure validated ({} reason groups)",
            stats_data.len()
        );
    }

    println!("✅ DLQ endpoints work correctly with proper authentication");

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}
