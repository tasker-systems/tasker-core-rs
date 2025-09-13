//! # PostgreSQL Container with PGMQ Extension
//!
//! PostgreSQL container with PGMQ and UUIDv7 extensions using ImageFromDockerfile

use std::collections::HashMap;
use testcontainers::core::{ContainerPort, WaitFor};
use testcontainers::{GenericBuildableImage, GenericImage, Container, ContainerAsync, ContainerRequest, ImageExt, runners::{AsyncRunner, SyncBuilder, SyncRunner}};

#[derive(Debug, Clone)]
pub struct PgmqPostgres {
    dockerfile_path: String,
    context_path: String,
    env_vars: HashMap<String, String>,
}

impl Default for PgmqPostgres {
    fn default() -> Self {
        let mut env_vars = HashMap::new();
        env_vars.insert("POSTGRES_DB".to_string(), "tasker_rust_test".to_string());
        env_vars.insert("POSTGRES_USER".to_string(), "tasker".to_string());
        env_vars.insert("POSTGRES_PASSWORD".to_string(), "tasker".to_string());
        env_vars.insert("POSTGRES_HOST_AUTH_METHOD".to_string(), "trust".to_string());

        Self {
            dockerfile_path: "docker/db/Dockerfile".to_string(),
            context_path: ".".to_string(),
            env_vars,
        }
    }
}

impl PgmqPostgres {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_db_name(mut self, db_name: &str) -> Self {
        self.env_vars.insert("POSTGRES_DB".to_string(), db_name.to_string());
        self
    }

    pub fn with_user(mut self, user: &str) -> Self {
        self.env_vars.insert("POSTGRES_USER".to_string(), user.to_string());
        self
    }

    pub fn with_password(mut self, password: &str) -> Self {
        self.env_vars.insert("POSTGRES_PASSWORD".to_string(), password.to_string());
        self
    }

    /// Start the PostgreSQL container with PGMQ (synchronous)
    pub fn start(&self) -> anyhow::Result<Container<GenericImage>> {
        let buildable_image = GenericBuildableImage::new("tasker-pgmq", "test")
            .with_dockerfile(&self.dockerfile_path);

        // Build the image first using SyncBuilder trait
        let image = buildable_image.build_image()?;

        // Start with basic container configuration - convert to ContainerRequest  
        let base_request: ContainerRequest<GenericImage> = image
            .with_exposed_port(ContainerPort::Tcp(5432))
            .with_wait_for(WaitFor::message_on_stderr("database system is ready to accept connections"))
            .into();

        // Apply all environment variables via fold
        let final_request = self.env_vars.iter().fold(
            base_request,
            |req: ContainerRequest<GenericImage>, (key, value)| req.with_env_var(key, value),
        );

        Ok(SyncRunner::start(final_request)?)
    }

    /// Start the PostgreSQL container with PGMQ (asynchronous)
    pub async fn start_async(&self) -> anyhow::Result<ContainerAsync<GenericImage>> {
        let buildable_image = GenericBuildableImage::new("tasker-pgmq", "test")
            .with_dockerfile(&self.dockerfile_path);

        // Build the image in a blocking thread to avoid runtime conflict
        let image = tokio::task::spawn_blocking(move || {
            buildable_image.build_image()
        }).await??;

        // Start with basic container configuration - convert to ContainerRequest  
        let base_request: ContainerRequest<GenericImage> = image
            .with_exposed_port(ContainerPort::Tcp(5432))
            .with_wait_for(WaitFor::message_on_stderr("database system is ready to accept connections"))
            .into();

        // Apply all environment variables via fold
        let final_request = self.env_vars.iter().fold(
            base_request,
            |req: ContainerRequest<GenericImage>, (key, value)| req.with_env_var(key, value),
        );

        Ok(AsyncRunner::start(final_request).await?)
    }

    /// Get connection string for a running container
    pub fn connection_string(&self, container: &Container<GenericImage>) -> String {
        let host_port = container
            .get_host_port_ipv4(5432)
            .expect("Failed to get PostgreSQL host port");
            
        let default_db = "tasker_rust_test".to_string();
        let default_user = "tasker".to_string();
        let default_password = "tasker".to_string();
        
        let db_name = self.env_vars.get("POSTGRES_DB").unwrap_or(&default_db);
        let user = self.env_vars.get("POSTGRES_USER").unwrap_or(&default_user);
        let password = self.env_vars.get("POSTGRES_PASSWORD").unwrap_or(&default_password);

        format!("postgresql://{}:{}@localhost:{}/{}", user, password, host_port, db_name)
    }

    /// Get connection string for a running async container
    pub async fn connection_string_async(&self, container: &ContainerAsync<GenericImage>) -> String {
        let host_port = container
            .get_host_port_ipv4(5432)
            .await.expect("Failed to get PostgreSQL host port");
            
        let default_db = "tasker_rust_test".to_string();
        let default_user = "tasker".to_string();
        let default_password = "tasker".to_string();
        
        let db_name = self.env_vars.get("POSTGRES_DB").unwrap_or(&default_db);
        let user = self.env_vars.get("POSTGRES_USER").unwrap_or(&default_user);
        let password = self.env_vars.get("POSTGRES_PASSWORD").unwrap_or(&default_password);

        format!("postgresql://{}:{}@localhost:{}/{}", user, password, host_port, db_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pgmq_postgres_container_config() {
        let postgres = PgmqPostgres::new()
            .with_db_name("test_db")
            .with_user("test_user")
            .with_password("test_pass");

        assert_eq!(postgres.env_vars.get("POSTGRES_DB"), Some(&"test_db".to_string()));
        assert_eq!(postgres.env_vars.get("POSTGRES_USER"), Some(&"test_user".to_string()));
        assert_eq!(postgres.env_vars.get("POSTGRES_PASSWORD"), Some(&"test_pass".to_string()));
        assert_eq!(postgres.dockerfile_path, "docker/db/Dockerfile");
    }

    #[test]
    #[ignore] // Only run when Docker is available
    fn test_postgres_container_starts() -> Result<(), Box<dyn std::error::Error>> {
        let postgres = PgmqPostgres::new();
        let container = postgres.start()?;
        let connection_string = postgres.connection_string(&container);

        // Basic connection string format check
        assert!(connection_string.starts_with("postgresql://tasker:tasker@localhost:"));
        assert!(connection_string.ends_with("/tasker_rust_test"));
        
        Ok(())
    }
}