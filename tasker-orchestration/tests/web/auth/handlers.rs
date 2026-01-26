//! Template discovery permission enforcement tests.
//!
//! TAS-76: Renamed from handlers.rs to match /v1/templates API.

use super::super::auth_test_helpers::*;
use reqwest::StatusCode;

#[tokio::test]
async fn templates_read_permission_grants_access() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);

    // TAS-76: Permission is now templates:read instead of system:handlers_read
    let token = generate_jwt(&["templates:read"]);
    client.with_jwt(&token);

    let response = client.get("/v1/templates").await.expect("request failed");
    assert_ne!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "GET /v1/templates with templates:read perm should not return 401"
    );
    assert_ne!(
        response.status(),
        StatusCode::FORBIDDEN,
        "GET /v1/templates with templates:read perm should not return 403"
    );

    server.shutdown().await.expect("shutdown failed");
}
