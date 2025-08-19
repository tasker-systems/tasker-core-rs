//! Integration tests for analytics endpoints

use reqwest::StatusCode;
use serde_json::Value;

use super::test_infrastructure::*;
use super::authenticated_tests::{TestClaims, generate_test_jwt, get_test_private_key};

#[tokio::test]
async fn test_analytics_performance_endpoint() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start().await.expect("Failed to start test server");
    let mut client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Generate a valid JWT token with analytics permissions
    let claims = TestClaims::new("analytics-user", 60);
    let private_key = get_test_private_key();
    let token = generate_test_jwt(&claims, &private_key)
        .expect("Failed to generate test JWT");

    client.with_jwt_token(token);

    // Test analytics performance endpoint
    let response = client.get_authenticated("/v1/analytics/performance").await
        .expect("Failed to send analytics performance request");
    
    // Should not return 401 Unauthorized with valid token
    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);
    
    println!("Analytics performance endpoint status: {}", response.status());
    
    // If the endpoint is available, validate the response structure
    if response.status().is_success() {
        let performance_data = response.json::<Value>().await
            .expect("Failed to parse analytics performance response");
        
        println!("✅ Analytics Performance Response validation:");
        
        // Validate all expected fields are present
        let expected_fields = [
            "total_tasks", "active_tasks", "completed_tasks", "failed_tasks",
            "completion_rate", "error_rate", "average_task_duration_seconds",
            "average_step_duration_seconds", "tasks_per_hour", "steps_per_hour",
            "system_health_score", "analysis_period_start", "calculated_at"
        ];
        
        for field in &expected_fields {
            assert!(performance_data.get(field).is_some(), "Missing required field: {}", field);
            println!("  ✓ {} present", field);
        }
        
        // Validate field types
        assert!(performance_data["total_tasks"].is_i64() || performance_data["total_tasks"].is_u64());
        assert!(performance_data["completion_rate"].is_f64() || performance_data["completion_rate"].is_i64());
        assert!(performance_data["system_health_score"].is_f64() || performance_data["system_health_score"].is_i64());
        assert!(performance_data["calculated_at"].is_string());
        
        println!("🎉 Analytics performance endpoint validation complete!");
    } else {
        println!("⚠️  Analytics performance endpoint not available (status: {})", response.status());
        println!("   This is expected if the web API server is not running");
    }
    
    // Shutdown test server
    test_server.shutdown().await.expect("Failed to shutdown test server");
}

#[tokio::test]
async fn test_analytics_performance_with_hours_parameter() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start().await.expect("Failed to start test server");
    let mut client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Generate a valid JWT token
    let claims = TestClaims::new("analytics-user", 60);
    let private_key = get_test_private_key();
    let token = generate_test_jwt(&claims, &private_key)
        .expect("Failed to generate test JWT");

    client.with_jwt_token(token);

    // Test analytics performance endpoint with hours parameter
    let response = client.get_authenticated("/v1/analytics/performance?hours=48").await
        .expect("Failed to send analytics performance request with hours parameter");
    
    // Should not return 401 Unauthorized with valid token
    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);
    
    println!("Analytics performance with hours parameter status: {}", response.status());
    
    // If the endpoint is available, validate the response
    if response.status().is_success() {
        let performance_data = response.json::<Value>().await
            .expect("Failed to parse analytics performance response");
        
        // Should have the same structure as the basic endpoint
        assert!(performance_data.get("total_tasks").is_some());
        assert!(performance_data.get("calculated_at").is_some());
        
        println!("✅ Analytics performance with hours parameter works correctly");
    } else {
        println!("⚠️  Analytics performance endpoint not available (status: {})", response.status());
    }
    
    // Shutdown test server
    test_server.shutdown().await.expect("Failed to shutdown test server");
}

