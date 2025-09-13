//! # Worker Service Container
//!
//! Worker service container using ImageFromDockerfile with centralized docker/build structure

use super::utils::BuildMode;
use std::collections::HashMap;
use testcontainers::core::{ContainerPort, WaitFor};
use testcontainers::{GenericBuildableImage, GenericImage, Container, ContainerAsync, ContainerRequest, ImageExt, runners::{AsyncRunner, SyncBuilder, SyncRunner}};

#[derive(Debug, Clone)]
pub struct WorkerService {
    dockerfile_path: String,
    context_path: String,
    env_vars: HashMap<String, String>,
    build_mode: BuildMode,
    worker_id: String,
}

impl WorkerService {
    /// Create a new worker service container
    pub fn new(database_url: String, build_mode: BuildMode, worker_id: String) -> Self {
        let dockerfile_path = match build_mode {
            BuildMode::Fast => "docker/build/rust-worker.test.Dockerfile".to_string(),
            BuildMode::CI => "docker/build/rust-worker.ci.Dockerfile".to_string(),
            BuildMode::Production => "docker/build/rust-worker.prod.Dockerfile".to_string(),
        };

        let mut env_vars = HashMap::new();
        env_vars.insert("DATABASE_URL".to_string(), database_url);
        env_vars.insert("TASKER_ENV".to_string(), "test".to_string());
        env_vars.insert("RUST_LOG".to_string(), "info".to_string());
        env_vars.insert("WORKER_ID".to_string(), worker_id.clone());
        env_vars.insert("WORKER_POOL_SIZE".to_string(), "2".to_string());
        env_vars.insert("PORT".to_string(), "8081".to_string());

        Self {
            dockerfile_path,
            context_path: ".".to_string(),
            env_vars,
            build_mode,
            worker_id,
        }
    }

    /// Create with custom configuration
    pub fn with_config(
        database_url: String,
        build_mode: BuildMode,
        worker_id: String,
        port: u16,
        pool_size: u32,
        rust_log_level: &str,
    ) -> Self {
        let mut service = Self::new(database_url, build_mode, worker_id);
        service.env_vars.insert("PORT".to_string(), port.to_string());
        service.env_vars.insert("WORKER_POOL_SIZE".to_string(), pool_size.to_string());
        service.env_vars.insert("RUST_LOG".to_string(), rust_log_level.to_string());
        service
    }

    /// Add environment variable
    pub fn with_env_var(mut self, key: &str, value: &str) -> Self {
        self.env_vars.insert(key.to_string(), value.to_string());
        self
    }

    /// Set orchestration service URL for the worker
    pub fn with_orchestration_url(mut self, orchestration_url: &str) -> Self {
        self.env_vars.insert("ORCHESTRATION_URL".to_string(), orchestration_url.to_string());
        self
    }

    /// Start the worker service container (synchronous)
    pub fn start(&self) -> anyhow::Result<Container<GenericImage>> {
        let tag = format!("tasker-worker-{:?}", self.build_mode).to_lowercase();
        let buildable_image = GenericBuildableImage::new("tasker-worker", &tag)
            .with_dockerfile(&self.dockerfile_path);

        // Build the image first using SyncBuilder trait
        let image = buildable_image.build_image()?;

        // Start with basic container configuration - convert to ContainerRequest
        let base_request: ContainerRequest<GenericImage> = image
            .with_exposed_port(ContainerPort::Tcp(8081))
            .with_wait_for(WaitFor::seconds(10))
            .into();

        // Apply all environment variables via fold
        let final_request = self.env_vars.iter().fold(
            base_request,
            |req: ContainerRequest<GenericImage>, (key, value)| req.with_env_var(key, value),
        );

        Ok(SyncRunner::start(final_request)?)
    }

