//! Integration tests for worker reachability testing functionality

use serde_json;
use sqlx::PgPool;
use std::time::Duration;
use tasker_core::services::worker_selection_service::WorkerSelectionService;

#[tokio::test]
async fn test_tcp_reachability_unreachable_host() {
    // Create service with dummy pool for testing connection logic
    let service = WorkerSelectionService::new(
        PgPool::connect_lazy("postgresql://localhost/dummy").unwrap(),
        "test_core".to_string(),
    );

    // Test connection to a non-existent host
    let connection_details = serde_json::json!({
        "host": "nonexistent.invalid.host",
        "port": 12345
    });

    let result = service.test_tcp_connection(&connection_details).await;
    assert!(!result, "Connection should fail to non-existent host");
}

#[tokio::test]
async fn test_tcp_reachability_timeout() {
    let service = WorkerSelectionService::new(
        PgPool::connect_lazy("postgresql://localhost/dummy").unwrap(),
        "test_core".to_string(),
    );

    // Test connection to a non-routable IP (reserved for documentation)
    let connection_details = serde_json::json!({
        "host": "192.0.2.1", // Non-routable test IP
        "port": 80
    });

    let start = std::time::Instant::now();
    let result = service.test_tcp_connection(&connection_details).await;
    let duration = start.elapsed();

    assert!(!result, "Connection should fail to non-routable IP");
    assert!(
        duration < Duration::from_secs(5),
        "Should timeout within reasonable time"
    );
    assert!(
        duration >= Duration::from_secs(2),
        "Should respect minimum timeout"
    );
}

#[tokio::test]
async fn test_unix_socket_reachability_nonexistent() {
    let service = WorkerSelectionService::new(
        PgPool::connect_lazy("postgresql://localhost/dummy").unwrap(),
        "test_core".to_string(),
    );

    // Test connection to a non-existent Unix socket
    let connection_details = serde_json::json!({
        "socket_path": "/tmp/nonexistent_socket_12345.sock"
    });

    let result = service.test_unix_connection(&connection_details).await;
    assert!(!result, "Connection should fail to non-existent socket");
}

#[tokio::test]
async fn test_connection_type_routing() {
    let service = WorkerSelectionService::new(
        PgPool::connect_lazy("postgresql://localhost/dummy").unwrap(),
        "test_core".to_string(),
    );

    // Test that TCP connections are routed correctly
    let tcp_details = serde_json::json!({
        "host": "nonexistent.host",
        "port": 9999
    });

    let tcp_result = service.test_connection("tcp", &tcp_details).await;
    assert!(
        !tcp_result,
        "TCP connection should fail to non-existent host"
    );

    // Test that Unix connections are routed correctly
    let unix_details = serde_json::json!({
        "socket_path": "/tmp/nonexistent.sock"
    });

    let unix_result = service.test_connection("unix", &unix_details).await;
    assert!(
        !unix_result,
        "Unix connection should fail to non-existent socket"
    );

    // Test unknown transport type
    let unknown_result = service.test_connection("unknown", &tcp_details).await;
    assert!(
        !unknown_result,
        "Unknown transport type should return false"
    );
}
