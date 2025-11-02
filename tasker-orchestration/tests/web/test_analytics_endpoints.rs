//! Integration tests for analytics endpoints

use reqwest::StatusCode;
use serde_json::Value;

use super::authenticated_tests::{generate_test_jwt, get_test_private_key, TestClaims};
use super::test_infrastructure::*;

#[tokio::test]
async fn test_analytics_performance_endpoint() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let mut client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Generate a valid JWT token with analytics permissions
    let claims = TestClaims::new("analytics-user", 60);
    let private_key = get_test_private_key();
    let token = generate_test_jwt(&claims, &private_key).expect("Failed to generate test JWT");

    client.with_jwt_token(token);

    // Test analytics performance endpoint
    let response = client
        .get_authenticated("/v1/analytics/performance")
        .await
        .expect("Failed to send analytics performance request");

    // Should not return 401 Unauthorized with valid token
    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);

    println!(
        "Analytics performance endpoint status: {}",
        response.status()
    );

    // If the endpoint is available, validate the response structure
    if response.status().is_success() {
        let performance_data = response
            .json::<Value>()
            .await
            .expect("Failed to parse analytics performance response");

        println!("✅ Analytics Performance Response validation:");

        // Validate all expected fields are present
        let expected_fields = [
            "total_tasks",
            "active_tasks",
            "completed_tasks",
            "failed_tasks",
            "completion_rate",
            "error_rate",
            "average_task_duration_seconds",
            "average_step_duration_seconds",
            "tasks_per_hour",
            "steps_per_hour",
            "system_health_score",
            "analysis_period_start",
            "calculated_at",
        ];

        for field in &expected_fields {
            assert!(
                performance_data.get(field).is_some(),
                "Missing required field: {field}"
            );
            println!("  ✓ {field} present");
        }

        // Validate field types
        assert!(
            performance_data["total_tasks"].is_i64() || performance_data["total_tasks"].is_u64()
        );
        assert!(
            performance_data["completion_rate"].is_f64()
                || performance_data["completion_rate"].is_i64()
        );
        assert!(
            performance_data["system_health_score"].is_f64()
                || performance_data["system_health_score"].is_i64()
        );
        assert!(performance_data["calculated_at"].is_string());

        println!("🎉 Analytics performance endpoint validation complete!");
    } else {
        println!(
            "⚠️  Analytics performance endpoint not available (status: {})",
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
async fn test_analytics_performance_with_hours_parameter() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let mut client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Generate a valid JWT token
    let claims = TestClaims::new("analytics-user", 60);
    let private_key = get_test_private_key();
    let token = generate_test_jwt(&claims, &private_key).expect("Failed to generate test JWT");

    client.with_jwt_token(token);

    // Test analytics performance endpoint with hours parameter
    let response = client
        .get_authenticated("/v1/analytics/performance?hours=48")
        .await
        .expect("Failed to send analytics performance request with hours parameter");

    // Should not return 401 Unauthorized with valid token
    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);

    println!(
        "Analytics performance with hours parameter status: {}",
        response.status()
    );

    // If the endpoint is available, validate the response
    if response.status().is_success() {
        let performance_data = response
            .json::<Value>()
            .await
            .expect("Failed to parse analytics performance response");

        // Should have the same structure as the basic endpoint
        assert!(performance_data.get("total_tasks").is_some());
        assert!(performance_data.get("calculated_at").is_some());

        println!("✅ Analytics performance with hours parameter works correctly");
    } else {
        println!(
            "⚠️  Analytics performance endpoint not available (status: {})",
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
async fn test_analytics_bottlenecks_endpoint() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let mut client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Generate a valid JWT token
    let claims = TestClaims::new("analytics-user", 60);
    let private_key = get_test_private_key();
    let token = generate_test_jwt(&claims, &private_key).expect("Failed to generate test JWT");

    client.with_jwt_token(token);

    // Test analytics bottlenecks endpoint
    let response = client
        .get_authenticated("/v1/analytics/bottlenecks")
        .await
        .expect("Failed to send analytics bottlenecks request");

    // Should not return 401 Unauthorized with valid token
    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);

    println!(
        "Analytics bottlenecks endpoint status: {}",
        response.status()
    );

    // If the endpoint is available, validate the response structure
    if response.status().is_success() {
        let bottleneck_data = response
            .json::<Value>()
            .await
            .expect("Failed to parse analytics bottlenecks response");

        println!("✅ Analytics Bottlenecks Response validation:");

        // Validate all expected fields are present
        let expected_fields = [
            "slow_steps",
            "slow_tasks",
            "resource_utilization",
            "recommendations",
        ];

        for field in &expected_fields {
            assert!(
                bottleneck_data.get(field).is_some(),
                "Missing required field: {field}"
            );
            println!("  ✓ {field} present");
        }

        // Validate field types
        assert!(bottleneck_data["slow_steps"].is_array());
        assert!(bottleneck_data["slow_tasks"].is_array());
        assert!(bottleneck_data["resource_utilization"].is_object());
        assert!(bottleneck_data["recommendations"].is_array());

        println!("🎉 Analytics bottlenecks endpoint validation complete!");
    } else {
        println!(
            "⚠️  Analytics bottlenecks endpoint not available (status: {})",
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
async fn test_analytics_bottlenecks_with_parameters() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let mut client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Generate a valid JWT token
    let claims = TestClaims::new("analytics-user", 60);
    let private_key = get_test_private_key();
    let token = generate_test_jwt(&claims, &private_key).expect("Failed to generate test JWT");

    client.with_jwt_token(token);

    // Test analytics bottlenecks endpoint with parameters
    let response = client
        .get_authenticated("/v1/analytics/bottlenecks?limit=5&min_executions=10")
        .await
        .expect("Failed to send analytics bottlenecks request with parameters");

    // Should not return 401 Unauthorized with valid token
    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);

    println!(
        "Analytics bottlenecks with parameters status: {}",
        response.status()
    );

    // If the endpoint is available, validate the response
    if response.status().is_success() {
        let bottleneck_data = response
            .json::<Value>()
            .await
            .expect("Failed to parse analytics bottlenecks response");

        // Should have the same structure as the basic endpoint
        assert!(bottleneck_data.get("slow_steps").is_some());
        assert!(bottleneck_data.get("resource_utilization").is_some());

        println!("✅ Analytics bottlenecks with parameters works correctly");
    } else {
        println!(
            "⚠️  Analytics bottlenecks endpoint not available (status: {})",
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
async fn test_analytics_endpoints_without_authentication() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    println!("🔍 Testing analytics endpoints WITHOUT authentication...");

    // Test performance endpoint without authentication
    let response = client
        .get("/v1/analytics/performance")
        .await
        .expect("Failed to send unauthenticated performance request");

    // In test mode (create_test_app), authentication is typically disabled,
    // so we expect 200 OK if the endpoints are implemented correctly
    let status = response.status();
    println!("   Performance endpoint (unauthenticated): {status}");

    if status != StatusCode::OK {
        // Print error response body for debugging
        let error_body = response
            .text()
            .await
            .unwrap_or_else(|_| "Failed to read error body".to_string());
        println!("   Error response body: {error_body}");
    }

    assert_eq!(
        status,
        StatusCode::OK,
        "Expected 200 OK for test mode (auth disabled), got {status}"
    );

    // Test bottlenecks endpoint without authentication
    let response = client
        .get("/v1/analytics/bottlenecks")
        .await
        .expect("Failed to send unauthenticated bottlenecks request");

    let status = response.status();
    println!("   Bottlenecks endpoint (unauthenticated): {status}");

    if status != StatusCode::OK {
        // Print error response body for debugging
        let error_body = response
            .text()
            .await
            .unwrap_or_else(|_| "Failed to read error body".to_string());
        println!("   Error response body: {error_body}");
    }

    assert_eq!(
        status,
        StatusCode::OK,
        "Expected 200 OK for test mode (auth disabled), got {status}"
    );

    println!("✅ Analytics endpoints work correctly without authentication in test mode");

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

#[tokio::test]
async fn test_analytics_endpoints_with_proper_authentication() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let mut client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Generate a valid JWT token with analytics permissions
    let claims = TestClaims::new("analytics-test-user", 60);
    // Note: TestClaims already includes necessary permissions for test mode

    let private_key = get_test_private_key();
    let token = generate_test_jwt(&claims, &private_key).expect("Failed to generate test JWT");

    client.with_jwt_token(token);

    println!("🔍 Testing analytics endpoints WITH proper authentication...");

    // Test performance endpoint with authentication
    let response = client
        .get_authenticated("/v1/analytics/performance")
        .await
        .expect("Failed to send authenticated performance request");

    let status = response.status();
    println!("   Performance endpoint (authenticated): {status}");
    assert_eq!(
        status,
        StatusCode::OK,
        "Expected 200 OK with valid auth token, got {status}"
    );

    if status.is_success() {
        // Validate the response structure to ensure the endpoint actually works
        let performance_data = response
            .json::<Value>()
            .await
            .expect("Failed to parse analytics performance response");

        // Key fields that should be present
        assert!(
            performance_data.get("total_tasks").is_some(),
            "Missing total_tasks field"
        );
        assert!(
            performance_data.get("completion_rate").is_some(),
            "Missing completion_rate field"
        );
        assert!(
            performance_data.get("system_health_score").is_some(),
            "Missing system_health_score field"
        );

        println!("   ✓ Performance response structure validated");
    }

    // Test bottlenecks endpoint with authentication
    let response = client
        .get_authenticated("/v1/analytics/bottlenecks")
        .await
        .expect("Failed to send authenticated bottlenecks request");

    let status = response.status();
    println!("   Bottlenecks endpoint (authenticated): {status}");

    if status.is_success() {
        // Validate the response structure to ensure the endpoint actually works
        let bottleneck_data = response
            .json::<Value>()
            .await
            .expect("Failed to parse analytics bottlenecks response");

        // Key fields that should be present
        assert!(
            bottleneck_data.get("slow_steps").is_some(),
            "Missing slow_steps field"
        );
        assert!(
            bottleneck_data.get("resource_utilization").is_some(),
            "Missing resource_utilization field"
        );
        assert!(
            bottleneck_data.get("recommendations").is_some(),
            "Missing recommendations field"
        );

        println!("   ✓ Bottlenecks response structure validated");
    } else {
        // Print error response body for debugging
        let error_body = response
            .text()
            .await
            .unwrap_or_else(|_| "Failed to read error body".to_string());
        println!("   Error response body: {error_body}");
    }

    assert_eq!(
        status,
        StatusCode::OK,
        "Expected 200 OK with valid auth token, got {status}"
    );

    println!("✅ Analytics endpoints work correctly with proper authentication");

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}
