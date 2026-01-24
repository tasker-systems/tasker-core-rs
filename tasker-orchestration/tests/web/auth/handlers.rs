//! Handler registry permission enforcement tests.

use super::super::auth_test_helpers::*;
use reqwest::StatusCode;

#[tokio::test]
async fn handlers_read_permission_grants_access() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);

    let token = generate_jwt(&["system:handlers_read"]);
    client.with_jwt(&token);

    let response = client.get("/v1/handlers").await.expect("request failed");
    assert_ne!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "GET /v1/handlers with handlers_read perm should not return 401"
    );
    assert_ne!(
        response.status(),
        StatusCode::FORBIDDEN,
        "GET /v1/handlers with handlers_read perm should not return 403"
    );

    server.shutdown().await.expect("shutdown failed");
}
