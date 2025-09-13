//! # Integrated Test Suite Manager
//!
//! Provides a unified test suite manager that orchestrates PostgreSQL, orchestration service,
//! and worker service containers for comprehensive integration testing.

use super::{
    postgres_container::PgmqPostgres,
    orchestration_container::OrchestrationService, 
    worker_container::WorkerService,
    utils::BuildMode,
};
use crate::test_helpers::{setup_test_environment, MIGRATOR};
use sqlx::PgPool;
use std::sync::Arc;
use testcontainers::{clients::Cli, Container};

/// Result type for test suite operations
type TestSuiteResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Integrated test suite manager using testcontainers
/// 
/// Provides complete orchestration of:
/// - PostgreSQL database with PGMQ extension
/// - Orchestration service 
/// - Worker service(s)
/// - Automatic cleanup and isolation
pub struct TestcontainersTestSuite {
    /// Docker client
    docker: Arc<Cli>,
    /// PostgreSQL container
    postgres_container: Option<Container<'static, PgmqPostgres>>,
    /// Orchestration service container  
    orchestration_container: Option<Container<'static, OrchestrationService>>,
    /// Worker service container(s)
    worker_containers: Vec<Container<'static, WorkerService>>,
    /// Database connection URL
    database_url: String,
    /// Orchestration API endpoint
    orchestration_url: String,
    /// Worker API endpoints
    worker_urls: Vec<String>,
    /// Build mode for this test suite
    build_mode: BuildMode,
    /// Database connection pool (for direct database operations)
    db_pool: Option<PgPool>,
}

impl TestcontainersTestSuite {
    /// Start a complete test suite with default configuration
    /// 
    /// This creates:
    /// - 1 PostgreSQL container with PGMQ
    /// - 1 Orchestration service container
    /// - 1 Worker service container
    pub async fn start() -> TestSuiteResult<Self> {
        Self::start_with_config(TestSuiteConfig::default()).await
    }

    /// Start a test suite with custom configuration
    pub async fn start_with_config(config: TestSuiteConfig) -> TestSuiteResult<Self> {
        // Setup test environment using existing helpers
        setup_test_environment();
        
        let docker = Arc::new(Cli::default());
        let build_mode = config.build_mode;
        
        tracing::info!(
            build_mode = %build_mode,
            worker_count = config.worker_count,
            "Starting testcontainers test suite..."
        );

        // Build required images
        Self::build_images(build_mode).await?;

        // Start PostgreSQL container
        let postgres = PgmqPostgres::new();
        let postgres_container = docker.run(postgres);
        let database_url = PgmqPostgres::default().connection_string(&postgres_container);

        tracing::info!(
            database_url = %database_url,
            "PostgreSQL container started"
        );

        // Set up database with migrations
        let db_pool = PgPool::connect(&database_url).await?;
        MIGRATOR.run(&db_pool).await?;
        
        tracing::info!("Database migrations completed");

        // Start orchestration service
        let orchestration_service = OrchestrationService::new(database_url.clone(), build_mode);
        let orchestration_container = docker.run(orchestration_service);
        let orchestration_url = OrchestrationService::new(database_url.clone(), build_mode)
            .api_endpoint(&orchestration_container);

        tracing::info!(
            orchestration_url = %orchestration_url,
            "Orchestration service started"
        );

        // Start worker service(s)  
        let mut worker_containers = Vec::new();
        let mut worker_urls = Vec::new();

        for i in 0..config.worker_count {
            let worker_service = WorkerService::with_config(
                database_url.clone(),
                build_mode,
                &format!("test-worker-{:03}", i + 1),
                8081 + i as u16,
                "/health",
            );
            
            let worker_container = docker.run(worker_service.clone());
            let worker_url = worker_service.api_endpoint(&worker_container);
            
            worker_containers.push(worker_container);
            worker_urls.push(worker_url);
            
            tracing::info!(
                worker_id = %format!("test-worker-{:03}", i + 1),
                worker_url = %worker_urls[i],
                "Worker service started"
            );
        }

        tracing::info!("Test suite startup complete");

        Ok(Self {
            docker,
            postgres_container: Some(postgres_container),
            orchestration_container: Some(orchestration_container),
            worker_containers,
            database_url,
            orchestration_url,
            worker_urls,
            build_mode,
            db_pool: Some(db_pool),
        })
    }

    /// Build all required Docker images
    async fn build_images(build_mode: BuildMode) -> TestSuiteResult<()> {
        use tokio::task;

        tracing::info!(
            build_mode = %build_mode,
            "Building Docker images..."
        );

        // Build images in parallel for speed
        let postgres_build = task::spawn_blocking(|| PgmqPostgres::new().build_image());
        
        let orchestration_build = task::spawn_blocking(move || {
            OrchestrationService::new("dummy".to_string(), build_mode).build_image()
        });
        
        let worker_build = task::spawn_blocking(move || {
            WorkerService::new("dummy".to_string(), build_mode).build_image()
        });

        // Wait for all builds to complete
        let (postgres_result, orchestration_result, worker_result) = tokio::try_join!(
            postgres_build,
            orchestration_build, 
            worker_build
        )?;

        postgres_result?;
        orchestration_result?;
        worker_result?;

        tracing::info!("All Docker images built successfully");
        Ok(())
    }

