use tasker_core::models::{
    task_namespace::{TaskNamespace, NewTaskNamespace},
    named_task::{NamedTask, NewNamedTask},
    named_step::{NamedStep, NewNamedStep},
    task::{Task, NewTask},
    workflow_step::{WorkflowStep, NewWorkflowStep},
    workflow_step_edge::{WorkflowStepEdge, NewWorkflowStepEdge},
};
use sqlx::PgPool;
use super::unique_name;

/// Builder pattern for creating test TaskNamespaces
pub struct TaskNamespaceBuilder {
    name: Option<String>,
    description: Option<String>,
}

impl TaskNamespaceBuilder {
    pub fn new() -> Self {
        Self {
            name: None,
            description: None,
        }
    }

    pub fn with_name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }

    pub fn with_description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }

    pub async fn build(self, pool: &PgPool) -> TaskNamespace {
        let new_namespace = NewTaskNamespace {
            name: self.name.unwrap_or_else(|| unique_name("namespace")),
            description: self.description,
        };
        TaskNamespace::create(pool, new_namespace).await
            .expect("Failed to create test TaskNamespace")
    }

    pub async fn build_in_tx(self, pool: &PgPool) -> TaskNamespace {
        let new_namespace = NewTaskNamespace {
            name: self.name.unwrap_or_else(|| unique_name("namespace")),
            description: self.description,
        };
        TaskNamespace::create(pool, new_namespace).await
            .expect("Failed to create test TaskNamespace")
    }
}

/// Builder pattern for creating test NamedSteps
pub struct NamedStepBuilder {
    name: Option<String>,
    version: Option<i32>,
    description: Option<String>,
    handler_class: Option<String>,
}

impl NamedStepBuilder {
    pub fn new() -> Self {
        Self {
            name: None,
            version: None,
            description: None,
            handler_class: None,
        }
    }

    pub fn with_name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }

    pub fn with_version(mut self, version: i32) -> Self {
        self.version = Some(version);
        self
    }

    pub fn with_description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }

    pub fn with_handler_class(mut self, handler_class: &str) -> Self {
        self.handler_class = Some(handler_class.to_string());
        self
    }

    pub async fn build(self, pool: &PgPool) -> NamedStep {
        let new_step = NewNamedStep {
            name: self.name.unwrap_or_else(|| unique_name("step")),
            version: self.version.or(Some(1)),
            description: self.description,
            handler_class: self.handler_class.unwrap_or_else(|| "TestStepHandler".to_string()),
        };
        NamedStep::create(pool, new_step).await
            .expect("Failed to create test NamedStep")
    }

    pub async fn build_in_tx(self, pool: &PgPool) -> NamedStep {
        let new_step = NewNamedStep {
            name: self.name.unwrap_or_else(|| unique_name("step")),
            version: self.version.or(Some(1)),
            description: self.description,
            handler_class: self.handler_class.unwrap_or_else(|| "TestStepHandler".to_string()),
        };
        NamedStep::create(pool, new_step).await
            .expect("Failed to create test NamedStep")
    }
}

/// Builder pattern for creating test NamedTasks
pub struct NamedTaskBuilder {
    name: Option<String>,
    version: Option<i32>,
    description: Option<String>,
    namespace: Option<TaskNamespace>,
}

impl NamedTaskBuilder {
    pub fn new() -> Self {
        Self {
            name: None,
            version: None,
            description: None,
            namespace: None,
        }
    }

    pub fn with_name(mut self, name: &str) -> Self {
        self.name = Some(name.to_string());
        self
    }

    pub fn with_version(mut self, version: i32) -> Self {
        self.version = Some(version);
        self
    }

