//! # Orchestration Service Container
//!
//! Orchestration service container using ImageFromDockerfile with centralized docker/build structure

use super::utils::BuildMode;
use std::collections::HashMap;
use testcontainers::core::{ContainerPort, WaitFor};
use testcontainers::{GenericBuildableImage, GenericImage, Container, ContainerAsync, ContainerRequest, ImageExt, runners::{AsyncRunner, SyncBuilder, SyncRunner}};

#[derive(Debug, Clone)]
pub struct OrchestrationService {
    dockerfile_path: String,
    context_path: String,
    env_vars: HashMap<String, String>,
    build_mode: BuildMode,
}

impl OrchestrationService {
    /// Create a new orchestration service container
    pub fn new(database_url: String, build_mode: BuildMode) -> Self {
        let dockerfile_path = match build_mode {
            BuildMode::Fast => "docker/build/orchestration.test.Dockerfile".to_string(),
            BuildMode::CI => "docker/build/orchestration.ci.Dockerfile".to_string(),
            BuildMode::Production => "docker/build/orchestration.prod.Dockerfile".to_string(),
        };

        let mut env_vars = HashMap::new();
        env_vars.insert("DATABASE_URL".to_string(), database_url);
        env_vars.insert("TASKER_ENV".to_string(), "test".to_string());
        env_vars.insert("RUST_LOG".to_string(), "info".to_string());
        env_vars.insert("PORT".to_string(), "8080".to_string());

        Self {
            dockerfile_path,
            context_path: ".".to_string(),
            env_vars,
            build_mode,
        }
    }

    /// Create with custom configuration
    pub fn with_config(
        database_url: String,
        build_mode: BuildMode,
        port: u16,
        rust_log_level: &str,
    ) -> Self {
        let mut service = Self::new(database_url, build_mode);
        service.env_vars.insert("PORT".to_string(), port.to_string());
        service.env_vars.insert("RUST_LOG".to_string(), rust_log_level.to_string());
        service
    }

    /// Add environment variable
    pub fn with_env_var(mut self, key: &str, value: &str) -> Self {
        self.env_vars.insert(key.to_string(), value.to_string());
        self
    }

    /// Start the orchestration service container (synchronous)  
    pub fn start(&self) -> anyhow::Result<Container<GenericImage>> {
        let tag = format!("tasker-orchestration-{:?}", self.build_mode).to_lowercase();
        let buildable_image = GenericBuildableImage::new("tasker-orchestration", &tag)
            .with_dockerfile(&self.dockerfile_path);

        // Build the image first using SyncBuilder trait
        let image = buildable_image.build_image()?;

        // Start with basic container configuration - convert to ContainerRequest
        let base_request: ContainerRequest<GenericImage> = image
            .with_exposed_port(ContainerPort::Tcp(8080))
            .with_wait_for(WaitFor::seconds(10))
            .into();

        // Apply all environment variables via fold
        let final_request = self.env_vars.iter().fold(
            base_request,
            |req: ContainerRequest<GenericImage>, (key, value)| req.with_env_var(key, value),
        );

        Ok(SyncRunner::start(final_request)?)
    }

    /// Start the orchestration service container (asynchronous)
    pub async fn start_async(&self) -> anyhow::Result<ContainerAsync<GenericImage>> {
        let tag = format!("tasker-orchestration-{:?}", self.build_mode).to_lowercase();
        let buildable_image = GenericBuildableImage::new("tasker-orchestration", &tag)
            .with_dockerfile(&self.dockerfile_path);

        // Build the image in a blocking thread to avoid runtime conflict
        let image = tokio::task::spawn_blocking(move || {
            buildable_image.build_image()
        }).await??;

        // Start with basic container configuration - convert to ContainerRequest
        let base_request: ContainerRequest<GenericImage> = image
            .with_exposed_port(ContainerPort::Tcp(8080))
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
            .get_host_port_ipv4(8080)
            .expect("Failed to get orchestration service host port");
        
        format!("http://localhost:{}", host_port)
    }

    /// Get the service URL for this async container
    pub async fn service_url_async(&self, container: &ContainerAsync<GenericImage>) -> String {
        let host_port = container
            .get_host_port_ipv4(8080)
            .await.expect("Failed to get orchestration service host port");
        
        format!("http://localhost:{}", host_port)
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
    fn test_orchestration_service_config() {
        let service = OrchestrationService::new(
            "postgresql://test:test@localhost:5432/test_db".to_string(),
            BuildMode::Fast,
        );

        assert_eq!(service.dockerfile_path, "docker/build/orchestration.test.Dockerfile");
        assert_eq!(service.env_vars.get("TASKER_ENV"), Some(&"test".to_string()));
        assert_eq!(service.env_vars.get("PORT"), Some(&"8080".to_string()));
    }

    #[test]
    fn test_orchestration_service_with_config() {
        let service = OrchestrationService::with_config(
            "postgresql://test:test@localhost:5432/test_db".to_string(),
            BuildMode::CI,
            8081,
            "debug",
        );

        assert_eq!(service.dockerfile_path, "docker/build/orchestration.ci.Dockerfile");
        assert_eq!(service.env_vars.get("PORT"), Some(&"8081".to_string()));
        assert_eq!(service.env_vars.get("RUST_LOG"), Some(&"debug".to_string()));
    }

    #[test]
    #[ignore] // Only run when Docker is available
    fn test_orchestration_service_starts() -> Result<(), Box<dyn std::error::Error>> {
        let service = OrchestrationService::new(
            "postgresql://tasker:tasker@localhost:5432/tasker_rust_test".to_string(),
            BuildMode::Fast,
        );
        
        let container = service.start()?;
        let service_url = service.service_url(&container);
        
        // Basic URL format check
        assert!(service_url.starts_with("http://localhost:"));
        
        Ok(())
    }
}