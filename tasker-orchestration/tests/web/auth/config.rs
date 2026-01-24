//! Config endpoint permission enforcement tests.

use super::super::auth_test_helpers::*;
use reqwest::StatusCode;

#[tokio::test]
async fn config_read_permission_grants_access() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);

    let token = generate_jwt(&["system:config_read"]);
    client.with_jwt(&token);

    let response = client.get("/config").await.expect("request failed");
    assert_ne!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "GET /config with config_read perm should not return 401"
    );
    assert_ne!(
        response.status(),
        StatusCode::FORBIDDEN,
        "GET /config with config_read perm should not return 403"
    );

    server.shutdown().await.expect("shutdown failed");
}