    pub fn with_description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }

    pub fn with_namespace(mut self, namespace: TaskNamespace) -> Self {
        self.namespace = Some(namespace);
        self
    }

    pub async fn build(self, pool: &PgPool) -> NamedTask {
        let namespace = if let Some(ns) = self.namespace {
            ns
        } else {
            TaskNamespaceBuilder::new().build(pool).await
        };

        let new_task = NewNamedTask {
            name: self.name.unwrap_or_else(|| unique_name("task")),
            version: self.version.or(Some(1)),
            description: self.description,
            task_namespace_id: namespace.task_namespace_id,
        };
        NamedTask::create(pool, new_task).await
            .expect("Failed to create test NamedTask")
    }

    pub async fn build_in_tx(self, pool: &PgPool) -> NamedTask {
        let namespace = if let Some(ns) = self.namespace {
            ns
        } else {
            TaskNamespaceBuilder::new().build(pool).await
        };

        let new_task = NewNamedTask {
            name: self.name.unwrap_or_else(|| unique_name("task")),
            version: self.version.or(Some(1)),
            description: self.description,
            task_namespace_id: namespace.task_namespace_id,
        };
        NamedTask::create(pool, new_task).await
            .expect("Failed to create test NamedTask")
    }
}

/// Builder pattern for creating test Tasks
pub struct TaskBuilder {
    context: Option<serde_json::Value>,
    named_task: Option<NamedTask>,
}

impl TaskBuilder {
    pub fn new() -> Self {
        Self {
            context: None,
            named_task: None,
        }
    }

    pub fn with_context(mut self, context: serde_json::Value) -> Self {
        self.context = Some(context);
        self
    }

    pub fn with_named_task(mut self, named_task: NamedTask) -> Self {
        self.named_task = Some(named_task);
        self
    }

    pub async fn build(self, pool: &PgPool) -> Task {
        let named_task = if let Some(nt) = self.named_task {
            nt
        } else {
            NamedTaskBuilder::new().build(pool).await
        };

        let new_task = NewTask {
            context: self.context.unwrap_or_else(|| serde_json::json!({})),
            named_task_id: named_task.named_task_id,
        };
        Task::create(pool, new_task).await
            .expect("Failed to create test Task")
    }

    pub async fn build_in_tx(self, pool: &PgPool) -> Task {
        let named_task = if let Some(nt) = self.named_task {
            nt
        } else {
            NamedTaskBuilder::new().build(pool).await
        };

        let new_task = NewTask {
            context: self.context.unwrap_or_else(|| serde_json::json!({})),
            named_task_id: named_task.named_task_id,
        };
        Task::create(pool, new_task).await
            .expect("Failed to create test Task")
    }
}

/// Builder pattern for creating test WorkflowSteps
pub struct WorkflowStepBuilder {
    context: Option<serde_json::Value>,
    max_retries: Option<i32>,
    task: Option<Task>,
    named_step: Option<NamedStep>,
}

impl WorkflowStepBuilder {
    pub fn new() -> Self {
        Self {
            context: None,
            max_retries: None,
            task: None,
            named_step: None,
        }
    }

    pub fn with_context(mut self, context: serde_json::Value) -> Self {
        self.context = Some(context);
        self
    }

    pub fn with_max_retries(mut self, max_retries: i32) -> Self {
        self.max_retries = Some(max_retries);
        self
    }

    pub fn with_task(mut self, task: Task) -> Self {
        self.task = Some(task);
        self
    }

    pub fn with_named_step(mut self, named_step: NamedStep) -> Self {
        self.named_step = Some(named_step);
        self
    }

    pub async fn build(self, pool: &PgPool) -> WorkflowStep {
        let task = if let Some(t) = self.task {
            t
        } else {
            TaskBuilder::new().build(pool).await
        };

        let named_step = if let Some(ns) = self.named_step {
            ns
        } else {
            NamedStepBuilder::new().build(pool).await
        };

        let new_step = NewWorkflowStep {
            context: self.context.unwrap_or_else(|| serde_json::json!({})),
            max_retries: self.max_retries,
            task_id: task.task_id,
            named_step_id: named_step.named_step_id,
        };
        WorkflowStep::create(pool, new_step).await
            .expect("Failed to create test WorkflowStep")
    }

    pub async fn build_in_tx(self, pool: &PgPool) -> WorkflowStep {
        let task = if let Some(t) = self.task {
            t
        } else {
            TaskBuilder::new().build(pool).await
        };

        let named_step = if let Some(ns) = self.named_step {
            ns
        } else {
            NamedStepBuilder::new().build(pool).await
        };

        let new_step = NewWorkflowStep {
            context: self.context.unwrap_or_else(|| serde_json::json!({})),
            max_retries: self.max_retries,
            task_id: task.task_id,
            named_step_id: named_step.named_step_id,
        };
        WorkflowStep::create(pool, new_step).await
            .expect("Failed to create test WorkflowStep")
    }
}

