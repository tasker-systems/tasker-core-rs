//! Workflow step permission enforcement tests.
//!
//! Covers GET (list/get steps) and PATCH (resolve step) endpoints.
//!
//! NOTE: PATCH tests require a valid `StepManualAction` body because Axum
//! deserializes the request body (via the `Json<T>` extractor) before the
//! handler's `require_permission()` check runs. An invalid body returns 422
//! before authorization is evaluated. This is acceptable for now since all
//! requests are gated behind JWT/API key authentication — the caller has a
//! valid identity, they just lack the required permission. A future improvement
//! would move permission checks to route-level middleware so authorization is
//! evaluated before body extraction.

use super::super::auth_test_helpers::*;
use reqwest::StatusCode;

const FAKE_TASK_UUID: &str = "00000000-0000-0000-0000-000000000000";
const FAKE_STEP_UUID: &str = "00000000-0000-0000-0000-000000000001";

// =============================================================================
// GET /v1/tasks/{uuid}/workflow_steps - requires steps:read
// =============================================================================

#[tokio::test]
async fn steps_read_permission_grants_access() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);

    let token = generate_jwt(&["steps:read"]);
    client.with_jwt(&token);

    // Steps endpoint for a non-existent task should return 404, not 401/403
    let response = client
        .get(&format!("/v1/tasks/{FAKE_TASK_UUID}/workflow_steps"))
        .await
        .expect("request failed");
    assert_ne!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "GET workflow_steps with steps:read should not return 401"
    );
    assert_ne!(
        response.status(),
        StatusCode::FORBIDDEN,
        "GET workflow_steps with steps:read should not return 403"
    );

    server.shutdown().await.expect("shutdown failed");
}

#[tokio::test]
async fn tasks_wildcard_denied_on_steps() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);

    // tasks:* should NOT grant steps:read
    let token = generate_jwt(&["tasks:*"]);
    client.with_jwt(&token);

    let response = client
        .get(&format!("/v1/tasks/{FAKE_TASK_UUID}/workflow_steps"))
        .await
        .expect("request failed");
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "GET workflow_steps with tasks:* should return 403 (got {})",
        response.status()
    );

    server.shutdown().await.expect("shutdown failed");
}

#[tokio::test]
async fn no_credentials_on_steps_returns_401() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);
    client.without_auth();

    let response = client
        .get(&format!("/v1/tasks/{FAKE_TASK_UUID}/workflow_steps"))
        .await
        .expect("request failed");
    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "GET workflow_steps without credentials should return 401 (got {})",
        response.status()
    );

    server.shutdown().await.expect("shutdown failed");
}

// =============================================================================
// GET /v1/tasks/{uuid}/workflow_steps/{step_uuid} - requires steps:read
// =============================================================================

#[tokio::test]
async fn get_single_step_with_permission() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);

    let token = generate_jwt(&["steps:read"]);
    client.with_jwt(&token);

    let response = client
        .get(&format!(
            "/v1/tasks/{FAKE_TASK_UUID}/workflow_steps/{FAKE_STEP_UUID}"
        ))
        .await
        .expect("request failed");
    assert_ne!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "GET single step with steps:read should not return 401"
    );
    assert_ne!(
        response.status(),
        StatusCode::FORBIDDEN,
        "GET single step with steps:read should not return 403"
    );

    server.shutdown().await.expect("shutdown failed");
}

// =============================================================================
// PATCH /v1/tasks/{uuid}/workflow_steps/{step_uuid} - requires steps:resolve
// =============================================================================

#[tokio::test]
async fn resolve_step_with_permission() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);

    let token = generate_jwt(&["steps:resolve"]);
    client.with_jwt(&token);

    let body = serde_json::json!({
        "action_type": "resolve_manually",
        "reason": "test resolution",
        "resolved_by": "test-operator"
    });

    let response = client
        .patch_json(
            &format!("/v1/tasks/{FAKE_TASK_UUID}/workflow_steps/{FAKE_STEP_UUID}"),
            &body,
        )
        .await
        .expect("request failed");
    assert_ne!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "PATCH step with steps:resolve should not return 401"
    );
    assert_ne!(
        response.status(),
        StatusCode::FORBIDDEN,
        "PATCH step with steps:resolve should not return 403"
    );

    server.shutdown().await.expect("shutdown failed");
}

#[tokio::test]
async fn resolve_step_denied_without_permission() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);

    // steps:read is NOT sufficient for PATCH (resolve)
    let token = generate_jwt(&["steps:read"]);
    client.with_jwt(&token);

    let body = serde_json::json!({
        "action_type": "resolve_manually",
        "reason": "test resolution",
        "resolved_by": "test-operator"
    });

    let response = client
        .patch_json(
            &format!("/v1/tasks/{FAKE_TASK_UUID}/workflow_steps/{FAKE_STEP_UUID}"),
            &body,
        )
        .await
        .expect("request failed");
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "PATCH step with only steps:read should return 403 (got {})",
        response.status()
    );

    server.shutdown().await.expect("shutdown failed");
}

#[tokio::test]
async fn resolve_step_denied_with_tasks_wildcard() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);

    // tasks:* does NOT cover steps:resolve
    let token = generate_jwt(&["tasks:*"]);
    client.with_jwt(&token);

    let body = serde_json::json!({
        "action_type": "resolve_manually",
        "reason": "test resolution",
        "resolved_by": "test-operator"
    });

    let response = client
        .patch_json(
            &format!("/v1/tasks/{FAKE_TASK_UUID}/workflow_steps/{FAKE_STEP_UUID}"),
            &body,
        )
        .await
        .expect("request failed");
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "PATCH step with tasks:* should return 403 (got {})",
        response.status()
    );

    server.shutdown().await.expect("shutdown failed");
}

#[tokio::test]
async fn resolve_step_with_steps_wildcard() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);

    // steps:* should grant steps:resolve
    let token = generate_jwt(&["steps:*"]);
    client.with_jwt(&token);

    let body = serde_json::json!({
        "action_type": "resolve_manually",
        "reason": "test resolution",
        "resolved_by": "test-operator"
    });

    let response = client
        .patch_json(
            &format!("/v1/tasks/{FAKE_TASK_UUID}/workflow_steps/{FAKE_STEP_UUID}"),
            &body,
        )
        .await
        .expect("request failed");
    assert_ne!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "PATCH step with steps:* should not return 401"
    );
    assert_ne!(
        response.status(),
        StatusCode::FORBIDDEN,
        "PATCH step with steps:* should not return 403"
    );

    server.shutdown().await.expect("shutdown failed");
}

#[tokio::test]
async fn global_wildcard_rejected() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);

    // Global wildcard (*) is NOT supported - should be treated as invalid permission
    let token = generate_jwt(&["*"]);
    client.with_jwt(&token);

    let body = serde_json::json!({
        "action_type": "resolve_manually",
        "reason": "test resolution",
        "resolved_by": "test-operator"
    });

    let response = client
        .patch_json(
            &format!("/v1/tasks/{FAKE_TASK_UUID}/workflow_steps/{FAKE_STEP_UUID}"),
            &body,
        )
        .await
        .expect("request failed");
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "PATCH step with global wildcard (*) should return 403 (global wildcards not supported)"
    );

    server.shutdown().await.expect("shutdown failed");
}
