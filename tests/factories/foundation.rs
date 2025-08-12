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
    base: BaseFactory,
    name: String,
    description: Option<String>,
}

impl Default for TaskNamespaceFactory {
    fn default() -> Self {
        Self {
            base: BaseFactory::new(),
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
    base: BaseFactory,
    name: String,
    description: Option<String>,
}

impl Default for DependentSystemFactory {
    fn default() -> Self {
        Self {
            base: BaseFactory::new(),
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
    base: BaseFactory,
    name: String,
    namespace_name: String,
    version: String,
    description: Option<String>,
    configuration: Option<Value>,
    timeout_seconds: Option<i32>,
    retryable: bool,
}

impl Default for NamedTaskFactory {
    fn default() -> Self {
        Self {
            base: BaseFactory::new(),
            name: "dummy_task".to_string(),
            namespace_name: "default".to_string(),
            version: "0.1.0".to_string(),
            description: Some("Test task created by factory".to_string()),
            configuration: Some(json!({
                "test_mode": true,
                "auto_generated": true
            })),
            timeout_seconds: Some(300),
            retryable: true,
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

    pub fn api_integration(mut self) -> Self {
        self.name = "api_integration_task".to_string();
        self.description = Some("API integration workflow task".to_string());
        self.configuration = Some(json!({
            "api_endpoint": "/api/v1/integrate",
            "method": "POST",
            "timeout": 60,
            "retry_on_failure": true
        }));
        self
    }

    pub fn data_processing(mut self) -> Self {
        self.name = "data_processing_task".to_string();
        self.namespace_name = "data_processing".to_string();
        self.description = Some("Batch data processing task".to_string());
        self.configuration = Some(json!({
            "batch_size": 1000,
            "parallel_workers": 4,
            "output_format": "json"
        }));
        self
    }

    pub fn notification_workflow(mut self) -> Self {
        self.name = "notification_workflow".to_string();
        self.namespace_name = "notifications".to_string();
        self.description = Some("User notification workflow".to_string());
        self.configuration = Some(json!({
            "channels": ["email", "sms"],
            "priority": "normal",
            "delivery_window": "24h"
        }));
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
    base: BaseFactory,
    name: String,
    step_type: String,
    dependent_system_name: String,
    configuration: Option<Value>,
    retryable: bool,
    timeout_seconds: Option<i32>,
}

impl Default for NamedStepFactory {
    fn default() -> Self {
        Self {
            base: BaseFactory::new(),
            name: "dummy_step".to_string(),
            step_type: "generic".to_string(),
            dependent_system_name: "dummy-system".to_string(),
            configuration: Some(json!({
                "test_mode": true,
                "always_succeed": true
            })),
            retryable: true,
            timeout_seconds: Some(60),
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

    pub fn with_type(mut self, step_type: &str) -> Self {
        self.step_type = step_type.to_string();
        self
    }

    pub fn with_system(mut self, system_name: &str) -> Self {
        self.dependent_system_name = system_name.to_string();
        self
    }

    pub fn api_call(mut self) -> Self {
        self.name = "api_call_step".to_string();
        self.step_type = "http_request".to_string();
        self.dependent_system_name = "api".to_string();
        self.configuration = Some(json!({
            "method": "POST",
            "endpoint": "/api/process",
            "headers": {"Content-Type": "application/json"}
        }));
        self
    }

    pub fn database_operation(mut self) -> Self {
        self.name = "db_operation_step".to_string();
        self.step_type = "database_query".to_string();
        self.dependent_system_name = "database".to_string();
        self.configuration = Some(json!({
            "query_type": "update",
            "table": "test_table",
            "transaction": true
        }));
        self
    }

    pub fn notification_send(mut self) -> Self {
        self.name = "send_notification_step".to_string();
        self.step_type = "notification".to_string();
        self.dependent_system_name = "notification".to_string();
        self.configuration = Some(json!({
            "template": "welcome_email",
            "priority": "normal"
        }));
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
