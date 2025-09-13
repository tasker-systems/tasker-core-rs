//! # Integration Test Suite
//!
//! Provides a unified setup and teardown infrastructure for integration tests
//! that require PostgreSQL, orchestration service, worker service, and API client.

use anyhow::Result;
use reqwest;
use std::time::Duration;
use testcontainers::{ContainerAsync, GenericImage};
use tokio::time::sleep;

use crate::test_helpers::testcontainers::{
    BuildMode, OrchestrationService, PgmqPostgres, WorkerService,
};
use crate::test_helpers::{setup_test_environment, MIGRATOR};
use tasker_client::{OrchestrationApiClient, OrchestrationApiConfig};

/// Complete integration test environment with all services
pub struct IntegrationTestSuite {
    pub postgres_container: ContainerAsync<GenericImage>,
    pub orchestration_container: ContainerAsync<GenericImage>,
    pub worker_container: ContainerAsync<GenericImage>,
    pub database_url: String,
    pub orchestration_url: String,
    pub worker_url: String,
    pub worker_id: String,
    pub client: OrchestrationApiClient,
    pub database_pool: sqlx::PgPool,
}

impl IntegrationTestSuite {
    /// Set up complete integration test environment
    ///
    /// This sets up:
    /// - PostgreSQL container with PGMQ
    /// - Database migrations
    /// - Orchestration service container
    /// - Rust worker container
    /// - API client configured and ready
    pub async fn setup() -> Result<Self> {
        Self::setup_with_worker_id("test-worker-001").await
    }

    /// Set up integration test environment with custom worker ID
    pub async fn setup_with_worker_id(worker_id: &str) -> Result<Self> {
        setup_test_environment();
        println!("🚀 Setting up Integration Test Suite");

        // Step 1: Start PostgreSQL container with PGMQ
        println!("\n📦 Step 1: Starting PostgreSQL container with PGMQ...");
        let postgres = PgmqPostgres::new()
            .with_db_name("tasker_rust_test")
            .with_user("tasker")
            .with_password("tasker");

        let postgres_container = postgres.start_async().await?;
        let database_url = postgres.connection_string_async(&postgres_container).await;

        println!("✅ PostgreSQL container started");
        println!("   Database URL: {}", database_url);

        // Step 2: Run migrations
        println!("\n🔄 Step 2: Running database migrations...");
        let database_pool = sqlx::PgPool::connect(&database_url).await?;
        MIGRATOR.run(&database_pool).await?;
        println!("✅ Database migrations completed");

        // Step 3: Start orchestration service container
        println!("\n📦 Step 3: Starting Orchestration service container...");
        let orchestration_service =
            OrchestrationService::new(database_url.clone(), BuildMode::Fast);

        let orchestration_container = orchestration_service.start_async().await?;
        let orchestration_url = orchestration_service
            .service_url_async(&orchestration_container)
            .await;

        println!("✅ Orchestration service container started");
        println!("   Service URL: {}", orchestration_url);

        // Wait for orchestration service to be healthy
        println!("🔄 Waiting for orchestration service health...");
        Self::wait_for_service_health(&orchestration_url, 10).await?;

        // Step 4: Start rust worker container
        println!("\n📦 Step 4: Starting Rust Worker container...");
        let worker_service =
            WorkerService::new(database_url.clone(), BuildMode::Fast, worker_id.to_string())
                .with_orchestration_url(&orchestration_url);

        let worker_container = worker_service.start_async().await?;
        let worker_url = worker_service.service_url_async(&worker_container).await;

        println!("✅ Rust Worker container started");
        println!("   Worker URL: {}", worker_url);
        println!("   Worker ID: {}", worker_service.worker_id());

        // Give the worker time to register with orchestration
        println!("🔄 Allowing worker registration time...");
        sleep(Duration::from_secs(5)).await;

        // Step 5: Create tasker-client and test task creation
        println!("\n🔧 Step 5: Setting up tasker-client...");

        let client_config = OrchestrationApiConfig {
            base_url: orchestration_url.clone(),
            timeout_ms: 30000,
            max_retries: 3,
            auth: None, // No auth for test environment
        };

        let client = OrchestrationApiClient::new(client_config)?;
        println!("✅ Orchestration API client configured");

        println!("🎉 Integration Test Suite setup complete!");

        Ok(IntegrationTestSuite {
            postgres_container,
            orchestration_container,
            worker_container,
            database_url,
            orchestration_url,
            worker_url,
            worker_id: worker_id.to_string(),
            client,
            database_pool,
        })
    }

