//! Dead letter queue permission enforcement tests.

use super::super::auth_test_helpers::*;
use reqwest::StatusCode;

#[tokio::test]
async fn dlq_read_permission_grants_access() {
    let server = AuthTestServer::start()
        .await
        .expect("Failed to start auth test server");
    let mut client = AuthWebTestClient::for_server(&server);

    let token = generate_jwt(&["dlq:read", "dlq:stats"]);
    client.with_jwt(&token);

    // DLQ list
    let response = client.get("/v1/dlq").await.expect("request failed");
    assert_ne!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "GET /v1/dlq with dlq:read should not return 401"
    );
    assert_ne!(
        response.status(),
        StatusCode::FORBIDDEN,
        "GET /v1/dlq with dlq:read should not return 403"
    );

    // DLQ stats
    let response = client.get("/v1/dlq/stats").await.expect("request failed");
    assert_ne!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "GET /v1/dlq/stats with dlq:stats should not return 401"
    );
    assert_ne!(
        response.status(),
        StatusCode::FORBIDDEN,
        "GET /v1/dlq/stats with dlq:stats should not return 403"
    );

    server.shutdown().await.expect("shutdown failed");
}
