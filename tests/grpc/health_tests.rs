//! gRPC health endpoint tests (TAS-177).
//!
//! Tests for gRPC health endpoints to verify service connectivity and response structure.

use anyhow::Result;
use tasker_client::{GrpcOrchestrationClient, OrchestrationClient};

use crate::common::integration_test_manager::IntegrationTestManager;

/// Test basic gRPC health check connectivity
#[tokio::test]
async fn test_grpc_health_check_connectivity() -> Result<()> {
    let manager = IntegrationTestManager::setup_grpc_orchestration_only().await?;

    // Health check should succeed
    manager.unified_client.health_check().await?;

    println!("gRPC health check connectivity: OK");
    Ok(())
}

/// Test gRPC basic health response structure
#[tokio::test]
async fn test_grpc_basic_health_response() -> Result<()> {
    let manager = IntegrationTestManager::setup_grpc_orchestration_only().await?;

    let health = manager.unified_client.get_basic_health().await?;

    assert_eq!(
        health.status, "healthy",
        "Health status should be 'healthy'"
    );
    assert!(
        !health.timestamp.is_empty(),
        "Health timestamp should not be empty"
    );

    println!(
        "gRPC basic health response: status={}, timestamp={}",
        health.status, health.timestamp
    );
    Ok(())
}

/// Test gRPC detailed health response structure
#[tokio::test]
async fn test_grpc_detailed_health_response() -> Result<()> {
    let manager = IntegrationTestManager::setup_grpc_orchestration_only().await?;

    let health = manager.unified_client.get_detailed_health().await?;

    // Status can be "healthy" or "degraded" depending on system state
    assert!(
        health.status == "healthy" || health.status == "degraded",
        "Detailed health status should be 'healthy' or 'degraded', got: {}",
        health.status
    );

    // Check info section
    assert!(
        !health.info.version.is_empty(),
        "Version should not be empty"
    );

    // Check some health check subsections
    assert_eq!(
        health.checks.web_database.status, "healthy",
        "Web database should be healthy"
    );
    assert_eq!(
        health.checks.orchestration_database.status, "healthy",
        "Orchestration database should be healthy"
    );

    println!(
        "gRPC detailed health: version={}, web_db={}, orch_db={}",
        health.info.version,
        health.checks.web_database.status,
        health.checks.orchestration_database.status
    );
    Ok(())
}

/// Test gRPC client creation directly
#[tokio::test]
async fn test_grpc_client_direct_creation() -> Result<()> {
    // Get gRPC URL from environment or use default
    let grpc_url = std::env::var("TASKER_TEST_ORCHESTRATION_GRPC_URL")
        .unwrap_or_else(|_| "http://localhost:9190".to_string());

    let client = GrpcOrchestrationClient::connect(&grpc_url).await?;

    // Verify transport name
    assert_eq!(client.transport_name(), "gRPC");
    assert_eq!(client.endpoint(), &grpc_url);

    // Health check should work
    client.health_check().await?;

    println!("Direct gRPC client creation: OK at {}", grpc_url);
    Ok(())
}

/// Test gRPC client with unified wrapper
#[tokio::test]
async fn test_grpc_unified_client() -> Result<()> {
    let manager = IntegrationTestManager::setup_grpc_orchestration_only().await?;

    // Verify we're using gRPC transport
    assert!(manager.is_grpc(), "Should be using gRPC transport");
    assert!(!manager.is_rest(), "Should not be using REST transport");

    // Get unified client
    let client = manager.client();
    assert_eq!(
        client.transport_name(),
        "gRPC",
        "Unified client should report gRPC transport"
    );

    println!("Unified gRPC client transport: {}", client.transport_name());
    Ok(())
}
