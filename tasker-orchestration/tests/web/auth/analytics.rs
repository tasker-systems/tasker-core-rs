//! Analytics endpoint permission enforcement tests.

use super::super::auth_test_helpers::*;
use reqwest::StatusCode;

#[tokio::test]
async fn analytics_permission_grants_access() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);

    let token = generate_jwt(&["system:analytics_read"]);
    client.with_jwt(&token);

    let response = client
        .get("/v1/analytics/performance")
        .await
        .expect("request failed");
    assert_ne!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "GET /v1/analytics/performance with analytics perm should not return 401"
    );
    assert_ne!(
        response.status(),
        StatusCode::FORBIDDEN,
        "GET /v1/analytics/performance with analytics perm should not return 403"
    );

    server.shutdown().await.expect("shutdown failed");
}

#[tokio::test]
async fn analytics_denied_without_permission() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);

    let token = generate_jwt(&["tasks:list"]);
    client.with_jwt(&token);

    let response = client
        .get("/v1/analytics/performance")
        .await
        .expect("request failed");
    assert_eq!(
        response.status(),
        StatusCode::FORBIDDEN,
        "GET /v1/analytics/performance without analytics perm should return 403 (got {})",
        response.status()
    );

    server.shutdown().await.expect("shutdown failed");
}
