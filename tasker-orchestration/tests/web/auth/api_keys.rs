//! API key authentication and permission enforcement tests.

use super::super::auth_test_helpers::*;
use super::super::test_infrastructure::create_test_task_request_json;
use reqwest::StatusCode;

// =============================================================================
// Full Access API Key
// =============================================================================

#[tokio::test]
async fn full_access_granted() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);
    client.with_api_key(TEST_API_KEY_FULL);

    let response = client.get("/v1/tasks").await.expect("request failed");
    assert_ne!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "GET /v1/tasks with full-access API key should not return 401"
    );
    assert_ne!(
        response.status(),
        StatusCode::FORBIDDEN,
        "GET /v1/tasks with full-access API key should not return 403"
    );

    let task_body = create_test_task_request_json();
    let response = client
        .post_json("/v1/tasks", &task_body)
        .await
        .expect("request failed");
    assert_ne!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "POST /v1/tasks with full-access API key should not return 401"
    );
    assert_ne!(
        response.status(),
        StatusCode::FORBIDDEN,
        "POST /v1/tasks with full-access API key should not return 403"
    );

    server.shutdown().await.expect("shutdown failed");
}

// =============================================================================
// Read-Only API Key
// =============================================================================

#[tokio::test]
async fn read_only_list_allowed() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);
    client.with_api_key(TEST_API_KEY_READ_ONLY);

    let response = client.get("/v1/tasks").await.expect("request failed");
    assert_ne!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "GET /v1/tasks with read-only API key should not return 401"
    );
    assert_ne!(
        response.status(),
        StatusCode::FORBIDDEN,
        "GET /v1/tasks with read-only API key should not return 403"
    );

    server.shutdown().await.expect("shutdown failed");
}

#[tokio::test]
async fn read_only_create_denied() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);
    client.with_api_key(TEST_API_KEY_READ_ONLY);

    let task_body = create_test_task_request_json();
    let response = client
        .post_json("/v1/tasks", &task_body)
        .await
        .expect("request failed");
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "POST /v1/tasks with read-only API key should return 403 (got {})",
        response.status()
    );

    server.shutdown().await.expect("shutdown failed");
}

// =============================================================================
// Tasks-Only API Key
// =============================================================================

#[tokio::test]
async fn tasks_only_list_allowed() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);
    client.with_api_key(TEST_API_KEY_TASKS_ONLY);

    let response = client.get("/v1/tasks").await.expect("request failed");
    assert_ne!(
        response.status(),
        StatusCode::FORBIDDEN,
        "GET /v1/tasks with tasks-only API key should not return 403"
    );

    server.shutdown().await.expect("shutdown failed");
}

#[tokio::test]
async fn tasks_only_dlq_denied() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);
    client.with_api_key(TEST_API_KEY_TASKS_ONLY);

    let response = client.get("/v1/dlq").await.expect("request failed");
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "GET /v1/dlq with tasks-only API key should return 403 (got {})",
        response.status()
    );

    server.shutdown().await.expect("shutdown failed");
}

// =============================================================================
// No Permissions API Key
// =============================================================================

#[tokio::test]
async fn no_permissions_denied() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);
    client.with_api_key(TEST_API_KEY_NO_PERMS);

    let response = client.get("/v1/tasks").await.expect("request failed");
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "GET /v1/tasks with no-perms API key should return 403 (got {})",
        response.status()
    );

    server.shutdown().await.expect("shutdown failed");
}

// =============================================================================
// Invalid API Key
// =============================================================================

#[tokio::test]
async fn invalid_api_key_returns_401() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);
    client.with_api_key(INVALID_API_KEY);

    let response = client.get("/v1/tasks").await.expect("request failed");
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "GET /v1/tasks with invalid API key should return 401 (got {})",
        response.status()
    );

    server.shutdown().await.expect("shutdown failed");
}
