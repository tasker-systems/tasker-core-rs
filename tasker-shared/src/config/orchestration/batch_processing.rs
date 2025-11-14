// Re-export BatchProcessingConfig from tasker.rs (TAS-61 pattern)
pub use crate::config::tasker::BatchProcessingConfig;

impl BatchProcessingConfig {
    /// Get the batch size to use, with fallback to default
    pub fn effective_batch_size(&self, template_batch_size: Option<u32>) -> u32 {
        template_batch_size.unwrap_or(self.default_batch_size)
    }

    /// Get the checkpoint interval to use, with fallback to default
    pub fn effective_checkpoint_interval(&self, template_interval: Option<u32>) -> u32 {
        template_interval.unwrap_or(self.checkpoint_interval_default)
    }

    /// Check if batch processing is enabled
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get the checkpoint stall duration in seconds
    pub fn checkpoint_stall_seconds(&self) -> u64 {
        (self.checkpoint_stall_minutes as u64) * 60
    }

    /// Validate that a task template is compatible with current batch processing configuration
    ///
    /// ## Usage at Task Acceptance
    ///
    /// This should be called when accepting tasks (POST /v1/tasks) to reject
    /// tasks with batchable steps when batch processing is disabled.
    ///
    /// ```rust,ignore
    /// use tasker_shared::config::BatchProcessingConfig;
    /// use tasker_shared::models::core::task_template::TaskTemplate;
    ///
    /// let config = context.tasker_config.orchestration.batch_processing;
    /// let template = /* resolved from TaskRequest */;
    ///
    /// if let Err(e) = config.validate_template_compatibility(&template) {
    ///     // Return HTTP 422 Unprocessable Entity
    ///     return Err(format!("Task rejected: {}", e));
    /// }
    /// ```
    ///
    /// ## Rationale
    ///
    /// Validation must happen at task acceptance, not during execution:
    /// - By execution time, the task has been accepted and enqueued
    /// - The batchable step has already executed
    /// - Silently skipping batch processing would be a silent failure
    /// - Users would have no visibility into why workers weren't created
    pub fn validate_template_compatibility(
        &self,
        template: &crate::models::core::task_template::TaskTemplate,
    ) -> Result<(), String> {
        use crate::models::core::task_template::StepType;

        // Check if template has any batchable steps
        let has_batchable_steps = template
            .steps
            .iter()
            .any(|step| step.step_type == StepType::Batchable);

        if has_batchable_steps && !self.is_enabled() {
            return Err(format!(
                "Task template '{}/{}:{}' contains batchable steps but batch processing is disabled. \
                Enable 'orchestration.batch_processing.enabled' in configuration or use a different task template.",
                template.namespace_name, template.name, template.version
            ));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = BatchProcessingConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_parallel_batches, 50);
        assert_eq!(config.default_batch_size, 1000);
        assert_eq!(config.checkpoint_interval_default, 100);
        assert_eq!(config.checkpoint_stall_minutes, 15);
    }

    #[test]
    fn test_effective_batch_size() {
        let config = BatchProcessingConfig::default();
        assert_eq!(config.effective_batch_size(None), 1000);
        assert_eq!(config.effective_batch_size(Some(500)), 500);
    }

    #[test]
    fn test_effective_checkpoint_interval() {
        let config = BatchProcessingConfig::default();
        assert_eq!(config.effective_checkpoint_interval(None), 100);
        assert_eq!(config.effective_checkpoint_interval(Some(50)), 50);
    }

    #[test]
    fn test_checkpoint_stall_seconds() {
        let config = BatchProcessingConfig::default();
        assert_eq!(config.checkpoint_stall_seconds(), 900); // 15 * 60
    }

    #[test]
    fn test_validate_template_compatibility_enabled() {
        use crate::models::core::task_template::{
            HandlerDefinition, StepDefinition, StepType, TaskTemplate,
        };

        let config = BatchProcessingConfig::default();
        assert!(config.is_enabled());

        // Template with batchable step should be valid when enabled
        let template = TaskTemplate::builder()
            .name("test_task".to_string())
            .namespace_name("test".to_string())
            .version("1.0.0".to_string())
            .steps(vec![StepDefinition {
                name: "batchable_step".to_string(),
                description: None,
                handler: HandlerDefinition::builder()
                    .callable("TestHandler".to_string())
                    .build(),
                step_type: StepType::Batchable,
                system_dependency: None,
                dependencies: vec![],
                retry: Default::default(),
                timeout_seconds: None,
                publishes_events: vec![],
                batch_config: None,
            }])
            .build();

        assert!(config.validate_template_compatibility(&template).is_ok());
    }

    #[test]
    fn test_validate_template_compatibility_disabled() {
        use crate::models::core::task_template::{
            HandlerDefinition, StepDefinition, StepType, TaskTemplate,
        };

        let config = BatchProcessingConfig {
            enabled: false,
            ..Default::default()
        };

        // Template with batchable step should be rejected when disabled
        let template = TaskTemplate::builder()
            .name("test_task".to_string())
            .namespace_name("test".to_string())
            .version("1.0.0".to_string())
            .steps(vec![StepDefinition {
                name: "batchable_step".to_string(),
                description: None,
                handler: HandlerDefinition::builder()
                    .callable("TestHandler".to_string())
                    .build(),
                step_type: StepType::Batchable,
                system_dependency: None,
                dependencies: vec![],
                retry: Default::default(),
                timeout_seconds: None,
                publishes_events: vec![],
                batch_config: None,
            }])
            .build();

        let result = config.validate_template_compatibility(&template);
        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.contains("batch processing is disabled"));
        assert!(error.contains("test/test_task:1.0.0"));
    }

    #[test]
    fn test_validate_template_compatibility_no_batchable_steps() {
        use crate::models::core::task_template::{
            HandlerDefinition, StepDefinition, StepType, TaskTemplate,
        };

        let mut config = BatchProcessingConfig::default();
        config.enabled = false;

        // Template without batchable steps should be valid even when disabled
        let template = TaskTemplate::builder()
            .name("test_task".to_string())
            .namespace_name("test".to_string())
            .version("1.0.0".to_string())
            .steps(vec![StepDefinition {
                name: "standard_step".to_string(),
                description: None,
                handler: HandlerDefinition::builder()
                    .callable("TestHandler".to_string())
                    .build(),
                step_type: StepType::Standard,
                system_dependency: None,
                dependencies: vec![],
                retry: Default::default(),
                timeout_seconds: None,
                publishes_events: vec![],
                batch_config: None,
            }])
            .build();

        assert!(config.validate_template_compatibility(&template).is_ok());
    }
}