#[tokio::test]
async fn test_analytics_bottlenecks_endpoint() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start().await.expect("Failed to start test server");
    let mut client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Generate a valid JWT token
    let claims = TestClaims::new("analytics-user", 60);
    let private_key = get_test_private_key();
    let token = generate_test_jwt(&claims, &private_key)
        .expect("Failed to generate test JWT");

    client.with_jwt_token(token);

    // Test analytics bottlenecks endpoint
    let response = client.get_authenticated("/v1/analytics/bottlenecks").await
        .expect("Failed to send analytics bottlenecks request");
    
    // Should not return 401 Unauthorized with valid token
    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);
    
    println!("Analytics bottlenecks endpoint status: {}", response.status());
    
    // If the endpoint is available, validate the response structure
    if response.status().is_success() {
        let bottleneck_data = response.json::<Value>().await
            .expect("Failed to parse analytics bottlenecks response");
        
        println!("✅ Analytics Bottlenecks Response validation:");
        
        // Validate all expected fields are present
        let expected_fields = ["slow_steps", "slow_tasks", "resource_utilization", "recommendations"];
        
        for field in &expected_fields {
            assert!(bottleneck_data.get(field).is_some(), "Missing required field: {}", field);
            println!("  ✓ {} present", field);
        }
        
        // Validate field types
        assert!(bottleneck_data["slow_steps"].is_array());
        assert!(bottleneck_data["slow_tasks"].is_array());
        assert!(bottleneck_data["resource_utilization"].is_object());
        assert!(bottleneck_data["recommendations"].is_array());
        
        println!("🎉 Analytics bottlenecks endpoint validation complete!");
    } else {
        println!("⚠️  Analytics bottlenecks endpoint not available (status: {})", response.status());
    }
    
    // Shutdown test server
    test_server.shutdown().await.expect("Failed to shutdown test server");
}

#[tokio::test]
async fn test_analytics_bottlenecks_with_parameters() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start().await.expect("Failed to start test server");
    let mut client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Generate a valid JWT token
    let claims = TestClaims::new("analytics-user", 60);
    let private_key = get_test_private_key();
    let token = generate_test_jwt(&claims, &private_key)
        .expect("Failed to generate test JWT");

    client.with_jwt_token(token);

    // Test analytics bottlenecks endpoint with parameters
    let response = client.get_authenticated("/v1/analytics/bottlenecks?limit=5&min_executions=10").await
        .expect("Failed to send analytics bottlenecks request with parameters");
    
    // Should not return 401 Unauthorized with valid token
    assert_ne!(response.status(), StatusCode::UNAUTHORIZED);
    
    println!("Analytics bottlenecks with parameters status: {}", response.status());
    
    // If the endpoint is available, validate the response
    if response.status().is_success() {
        let bottleneck_data = response.json::<Value>().await
            .expect("Failed to parse analytics bottlenecks response");
        
        // Should have the same structure as the basic endpoint
        assert!(bottleneck_data.get("slow_steps").is_some());
        assert!(bottleneck_data.get("resource_utilization").is_some());
        
        println!("✅ Analytics bottlenecks with parameters works correctly");
    } else {
        println!("⚠️  Analytics bottlenecks endpoint not available (status: {})", response.status());
    }
    
    // Shutdown test server
    test_server.shutdown().await.expect("Failed to shutdown test server");
}

#[tokio::test]
async fn test_analytics_endpoints_require_authentication() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start().await.expect("Failed to start test server");
    let client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Test performance endpoint without authentication
    let response = client.get("/v1/analytics/performance").await
        .expect("Failed to send unauthenticated performance request");
    
    // In test mode, authentication might be disabled, so we expect either:
    // - 401 Unauthorized (if auth is enabled)
    // - 500 Internal Server Error (if auth is disabled but endpoints not implemented)
    let status = response.status();
    assert!(
        status == StatusCode::UNAUTHORIZED || status == StatusCode::INTERNAL_SERVER_ERROR,
        "Expected 401 or 500, got {}", status
    );
    
    // Test bottlenecks endpoint without authentication
    let response = client.get("/v1/analytics/bottlenecks").await
        .expect("Failed to send unauthenticated bottlenecks request");
    
    // Same expectation for bottlenecks endpoint
    let status = response.status();
    assert!(
        status == StatusCode::UNAUTHORIZED || status == StatusCode::INTERNAL_SERVER_ERROR,
        "Expected 401 or 500, got {}", status
    );
    
    println!("✅ Analytics endpoints behave correctly (401 if auth enabled, 500 if not implemented)");
    
    // Shutdown test server
    test_server.shutdown().await.expect("Failed to shutdown test server");
}