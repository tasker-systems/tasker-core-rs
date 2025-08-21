//! # Relationship Factories
//!
//! Factories for creating relationships and edges between workflow entities.

use super::base::*;
use crate::models::core::workflow_step_edge::NewWorkflowStepEdge;
use crate::models::WorkflowStepEdge;
use async_trait::async_trait;
use sqlx::{types::Uuid, PgPool};

/// Factory for creating workflow step edges (dependencies between steps)
#[derive(Debug, Clone)]
pub struct WorkflowStepEdgeFactory {
    from_step_uuid: Option<Uuid>,
    to_step_uuid: Option<Uuid>,
    name: String,
}

impl Default for WorkflowStepEdgeFactory {
    fn default() -> Self {
        Self {
            from_step_uuid: None,
            to_step_uuid: None,
            name: "provides".to_string(),
        }
    }
}

impl WorkflowStepEdgeFactory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_from_step(mut self, step_uuid: Uuid) -> Self {
        self.from_step_uuid = Some(step_uuid);
        self
    }

    pub fn with_to_step(mut self, step_uuid: Uuid) -> Self {
        self.to_step_uuid = Some(step_uuid);
        self
    }

    pub fn with_name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    /// Create a "provides" edge (default)
    pub fn provides(self) -> Self {
        self.with_name("provides")
    }
}

#[async_trait]
impl SqlxFactory<WorkflowStepEdge> for WorkflowStepEdgeFactory {
    async fn create(&self, pool: &PgPool) -> FactoryResult<WorkflowStepEdge> {
        let from_step_uuid = self
            .from_step_uuid
            .ok_or_else(|| FactoryError::InvalidConfig {
                details: "from_step_uuid is required".to_string(),
            })?;

        let to_step_uuid = self
            .to_step_uuid
            .ok_or_else(|| FactoryError::InvalidConfig {
                details: "to_step_uuid is required".to_string(),
            })?;

        let new_edge = NewWorkflowStepEdge {
            from_step_uuid,
            to_step_uuid,
            name: self.name.clone(),
        };

        let edge = WorkflowStepEdge::create(pool, new_edge).await?;
        Ok(edge)
    }

    async fn find_or_create(&self, pool: &PgPool) -> FactoryResult<WorkflowStepEdge> {
        let from_step_uuid = self
            .from_step_uuid
            .ok_or_else(|| FactoryError::InvalidConfig {
                details: "from_step_uuid is required".to_string(),
            })?;

        let to_step_uuid = self
            .to_step_uuid
            .ok_or_else(|| FactoryError::InvalidConfig {
                details: "to_step_uuid is required".to_string(),
            })?;

        // Try to find existing edge
        if let Some(existing) =
            WorkflowStepEdge::find_by_steps_and_name(pool, from_step_uuid, to_step_uuid, &self.name)
                .await?
        {
            return Ok(existing);
        }

        // Create if not found
        self.create(pool).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::factories::{TaskFactory, WorkflowStepFactory};

    #[sqlx::test(migrator = "crate::test_utils::MIGRATOR")]
    async fn test_create_workflow_step_edge(pool: PgPool) -> FactoryResult<()> {
        // Create task and steps
        let task = TaskFactory::new().create(&pool).await?;
        let step1 = WorkflowStepFactory::new()
            .for_task(task.task_uuid)
            .create(&pool)
            .await?;
        let step2 = WorkflowStepFactory::new()
            .for_task(task.task_uuid)
            .create(&pool)
            .await?;

        // Create edge
        let edge = WorkflowStepEdgeFactory::new()
            .with_from_step(step1.workflow_step_uuid)
            .with_to_step(step2.workflow_step_uuid)
            .provides()
            .create(&pool)
            .await?;

        assert_eq!(edge.from_step_uuid, step1.workflow_step_uuid);
        assert_eq!(edge.to_step_uuid, step2.workflow_step_uuid);
        assert_eq!(edge.name, "provides");

        Ok(())
    }
}