    /// Wait for service health with exponential backoff
    async fn wait_for_service_health(base_url: &str, max_attempts: usize) -> Result<()> {
        let client = reqwest::Client::new();

        for attempt in 1..=max_attempts {
            match client.get(&format!("{}/health", base_url)).send().await {
                Ok(response) if response.status().is_success() => {
                    println!("✅ Service healthy at {} (attempt {})", base_url, attempt);
                    return Ok(());
                }
                Ok(response) => {
                    println!(
                        "🔄 Service not ready: HTTP {} (attempt {}/{})",
                        response.status(),
                        attempt,
                        max_attempts
                    );
                }
                Err(e) => {
                    println!(
                        "🔄 Service unreachable: {} (attempt {}/{})",
                        e, attempt, max_attempts
                    );
                }
            }

            if attempt < max_attempts {
                let delay_ms = 2_u64.pow(attempt as u32 - 1) * 1000;
                println!("   Waiting {}ms before retry...", delay_ms);
                sleep(Duration::from_millis(delay_ms)).await;
            }
        }

        Err(anyhow::anyhow!(
            "Service failed to become healthy after {} attempts",
            max_attempts
        ))
    }
}

/// Simplified setup for API-only testing (no worker required)
pub struct ApiOnlyTestSuite {
    pub postgres_container: ContainerAsync<GenericImage>,
    pub orchestration_container: ContainerAsync<GenericImage>,
    pub database_url: String,
    pub orchestration_url: String,
    pub client: OrchestrationApiClient,
    pub database_pool: sqlx::PgPool,
}

impl ApiOnlyTestSuite {
    /// Set up API-only test environment (PostgreSQL + Orchestration only)
    pub async fn setup() -> Result<Self> {
        setup_test_environment();
        println!("🔧 Setting up API-Only Test Suite");

        // Start PostgreSQL and orchestration service only (no worker needed for API tests)
        let postgres = PgmqPostgres::new()
            .with_db_name("tasker_api_test")
            .with_user("tasker")
            .with_password("tasker");

        let postgres_container = postgres.start_async().await?;
        let database_url = postgres.connection_string_async(&postgres_container).await;

        // Run migrations
        let database_pool = sqlx::PgPool::connect(&database_url).await?;
        MIGRATOR.run(&database_pool).await?;

        // Start orchestration service
        let orchestration_service =
            OrchestrationService::new(database_url.clone(), BuildMode::Fast);
        let orchestration_container = orchestration_service.start_async().await?;
        let orchestration_url = orchestration_service
            .service_url_async(&orchestration_container)
            .await;

        // Wait for service health
        IntegrationTestSuite::wait_for_service_health(&orchestration_url, 10).await?;

        // Test API client functionality
        let client_config = OrchestrationApiConfig {
            base_url: orchestration_url.clone(),
            timeout_ms: 15000,
            max_retries: 3,
            auth: None,
        };

        let client = OrchestrationApiClient::new(client_config)?;

        println!("✅ API-Only Test Suite setup complete!");

        Ok(ApiOnlyTestSuite {
            postgres_container,
            orchestration_container,
            database_url,
            orchestration_url,
            client,
            database_pool,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_integration_suite_setup() -> Result<()> {
        let _suite = IntegrationTestSuite::setup().await?;
        // Suite automatically cleans up when dropped
        Ok(())
    }

    #[tokio::test]
    async fn test_api_only_suite_setup() -> Result<()> {
        let suite = ApiOnlyTestSuite::setup().await?;

        // Test that we can make a health check
        let health = suite.client.get_basic_health().await?;
        assert_eq!(health.status, "healthy");

        Ok(())
    }
}