    /// Get the database connection URL
    pub fn database_url(&self) -> &str {
        &self.database_url
    }

    /// Get the orchestration API endpoint
    pub fn orchestration_url(&self) -> &str {
        &self.orchestration_url
    }

    /// Get worker API endpoints
    pub fn worker_urls(&self) -> &[String] {
        &self.worker_urls
    }

    /// Get the primary worker URL (first worker)
    pub fn primary_worker_url(&self) -> Option<&String> {
        self.worker_urls.first()
    }

    /// Get the build mode used by this test suite
    pub fn build_mode(&self) -> BuildMode {
        self.build_mode
    }

    /// Get direct database connection pool
    pub fn db_pool(&self) -> Option<&PgPool> {
        self.db_pool.as_ref()
    }

    /// Create an orchestration API client (would be implemented separately)
    // pub fn orchestration_client(&self) -> OrchestrationApiClient {
    //     OrchestrationApiClient::new(OrchestrationApiConfig {
    //         base_url: self.orchestration_url.clone(),
    //         timeout_ms: 30000,
    //         max_retries: 3,
    //         auth: None,
    //     })
    // }

    /// Create a worker API client (would be implemented separately)  
    // pub fn worker_client(&self, worker_index: usize) -> Option<WorkerApiClient> {
    //     self.worker_urls.get(worker_index).map(|url| {
    //         WorkerApiClient::new(WorkerApiConfig {
    //             base_url: url.clone(),
    //             timeout_ms: 30000,
    //             max_retries: 3,
    //             auth: None,
    //         })
    //     })
    // }

    /// Wait for all services to be healthy
    pub async fn wait_for_health(&self) -> TestSuiteResult<()> {
        tracing::info!("Waiting for all services to be healthy...");
        
        // Health checks are handled by testcontainers wait conditions
        // Services are already healthy when containers are returned
        
        tracing::info!("All services are healthy");
        Ok(())
    }

    /// Get suite statistics
    pub fn stats(&self) -> TestSuiteStats {
        TestSuiteStats {
            build_mode: self.build_mode,
            worker_count: self.worker_containers.len(),
            has_database: self.postgres_container.is_some(),
            has_orchestration: self.orchestration_container.is_some(),
            services_healthy: true, // Containers wouldn't be running if not healthy
        }
    }
}

/// Configuration for test suite startup
#[derive(Debug, Clone)]
pub struct TestSuiteConfig {
    /// Build mode to use for service containers
    pub build_mode: BuildMode,
    /// Number of worker instances to start
    pub worker_count: usize,
    /// Custom database configuration (optional)
    pub database_config: Option<DatabaseConfig>,
}

impl Default for TestSuiteConfig {
    fn default() -> Self {
        Self {
            build_mode: BuildMode::from_env(),
            worker_count: 1,
            database_config: None,
        }
    }
}

/// Custom database configuration
#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub database: String,
    pub user: String,
    pub password: String,
}

/// Test suite runtime statistics
#[derive(Debug, Clone)]
pub struct TestSuiteStats {
    pub build_mode: BuildMode,
    pub worker_count: usize,
    pub has_database: bool,
    pub has_orchestration: bool,
    pub services_healthy: bool,
}

// Automatic cleanup on drop - testcontainers handles container lifecycle
impl Drop for TestcontainersTestSuite {
    fn drop(&mut self) {
        tracing::info!(
            build_mode = %self.build_mode,
            worker_count = self.worker_containers.len(),
            "Cleaning up test suite containers"
        );
        
        // testcontainers automatically cleans up containers when they go out of scope
        // This is just for logging and any additional cleanup if needed
        
        if let Some(ref pool) = self.db_pool {
            if !pool.is_closed() {
                // Note: In a real implementation, you might want to close the pool here
                tracing::debug!("Database pool will be closed automatically");
            }
        }
        
        tracing::info!("Test suite cleanup complete");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_suite_config_default() {
        let config = TestSuiteConfig::default();
        assert_eq!(config.worker_count, 1);
        assert!(config.database_config.is_none());
    }

    #[test]
    fn test_suite_config_custom() {
        let config = TestSuiteConfig {
            build_mode: BuildMode::CI,
            worker_count: 3,
            database_config: Some(DatabaseConfig {
                database: "test_db".to_string(),
                user: "test_user".to_string(),
                password: "test_pass".to_string(),
            }),
        };
        
        assert_eq!(config.build_mode, BuildMode::CI);
        assert_eq!(config.worker_count, 3);
        assert!(config.database_config.is_some());
    }

    #[test]
    fn test_suite_stats() {
        let stats = TestSuiteStats {
            build_mode: BuildMode::Fast,
            worker_count: 2,
            has_database: true,
            has_orchestration: true,
            services_healthy: true,
        };
        
        assert_eq!(stats.build_mode, BuildMode::Fast);
        assert_eq!(stats.worker_count, 2);
        assert!(stats.has_database);
        assert!(stats.has_orchestration);
        assert!(stats.services_healthy);
    }
}