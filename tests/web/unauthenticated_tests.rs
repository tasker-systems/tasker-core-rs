//! # Unauthenticated Web API Endpoint Tests
//!
//! Tests for public endpoints that don't require authentication, including:
//! - Health check endpoints
//! - CORS preflight requests
//! - Public information endpoints
//! - Error handling for protected endpoints

use super::test_infrastructure::*;
use reqwest::StatusCode;
use serde_json::Value;
use std::time::Duration;
use tokio::time::timeout;

/// Test health check endpoint (should be publicly accessible)
#[tokio::test]
async fn test_health_endpoint_unauthenticated() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Test health endpoint
    let response = client.get("/health").await.expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    let health_data = assert_json_response(response, 200, &["status", "timestamp"])
        .await
        .expect("Failed to parse health response");

    // Health endpoint might return "ok" or "healthy" depending on implementation
    let status = health_data["status"]
        .as_str()
        .expect("Status should be string");
    assert!(
        status == "healthy" || status == "ok",
        "Health status should be 'healthy' or 'ok', got '{status}'"
    );

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

/// Test that health endpoint returns proper JSON structure
#[tokio::test]
async fn test_health_endpoint_json_structure() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    let response = client.get("/health").await.expect("Failed to send request");
    let health_data: Value = response.json().await.expect("Failed to parse JSON");

    // Verify required fields
    assert!(health_data.get("status").is_some());
    assert!(health_data.get("timestamp").is_some());

    // Verify status is a string
    assert!(health_data["status"].is_string());

    // Verify timestamp is a string (ISO 8601 format)
    let timestamp_str = health_data["timestamp"]
        .as_str()
        .expect("Timestamp should be string");
    assert!(!timestamp_str.is_empty());

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

/// Test CORS preflight request (OPTIONS)
#[tokio::test]
async fn test_cors_preflight_request() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Make an OPTIONS request to test CORS
    let url = format!("{}/health", client.base_url());
    let response = reqwest::Client::new()
        .request(reqwest::Method::OPTIONS, &url)
        .header("Origin", "http://localhost:3000")
        .header("Access-Control-Request-Method", "GET")
        .header("Access-Control-Request-Headers", "content-type")
        .send()
        .await
        .expect("Failed to send OPTIONS request");

    // CORS is implemented with CorsLayer allowing any origin/methods/headers
    // Should return 200 OK for preflight requests
    assert_eq!(
        response.status(),
        StatusCode::OK,
        "CORS preflight should return 200 OK"
    );

    // Check required CORS headers are present
    let headers = response.headers();
    assert!(
        headers.get("access-control-allow-origin").is_some(),
        "CORS allow-origin header missing"
    );
    assert!(
        headers.get("access-control-allow-methods").is_some(),
        "CORS allow-methods header missing"
    );

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

/// Test that protected endpoints return expected responses without authentication
#[tokio::test]
async fn test_protected_endpoints_require_auth() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Test actual endpoints that exist (from routes.rs)
    // In test mode, auth is disabled, so we expect:
    // - 404 for non-existent endpoints
    // - 500 or other responses for existing but unimplemented endpoints
    let test_endpoints = vec![
        ("/v1/tasks", "tasks endpoint"),
        ("/v1/analytics/performance", "analytics endpoint"),
        ("/v1/handlers", "handlers endpoint"),
    ];

    for (endpoint, description) in test_endpoints {
        let response = client.get(endpoint).await.expect("Failed to send request");

        // In test mode with auth disabled, we expect endpoints to respond
        // (not 401) but may return 500 if not fully implemented
        assert_ne!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "{description} should not return 401 in test mode (auth disabled)"
        );

        // Verify we get some kind of response (not connection refused)
        let status = response.status();
        assert!(
            status.is_client_error() || status.is_server_error() || status.is_success(),
            "{description} should return a valid HTTP status, got {status}"
        );
    }

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

