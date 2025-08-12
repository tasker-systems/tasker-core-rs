//! # Foundation Factories
//!
//! Factories for creating foundational objects that other entities depend on.
//! These factories implement the "find-or-create" pattern to prevent conflicts
//! and ensure consistent test data across test runs.

use super::base::*;
use async_trait::async_trait;
use serde_json::{json, Value};
use sqlx::PgPool;
use tasker_core::models::core::{
    dependent_system::NewDependentSystem, named_step::NewNamedStep, named_task::NewNamedTask,
    task_namespace::NewTaskNamespace,
};
use tasker_core::models::{DependentSystem, NamedStep, NamedTask, TaskNamespace};

/// Factory for creating task namespaces
#[derive(Debug, Clone)]
pub struct TaskNamespaceFactory {
    name: String,
    description: Option<String>,
}

impl Default for TaskNamespaceFactory {
    fn default() -> Self {
        Self {
            name: "default".to_string(),
            description: Some("Default namespace for testing".to_string()),
        }
    }
}

impl TaskNamespaceFactory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    pub fn with_description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }

    /// Create common namespaces used in Rails factories
    pub async fn create_common_namespaces(pool: &PgPool) -> FactoryResult<Vec<TaskNamespace>> {
        let namespaces = vec![
            ("default", "Default namespace for core tasks"),
            ("payments", "Payment processing workflows"),
            ("notifications", "User notification workflows"),
            ("integrations", "Third-party API integrations"),
            ("data_processing", "Data transformation workflows"),
        ];

        let mut results = Vec::new();
        for (name, desc) in namespaces {
            let namespace = Self::new()
                .with_name(name)
                .with_description(desc)
                .find_or_create(pool)
                .await?;
            results.push(namespace);
        }

        Ok(results)
    }
}

#[async_trait]
impl SqlxFactory<TaskNamespace> for TaskNamespaceFactory {
    async fn create(&self, pool: &PgPool) -> FactoryResult<TaskNamespace> {
        let new_namespace = NewTaskNamespace {
            name: self.name.clone(),
            description: self.description.clone(),
        };

        let namespace = TaskNamespace::create(pool, new_namespace).await?;
        Ok(namespace)
    }

    async fn find_or_create(&self, pool: &PgPool) -> FactoryResult<TaskNamespace> {
        // Try to find existing namespace first
        if let Some(existing) = TaskNamespace::find_by_name(pool, &self.name).await? {
            return Ok(existing);
        }

        // Create new if not found
        self.create(pool).await
    }
}

/// Factory for creating dependent systems
#[derive(Debug, Clone)]
pub struct DependentSystemFactory {
    name: String,
    description: Option<String>,
}

impl Default for DependentSystemFactory {
    fn default() -> Self {
        Self {
            name: "api".to_string(),
            description: Some("HTTP API system for testing".to_string()),
        }
    }
}

impl DependentSystemFactory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    pub fn with_description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }

    /// Create common systems used in Rails factories
    pub async fn create_common_systems(pool: &PgPool) -> FactoryResult<Vec<DependentSystem>> {
        let systems = vec![
            ("api", "HTTP API system for external integrations"),
            ("database", "PostgreSQL database system"),
            ("notification", "Email notification service"),
            ("dummy-system", "Test mock system that always succeeds"),
        ];

        let mut results = Vec::new();
        for (name, description) in systems {
            let system = Self::new()
                .with_name(name)
                .with_description(description)
                .find_or_create(pool)
                .await?;
            results.push(system);
        }

        Ok(results)
    }
}

#[async_trait]
impl SqlxFactory<DependentSystem> for DependentSystemFactory {
    async fn create(&self, pool: &PgPool) -> FactoryResult<DependentSystem> {
        let new_system = NewDependentSystem {
            name: self.name.clone(),
            description: self.description.clone(),
        };

        let system = DependentSystem::create(pool, new_system).await?;
        Ok(system)
    }

    async fn find_or_create(&self, pool: &PgPool) -> FactoryResult<DependentSystem> {
        // Try to find existing system first
        if let Some(existing) = DependentSystem::find_by_name(pool, &self.name).await? {
            return Ok(existing);
        }

        // Create new if not found
        self.create(pool).await
    }
}

/// Factory for creating named tasks (task templates)
#[derive(Debug, Clone)]
pub struct NamedTaskFactory {
    name: String,
    namespace_name: String,
    version: String,
    description: Option<String>,
    configuration: Option<Value>,
}

impl Default for NamedTaskFactory {
    fn default() -> Self {
        Self {
            name: "dummy_task".to_string(),
            namespace_name: "default".to_string(),
            version: "0.1.0".to_string(),
            description: Some("Test task created by factory".to_string()),
            configuration: Some(json!({
                "test_mode": true,
                "auto_generated": true
            })),
        }
    }
}

impl NamedTaskFactory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    pub fn with_namespace(mut self, namespace: &str) -> Self {
        self.namespace_name = namespace.to_string();
        self
    }

    pub fn with_version(mut self, version: &str) -> Self {
        self.version = version.to_string();
        self
    }
}