    /// Start the worker service container (asynchronous)
    pub async fn start_async(&self) -> anyhow::Result<ContainerAsync<GenericImage>> {
        let tag = format!("tasker-worker-{:?}", self.build_mode).to_lowercase();
        let buildable_image = GenericBuildableImage::new("tasker-worker", &tag)
            .with_dockerfile(&self.dockerfile_path);

        // Build the image in a blocking thread to avoid runtime conflict
        let image = tokio::task::spawn_blocking(move || {
            buildable_image.build_image()
        }).await??;

        // Start with basic container configuration - convert to ContainerRequest
        let base_request: ContainerRequest<GenericImage> = image
            .with_exposed_port(ContainerPort::Tcp(8081))
            .with_wait_for(WaitFor::seconds(10))
            .into();

        // Apply all environment variables via fold
        let final_request = self.env_vars.iter().fold(
            base_request,
            |req: ContainerRequest<GenericImage>, (key, value)| req.with_env_var(key, value),
        );

        Ok(AsyncRunner::start(final_request).await?)
    }

    /// Get the service URL for this container
    pub fn service_url(&self, container: &Container<GenericImage>) -> String {
        let host_port = container
            .get_host_port_ipv4(8081)
            .expect("Failed to get worker service host port");
        
        format!("http://localhost:{}", host_port)
    }

    /// Get the service URL for this async container
    pub async fn service_url_async(&self, container: &ContainerAsync<GenericImage>) -> String {
        let host_port = container
            .get_host_port_ipv4(8081)
            .await.expect("Failed to get worker service host port");
        
        format!("http://localhost:{}", host_port)
    }

    /// Get the worker ID for this service
    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }

    /// Get the build mode for this service
    pub fn build_mode(&self) -> &BuildMode {
        &self.build_mode
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_worker_service_config() {
        let service = WorkerService::new(
            "postgresql://test:test@localhost:5432/test_db".to_string(),
            BuildMode::Fast,
            "test-worker-001".to_string(),
        );

        assert_eq!(service.dockerfile_path, "docker/build/rust-worker.test.Dockerfile");
        assert_eq!(service.env_vars.get("TASKER_ENV"), Some(&"test".to_string()));
        assert_eq!(service.env_vars.get("WORKER_ID"), Some(&"test-worker-001".to_string()));
        assert_eq!(service.worker_id(), "test-worker-001");
    }

    #[test]
    fn test_worker_service_with_config() {
        let service = WorkerService::with_config(
            "postgresql://test:test@localhost:5432/test_db".to_string(),
            BuildMode::CI,
            "ci-worker-001".to_string(),
            8082,
            4,
            "debug",
        );

        assert_eq!(service.dockerfile_path, "docker/build/rust-worker.ci.Dockerfile");
        assert_eq!(service.env_vars.get("PORT"), Some(&"8082".to_string()));
        assert_eq!(service.env_vars.get("WORKER_POOL_SIZE"), Some(&"4".to_string()));
        assert_eq!(service.env_vars.get("RUST_LOG"), Some(&"debug".to_string()));
    }

    #[test]
    fn test_worker_with_orchestration_url() {
        let service = WorkerService::new(
            "postgresql://test:test@localhost:5432/test_db".to_string(),
            BuildMode::Fast,
            "test-worker-001".to_string(),
        ).with_orchestration_url("http://localhost:8080");

        assert_eq!(service.env_vars.get("ORCHESTRATION_URL"), Some(&"http://localhost:8080".to_string()));
    }

    #[test]
    #[ignore] // Only run when Docker is available
    fn test_worker_service_starts() -> Result<(), Box<dyn std::error::Error>> {
        let service = WorkerService::new(
            "postgresql://tasker:tasker@localhost:5432/tasker_rust_test".to_string(),
            BuildMode::Fast,
            "test-worker-001".to_string(),
        );
        
        let container = service.start()?;
        let service_url = service.service_url(&container);
        
        // Basic URL format check
        assert!(service_url.starts_with("http://localhost:"));
        
        Ok(())
    }
}