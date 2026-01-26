//! Task CRUD permission enforcement tests.

use super::super::auth_test_helpers::*;
use super::super::test_infrastructure::create_test_task_request_json;
use reqwest::StatusCode;

// =============================================================================
// JWT Full Permissions (All Resource Wildcards)
// =============================================================================

#[tokio::test]
async fn full_permissions_access_granted() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);

    // Use explicit resource wildcards for full access (global * is not supported)
    let token = generate_jwt(&[
        "tasks:*",
        "steps:*",
        "dlq:*",
        "templates:*",
        "system:*",
        "worker:*",
    ]);
    client.with_jwt(&token);

    // All read endpoints should succeed (200 or 404 for missing resources, not 401/403)
    // TAS-76: /v1/handlers renamed to /v1/templates
    let endpoints = ["/v1/tasks", "/v1/templates", "/config"];

    for path in endpoints {
        let response = client.get(path).await.expect("request failed");
        assert_ne!(
            response.status(),
            StatusCode::UNAUTHORIZED,
            "GET {path} with full resource wildcards should not return 401"
        );
        assert_ne!(
            response.status(),
            StatusCode::FORBIDDEN,
            "GET {path} with full resource wildcards should not return 403"
        );
    }

    // POST task creation should not be rejected for auth reasons
    let task_body = create_test_task_request_json();
    let response = client
        .post_json("/v1/tasks", &task_body)
        .await
        .expect("request failed");
    assert_ne!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "POST /v1/tasks with full resource wildcards should not return 401"
    );
    assert_ne!(
        response.status(),
        StatusCode::FORBIDDEN,
        "POST /v1/tasks with full resource wildcards should not return 403"
    );

    server.shutdown().await.expect("shutdown failed");
}

// =============================================================================
// JWT Read-Only Permissions
// =============================================================================

#[tokio::test]
async fn read_only_list_tasks_allowed() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);

    let token = generate_jwt(&["tasks:list", "tasks:read"]);
    client.with_jwt(&token);

    let response = client.get("/v1/tasks").await.expect("request failed");
    assert_ne!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "GET /v1/tasks with tasks:list should not return 401"
    );
    assert_ne!(
        response.status(),
        StatusCode::FORBIDDEN,
        "GET /v1/tasks with tasks:list should not return 403"
    );

    server.shutdown().await.expect("shutdown failed");
}

#[tokio::test]
async fn read_only_create_task_denied() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);

    let token = generate_jwt(&["tasks:list", "tasks:read"]);
    client.with_jwt(&token);

    let task_body = create_test_task_request_json();
    let response = client
        .post_json("/v1/tasks", &task_body)
        .await
        .expect("request failed");
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "POST /v1/tasks with only read perms should return 403 (got {})",
        response.status()
    );

    server.shutdown().await.expect("shutdown failed");
}

#[tokio::test]
async fn read_only_cancel_task_denied() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);

    let token = generate_jwt(&["tasks:list", "tasks:read"]);
    client.with_jwt(&token);

    let response = client
        .delete("/v1/tasks/00000000-0000-0000-0000-000000000000")
        .await
        .expect("request failed");
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "DELETE /v1/tasks/... with only read perms should return 403 (got {})",
        response.status()
    );

    server.shutdown().await.expect("shutdown failed");
}

// =============================================================================
// Wildcard tasks:*
// =============================================================================

#[tokio::test]
async fn wildcard_grants_all_task_ops() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);

    let token = generate_jwt(&["tasks:*"]);
    client.with_jwt(&token);

    // List tasks should work
    let response = client.get("/v1/tasks").await.expect("request failed");
    assert_ne!(
        response.status(),
        StatusCode::FORBIDDEN,
        "GET /v1/tasks with tasks:* should not return 403"
    );

    // Create task should work (not auth denied)
    let task_body = create_test_task_request_json();
    let response = client
        .post_json("/v1/tasks", &task_body)
        .await
        .expect("request failed");
    assert_ne!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "POST /v1/tasks with tasks:* should not return 401"
    );
    assert_ne!(
        response.status(),
        StatusCode::FORBIDDEN,
        "POST /v1/tasks with tasks:* should not return 403"
    );

    server.shutdown().await.expect("shutdown failed");
}

// =============================================================================
// Wrong Resource Scope
// =============================================================================

#[tokio::test]
async fn tasks_only_denied_on_config() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);

    let token = generate_jwt(&["tasks:list", "tasks:read", "tasks:create"]);
    client.with_jwt(&token);

    let response = client.get("/config").await.expect("request failed");
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "/config with only tasks perms should return 403 (got {})",
        response.status()
    );

    server.shutdown().await.expect("shutdown failed");
}

#[tokio::test]
async fn tasks_only_denied_on_templates() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);

    let token = generate_jwt(&["tasks:list", "tasks:read", "tasks:create"]);
    client.with_jwt(&token);

    // TAS-76: Route renamed from /v1/handlers to /v1/templates
    let response = client.get("/v1/templates").await.expect("request failed");
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "/v1/templates with only tasks perms should return 403 (got {})",
        response.status()
    );

    server.shutdown().await.expect("shutdown failed");
}

#[tokio::test]
async fn tasks_wildcard_denied_on_dlq() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);

    let token = generate_jwt(&["tasks:*"]);
    client.with_jwt(&token);

    let response = client.get("/v1/dlq").await.expect("request failed");
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "GET /v1/dlq with tasks:* should return 403 (got {})",
        response.status()
    );

    server.shutdown().await.expect("shutdown failed");
}