/// Helper for creating complex workflow scenarios
pub struct WorkflowScenarioBuilder {
    task: Option<Task>,
    steps: Vec<NamedStep>,
    edges: Vec<(usize, usize)>, // Indices into steps array
}

impl WorkflowScenarioBuilder {
    pub fn new() -> Self {
        Self {
            task: None,
            steps: Vec::new(),
            edges: Vec::new(),
        }
    }

    pub fn with_task(mut self, task: Task) -> Self {
        self.task = Some(task);
        self
    }

    pub fn add_step(mut self, step: NamedStep) -> Self {
        self.steps.push(step);
        self
    }

    pub fn add_dependency(mut self, from_step_index: usize, to_step_index: usize) -> Self {
        self.edges.push((from_step_index, to_step_index));
        self
    }

    /// Create a linear workflow: step0 -> step1 -> step2 -> ...
    pub fn linear_workflow(mut self, step_count: usize) -> Self {
        for i in 0..step_count {
            if i > 0 {
                self.edges.push((i - 1, i));
            }
        }
        self
    }

    /// Create a diamond workflow: root -> left,right -> merge
    pub fn diamond_workflow(mut self) -> Self {
        // Assumes 4 steps have been added: [root, left, right, merge]
        self.edges.push((0, 1)); // root -> left
        self.edges.push((0, 2)); // root -> right
        self.edges.push((1, 3)); // left -> merge
        self.edges.push((2, 3)); // right -> merge
        self
    }

    pub async fn build(self, pool: &PgPool) -> (Task, Vec<WorkflowStep>, Vec<WorkflowStepEdge>) {
        let task = if let Some(t) = self.task {
            t
        } else {
            TaskBuilder::new().build(pool).await
        };

        // Create workflow steps
        let mut workflow_steps = Vec::new();
        for named_step in &self.steps {
            let workflow_step = WorkflowStepBuilder::new()
                .with_task(task.clone())
                .with_named_step(named_step.clone())
                .build(pool)
                .await;
            workflow_steps.push(workflow_step);
        }

        // Create edges
        let mut workflow_edges = Vec::new();
        for (from_idx, to_idx) in &self.edges {
            let edge = WorkflowStepEdge::create(
                pool,
                NewWorkflowStepEdge {
                    from_step_id: workflow_steps[*from_idx].workflow_step_id,
                    to_step_id: workflow_steps[*to_idx].workflow_step_id,
                },
            ).await.expect("Failed to create workflow edge");
            workflow_edges.push(edge);
        }

        (task, workflow_steps, workflow_edges)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::TestDatabase;

    #[tokio::test]
    async fn test_task_namespace_builder() {
        let test_db = TestDatabase::new().await;
        
        let namespace = TaskNamespaceBuilder::new()
            .with_name(&unique_name("test_builder_namespace"))
            .with_description("Built with builder pattern")
            .build(test_db.pool())
            .await;
        
        assert!(namespace.name.starts_with("test_builder_namespace"));
        assert_eq!(namespace.description, Some("Built with builder pattern".to_string()));
        
        test_db.close().await;
    }

    #[tokio::test]
    async fn test_workflow_scenario_builder() {
        let test_db = TestDatabase::new().await;
        
        // Create named steps
        let step1 = NamedStepBuilder::new().build(test_db.pool()).await;
        let step2 = NamedStepBuilder::new().build(test_db.pool()).await;
        let step3 = NamedStepBuilder::new().build(test_db.pool()).await;
        
        // Create linear workflow
        let (_task, workflow_steps, edges) = WorkflowScenarioBuilder::new()
            .add_step(step1)
            .add_step(step2)
            .add_step(step3)
            .linear_workflow(3)
            .build(test_db.pool())
            .await;
        
        assert_eq!(workflow_steps.len(), 3);
        assert_eq!(edges.len(), 2); // step1->step2, step2->step3
        
        test_db.close().await;
    }
}