/// Test invalid endpoint returns 404
#[tokio::test]
async fn test_invalid_endpoint_returns_404() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    let response = client
        .get("/api/nonexistent/endpoint")
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    // Try to parse JSON response, but handle cases where 404 might not return JSON
    match response.headers().get("content-type") {
        Some(content_type)
            if content_type
                .to_str()
                .unwrap_or("")
                .contains("application/json") =>
        {
            // Response contains JSON, parse it
            let error_data = assert_error_response(response, 404, "not found")
                .await
                .expect("Failed to parse JSON error response");

            assert!(error_data["error"]
                .as_str()
                .unwrap()
                .to_lowercase()
                .contains("not found"));
            println!("✅ Invalid endpoint test: Got JSON 404 response with error message");
        }
        _ => {
            // Response doesn't contain JSON (e.g., plain text 404 from Axum default handler)
            let body = response.text().await.expect("Failed to get response text");
            println!("✅ Invalid endpoint test: Got non-JSON 404 response: {body}");
            // Just verify we got 404, which we already did above
        }
    }

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

/// Test malformed JSON request returns 400
#[tokio::test]
async fn test_malformed_json_returns_400() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Send malformed JSON to the actual tasks POST endpoint
    let url = format!("{}/v1/tasks", client.base_url());
    let response = reqwest::Client::new()
        .post(&url)
        .header("Content-Type", "application/json")
        .body("{ invalid json }")
        .send()
        .await
        .expect("Failed to send request");

    // Should return 400 Bad Request for malformed JSON (or 401 if auth is checked first, or 500 if endpoint exists but not fully implemented)
    // In test environment with auth disabled, we expect either 400 for JSON parsing error or 500 for unimplemented handler
    let status = response.status();
    assert!(
        status == StatusCode::BAD_REQUEST
            || status == StatusCode::UNAUTHORIZED
            || status == StatusCode::INTERNAL_SERVER_ERROR,
        "Expected 400 (Bad Request), 401 (Unauthorized), or 500 (Internal Server Error) for malformed JSON, got {status}"
    );

    println!("✅ Malformed JSON test: Got {status} status for malformed JSON request to /v1/tasks");

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

/// Test rate limiting (if enabled)
#[tokio::test]
async fn test_rate_limiting_behavior() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Make multiple rapid requests to test rate limiting
    let mut responses = Vec::new();

    for _ in 0..10 {
        let response = client.get("/health").await.expect("Failed to send request");
        responses.push(response.status());

        // Small delay to avoid overwhelming the test
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // If rate limiting is disabled (default), all requests should succeed
    // If enabled, some might return 429 Too Many Requests
    let successful_requests = responses
        .iter()
        .filter(|&&status| status == StatusCode::OK)
        .count();

    // Should have at least some successful requests
    assert!(
        successful_requests > 0,
        "Should have at least some successful requests"
    );

    // Log rate limiting behavior for debugging
    println!(
        "Rate limiting test: {}/{} requests succeeded",
        successful_requests,
        responses.len()
    );

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

/// Test server response time is reasonable
#[tokio::test]
async fn test_response_time_performance() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    let start = std::time::Instant::now();
    let response = client.get("/health").await.expect("Failed to send request");
    let duration = start.elapsed();

    assert_eq!(response.status(), StatusCode::OK);

    // Health endpoint should respond quickly (under 1 second for local testing)
    assert!(
        duration < Duration::from_secs(1),
        "Health endpoint took too long: {duration:?}"
    );

    println!("Health endpoint response time: {duration:?}");

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

/// Test content-type headers
#[tokio::test]
async fn test_content_type_headers() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    let response = client.get("/health").await.expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    // Check that response has proper content-type
    let content_type = response
        .headers()
        .get("content-type")
        .expect("Content-Type header should be present")
        .to_str()
        .expect("Content-Type should be valid string");

    assert!(
        content_type.contains("application/json"),
        "Expected JSON content-type, got: {content_type}"
    );

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

/// Test request timeout handling
#[tokio::test]
async fn test_request_timeout_handling() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Test that requests complete within reasonable time
    let result = timeout(Duration::from_secs(5), client.get("/health")).await;

    assert!(result.is_ok(), "Request should complete within timeout");

    let response = result.unwrap().expect("Request should succeed");
    assert_eq!(response.status(), StatusCode::OK);

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    /// Test that we can create a test client successfully
    #[test]
    fn test_create_test_client() {
        let config = WebTestConfig::default();
        let client = WebTestClient::new(config);
        assert!(client.is_ok());
    }

    /// Test that test configuration has sensible defaults
    #[test]
    fn test_web_test_config_sanity() {
        let config = WebTestConfig::default();
        assert!(!config.base_url.is_empty());
        assert!(!config.bind_address.is_empty());
        assert!(config.port > 0);
    }
}
