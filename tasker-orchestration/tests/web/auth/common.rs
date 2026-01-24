//! Common auth enforcement tests: health endpoints, no credentials, invalid tokens.

use super::super::auth_test_helpers::*;
use reqwest::StatusCode;

// =============================================================================
// Health Endpoints Always Public
// =============================================================================

#[tokio::test]
async fn health_public_without_credentials() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);
    client.without_auth();

    // Liveness endpoints should always return 200
    for path in ["/health", "/health/live"] {
        let response = client.get(path).await.expect("request failed");
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "Health endpoint {path} should be public and return 200 (got {})",
            response.status()
        );
    }

    // Readiness endpoint is public (no 401/403) but may return 503 if DB isn't fully ready
    let response = client.get("/health/ready").await.expect("request failed");
    assert!(
        response.status() == StatusCode::OK
            || response.status() == StatusCode::SERVICE_UNAVAILABLE,
        "/health/ready should be public (got {} - expected 200 or 503)",
        response.status()
    );

    server.shutdown().await.expect("shutdown failed");
}

// =============================================================================
// No Credentials → 401
// =============================================================================

#[tokio::test]
async fn no_credentials_returns_401() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);
    client.without_auth();

    let protected_gets = ["/v1/tasks", "/v1/handlers", "/config"];

    for path in protected_gets {
        let response = client.get(path).await.expect("request failed");
        assert_eq!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "GET {path} without credentials should return 401 (got {})",
            response.status()
        );
    }

    // POST without credentials
    let task_body = super::super::test_infrastructure::create_test_task_request_json();
    let response = client
        .post_json("/v1/tasks", &task_body)
        .await
        .expect("request failed");
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "POST /v1/tasks without credentials should return 401"
    );

    server.shutdown().await.expect("shutdown failed");
}

// =============================================================================
// Expired JWT → 401
// =============================================================================

#[tokio::test]
async fn expired_jwt_returns_401() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);

    let token = generate_expired_jwt(&["*"]);
    client.with_jwt(&token);

    let response = client.get("/v1/tasks").await.expect("request failed");
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "GET /v1/tasks with expired JWT should return 401 (got {})",
        response.status()
    );

    server.shutdown().await.expect("shutdown failed");
}

// =============================================================================
// Malformed JWT → 401
// =============================================================================

#[tokio::test]
async fn malformed_jwt_returns_401() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);
    client.with_jwt("not.a.valid.jwt.token");

    let response = client.get("/v1/tasks").await.expect("request failed");
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "GET /v1/tasks with malformed JWT should return 401 (got {})",
        response.status()
    );

    server.shutdown().await.expect("shutdown failed");
}

// =============================================================================
// Wrong Issuer / Wrong Audience → 401
// =============================================================================

#[tokio::test]
async fn wrong_issuer_returns_401() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);

    let token = generate_jwt_wrong_issuer(&["*"]);
    client.with_jwt(&token);

    let response = client.get("/v1/tasks").await.expect("request failed");
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "GET /v1/tasks with wrong issuer JWT should return 401 (got {})",
        response.status()
    );

    server.shutdown().await.expect("shutdown failed");
}

#[tokio::test]
async fn wrong_audience_returns_401() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);

    let token = generate_jwt_wrong_audience(&["*"]);
    client.with_jwt(&token);

    let response = client.get("/v1/tasks").await.expect("request failed");
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "GET /v1/tasks with wrong audience JWT should return 401 (got {})",
        response.status()
    );

    server.shutdown().await.expect("shutdown failed");
}