#[async_trait]
impl SqlxFactory<NamedTask> for NamedTaskFactory {
    async fn create(&self, pool: &PgPool) -> FactoryResult<NamedTask> {
        // Ensure namespace exists
        let namespace = TaskNamespaceFactory::new()
            .with_name(&self.namespace_name)
            .find_or_create(pool)
            .await?;

        let config = self.configuration.clone().unwrap_or_else(|| json!({}));
        utils::validate_jsonb(&config)?;

        let new_task = NewNamedTask {
            name: self.name.clone(),
            task_namespace_uuid: namespace.task_namespace_uuid,
            version: Some(self.version.clone()),
            description: self.description.clone(),
            configuration: Some(config),
        };

        let task = NamedTask::create(pool, new_task).await?;
        Ok(task)
    }

    async fn find_or_create(&self, pool: &PgPool) -> FactoryResult<NamedTask> {
        // Get namespace first to get the ID
        let namespace = TaskNamespaceFactory::new()
            .with_name(&self.namespace_name)
            .find_or_create(pool)
            .await?;

        // Try to find existing task by name, version, and namespace
        if let Some(existing) = NamedTask::find_by_name_version_namespace(
            pool,
            &self.name,
            &self.version,
            namespace.task_namespace_uuid,
        )
        .await?
        {
            return Ok(existing);
        }

        // Create new if not found
        self.create(pool).await
    }
}

/// Factory for creating named steps (step templates)
#[derive(Debug, Clone)]
pub struct NamedStepFactory {
    name: String,
    step_type: String,
    dependent_system_name: String,
    configuration: Option<Value>,
}

impl Default for NamedStepFactory {
    fn default() -> Self {
        Self {
            name: "dummy_step".to_string(),
            step_type: "generic".to_string(),
            dependent_system_name: "dummy-system".to_string(),
            configuration: Some(json!({
                "test_mode": true,
                "always_succeed": true
            })),
        }
    }
}

impl NamedStepFactory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    // NOTE: Clippy false positive - this method is used across multiple test files
    // but clippy's dead code detection doesn't always catch cross-file test usage
    #[allow(dead_code)]
    pub fn with_system(mut self, system_name: &str) -> Self {
        self.dependent_system_name = system_name.to_string();
        self
    }
}

#[async_trait]
impl SqlxFactory<NamedStep> for NamedStepFactory {
    async fn create(&self, pool: &PgPool) -> FactoryResult<NamedStep> {
        // Ensure dependent system exists
        let system = DependentSystemFactory::new()
            .with_name(&self.dependent_system_name)
            .find_or_create(pool)
            .await?;
        let dependent_system_uuid = system.dependent_system_uuid;

        let config = self.configuration.clone().unwrap_or_else(|| json!({}));
        utils::validate_jsonb(&config)?;

        let new_step = NewNamedStep {
            name: self.name.clone(),
            dependent_system_uuid,
            description: Some(format!("{} step", self.step_type)),
        };

        let step = NamedStep::create(pool, new_step).await?;
        Ok(step)
    }

    async fn find_or_create(&self, pool: &PgPool) -> FactoryResult<NamedStep> {
        // Try to find existing step by name (returns Vec, so take first)
        let existing_steps = NamedStep::find_by_name(pool, &self.name).await?;
        if let Some(existing) = existing_steps.into_iter().next() {
            return Ok(existing);
        }

        // Create new if not found
        self.create(pool).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    #[sqlx::test]
    async fn test_namespace_factory(pool: PgPool) -> FactoryResult<()> {
        let namespace = TaskNamespaceFactory::new()
            .with_name("test_namespace")
            .with_description("Test namespace")
            .create(&pool)
            .await?;

        assert_eq!(namespace.name, "test_namespace");
        assert_eq!(namespace.description, Some("Test namespace".to_string()));

        Ok(())
    }

    #[sqlx::test]
    async fn test_system_factory(pool: PgPool) -> FactoryResult<()> {
        let system = DependentSystemFactory::new()
            .with_name("test_api")
            .with_description("Test API system")
            .create(&pool)
            .await?;

        assert_eq!(system.name, "test_api");
        assert_eq!(system.description, Some("Test API system".to_string()));

        Ok(())
    }

    #[sqlx::test]
    async fn test_common_foundations_creation(pool: PgPool) -> FactoryResult<()> {
        let namespaces = TaskNamespaceFactory::create_common_namespaces(&pool).await?;
        let systems = DependentSystemFactory::create_common_systems(&pool).await?;

        assert_eq!(namespaces.len(), 5);
        assert_eq!(systems.len(), 4);

        // Test find_or_create pattern
        let default_ns = TaskNamespaceFactory::new()
            .with_name("default")
            .find_or_create(&pool)
            .await?;

        // Should be the same as the one from common creation
        assert_eq!(
            default_ns.task_namespace_uuid,
            namespaces[0].task_namespace_uuid
        );

        Ok(())
    }
}
