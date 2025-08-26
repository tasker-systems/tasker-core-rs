//! # Shared Test Utilities
//!
//! Common utilities for workspace-level integration tests.
//! These utilities help coordinate testing across multiple packages.

use std::sync::Arc;
use tasker_shared::{
    system_context::SystemContext,
    models::core::{task_namespace::TaskNamespace, named_task::NamedTask, named_step::NamedStep}
};
use uuid::Uuid;

/// Cross-package test foundation that combines orchestration and worker components
pub struct IntegrationTestFoundation {
    pub system_context: Arc<SystemContext>,
    pub namespace: TaskNamespace,
    pub named_task: NamedTask,
    pub named_step: NamedStep,
    pub test_id: String,
}

impl IntegrationTestFoundation {
    /// Create a complete test foundation for cross-package testing
    pub async fn new(
        pool: &sqlx::PgPool,
        test_id: &str
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Create system context
        let system_context = Arc::new(SystemContext::with_pool(pool.clone()).await?);
        
        // Create namespace
        let namespace_name = format!("integration_test_{}", test_id);
        let namespace = TaskNamespace::find_or_create(pool, &namespace_name).await?;
        
        // Create named task
        let task_name = format!("test_task_{}", test_id);
        let named_task = NamedTask::find_or_create_by_name_version_namespace(
            pool,
            &task_name,
            "1.0.0",
            namespace.task_namespace_uuid,
        ).await?;
        
        // Create named step
        let step_name = format!("test_step_{}", test_id);
        let named_step = NamedStep::find_or_create_by_name_simple(pool, &step_name).await?;
        
        // Initialize namespace queue
        system_context.initialize_queues(&[&namespace_name]).await?;
        
        Ok(Self {
            system_context,
            namespace,
            named_task,
            named_step,
            test_id: test_id.to_string(),
        })
    }
    
    /// Get the namespace queue name
    pub fn queue_name(&self) -> String {
        format!("{}_queue", self.namespace.name)
    }
}

/// Utilities for cross-package test coordination
pub struct IntegrationTestUtils;

impl IntegrationTestUtils {
    /// Wait for condition with timeout (useful for async coordination)
    pub async fn wait_for_condition<F, Fut>(
        mut condition: F,
        timeout_secs: u64,
        check_interval_ms: u64,
    ) -> Result<bool, tokio::time::error::Elapsed>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        let timeout_duration = std::time::Duration::from_secs(timeout_secs);
        let check_interval = std::time::Duration::from_millis(check_interval_ms);
        
        tokio::time::timeout(timeout_duration, async {
            loop {
                if condition().await {
                    return true;
                }
                tokio::time::sleep(check_interval).await;
            }
        }).await
    }
    
    /// Generate unique test ID based on current timestamp and random component
    pub fn generate_test_id(prefix: &str) -> String {
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis();
        let random = Uuid::new_v4().to_string()[..8].to_string();
        format!("{}_{}_{}", prefix, timestamp, random)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_generate_unique_test_id() {
        let id1 = IntegrationTestUtils::generate_test_id("test");
        let id2 = IntegrationTestUtils::generate_test_id("test");
        
        assert_ne!(id1, id2);
        assert!(id1.starts_with("test_"));
        assert!(id2.starts_with("test_"));
    }
}