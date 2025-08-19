//! # OpenAPI Documentation Endpoint Tests
//!
//! Tests to verify that OpenAPI documentation endpoints are accessible
//! and return valid responses.

use super::test_infrastructure::*;
use reqwest::StatusCode;

/// Test that the OpenAPI JSON specification is accessible
#[tokio::test]
async fn test_openapi_json_endpoint() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Test OpenAPI JSON endpoint
    let response = client
        .get("/api-docs/openapi.json")
        .await
        .expect("Failed to send request");

    assert_eq!(response.status(), StatusCode::OK);

    // Verify content type is JSON
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

    // Verify the response is valid JSON
    let openapi_spec: serde_json::Value = response
        .json()
        .await
        .expect("Response should be valid JSON");

    // Verify basic OpenAPI structure
    assert!(
        openapi_spec.get("openapi").is_some(),
        "Should have openapi version field"
    );
    assert!(
        openapi_spec.get("info").is_some(),
        "Should have info section"
    );
    assert!(
        openapi_spec.get("paths").is_some(),
        "Should have paths section"
    );

    // Verify our API info
    let info = &openapi_spec["info"];
    assert_eq!(info["title"], "Tasker Web API");
    assert_eq!(info["version"], "1.0.0");

    // Verify some expected paths exist
    let paths = &openapi_spec["paths"];
    assert!(
        paths.get("/health").is_some(),
        "Should have health endpoint"
    );
    assert!(
        paths.get("/v1/tasks").is_some(),
        "Should have tasks endpoint"
    );
    assert!(
        paths.get("/v1/analytics/performance").is_some(),
        "Should have analytics endpoint"
    );

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

/// Test that the Swagger UI is accessible
#[tokio::test]
async fn test_swagger_ui_endpoint() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Test Swagger UI endpoint
    let response = client
        .get("/api-docs/ui")
        .await
        .expect("Failed to send request");

    // Swagger UI might redirect to the actual page, so accept 200 or 301/302
    let status = response.status();
    assert!(
        status == StatusCode::OK || status.is_redirection(),
        "Expected 200 OK or redirection for Swagger UI, got {status}"
    );

    // If it's a 200 response, verify it's HTML
    if status == StatusCode::OK {
        let content_type = response
            .headers()
            .get("content-type")
            .map(|ct| ct.to_str().unwrap_or(""))
            .unwrap_or("");

        assert!(
            content_type.contains("text/html") || content_type.contains("application/json"),
            "Expected HTML or JSON content for Swagger UI, got: {content_type}"
        );
    }

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

/// Test that Swagger UI redirects properly work
#[tokio::test]
async fn test_swagger_ui_redirect() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Test Swagger UI base path (should redirect to full path)
    let response = client
        .get("/api-docs/ui/")
        .await
        .expect("Failed to send request");

    // Should get a valid response (either direct content or redirect)
    let status = response.status();
    assert!(
        status.is_success() || status.is_redirection(),
        "Expected success or redirection for Swagger UI, got {status}"
    );

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}

/// Test OpenAPI specification contains expected operations
#[tokio::test]
async fn test_openapi_contains_expected_operations() {
    // Start test server with dynamic port allocation
    let test_server = TestServer::start()
        .await
        .expect("Failed to start test server");
    let client = WebTestClient::for_server(&test_server).expect("Failed to create test client");

    // Get OpenAPI specification
    let response = client
        .get("/api-docs/openapi.json")
        .await
        .expect("Failed to send request");
    let openapi_spec: serde_json::Value = response
        .json()
        .await
        .expect("Response should be valid JSON");

    let paths = &openapi_spec["paths"];

    // Verify health endpoints
    assert!(
        paths.get("/health").is_some(),
        "Should have basic health endpoint"
    );
    assert!(
        paths.get("/ready").is_some(),
        "Should have readiness probe endpoint"
    );
    assert!(
        paths.get("/live").is_some(),
        "Should have liveness probe endpoint"
    );

    // Verify task endpoints
    assert!(
        paths.get("/v1/tasks").is_some(),
        "Should have tasks endpoint"
    );
    if let Some(tasks_path) = paths.get("/v1/tasks") {
        assert!(
            tasks_path.get("get").is_some(),
            "Should have GET /v1/tasks operation"
        );
        assert!(
            tasks_path.get("post").is_some(),
            "Should have POST /v1/tasks operation"
        );
    }

    // Verify analytics endpoints
    assert!(
        paths.get("/v1/analytics/performance").is_some(),
        "Should have performance analytics endpoint"
    );
    assert!(
        paths.get("/v1/analytics/bottlenecks").is_some(),
        "Should have bottleneck analytics endpoint"
    );

    // Verify schemas section exists
    let components = &openapi_spec["components"];
    assert!(
        components.get("schemas").is_some(),
        "Should have schemas section"
    );

    let schemas = &components["schemas"];
    assert!(
        schemas.get("TaskRequest").is_some(),
        "Should have TaskRequest schema"
    );
    assert!(
        schemas.get("HealthResponse").is_some(),
        "Should have HealthResponse schema"
    );
    assert!(
        schemas.get("ApiError").is_some(),
        "Should have ApiError schema"
    );

    // Shutdown test server
    test_server
        .shutdown()
        .await
        .expect("Failed to shutdown test server");
}
