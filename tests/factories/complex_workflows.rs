//! Complex workflow pattern factories that build on the existing factory system
//!
//! This module provides factories for creating complex DAG workflow patterns
//! like linear, diamond, parallel merge, tree, and mixed structures.

#![allow(dead_code)]

use super::base::*;
use super::core::*;
use super::foundation::*;
use super::relationships::WorkflowStepEdgeFactory;
use async_trait::async_trait;
use serde_json::json;
use sqlx::PgPool;
use std::collections::HashMap;

/// Workflow pattern types for complex workflows
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WorkflowPattern {
    /// Linear: A -> B -> C -> D
    Linear,
    /// Diamond: A -> (B, C) -> D
    Diamond,
    /// Parallel Merge: (A, B, C) -> D
    ParallelMerge,
    /// Tree: A -> (B -> (D, E), C -> (F, G))
    Tree,
    /// Mixed DAG with various dependency patterns
    MixedDAG,
}

/// Factory for creating complex workflow patterns
#[derive(Debug, Clone)]
pub struct ComplexWorkflowFactory {
    base: BaseFactory,
    pattern: WorkflowPattern,
    task_factory: TaskFactory,
    step_count: Option<usize>,
    namespace: String,
    with_dependencies: bool,
}

impl Default for ComplexWorkflowFactory {
    fn default() -> Self {
        Self {
            base: BaseFactory::new(),
            pattern: WorkflowPattern::Linear,
            task_factory: TaskFactory::new().complex_workflow(),
            step_count: None,
            namespace: "default".to_string(),
            with_dependencies: true,
        }
    }
}

impl ComplexWorkflowFactory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_pattern(mut self, pattern: WorkflowPattern) -> Self {
        self.pattern = pattern;
        self
    }

    pub fn linear(self) -> Self {
        self.with_pattern(WorkflowPattern::Linear)
    }

    pub fn diamond(self) -> Self {
        self.with_pattern(WorkflowPattern::Diamond)
    }

    pub fn parallel_merge(self) -> Self {
        self.with_pattern(WorkflowPattern::ParallelMerge)
    }

    pub fn tree(self) -> Self {
        self.with_pattern(WorkflowPattern::Tree)
    }

    pub fn mixed_dag(self) -> Self {
        self.with_pattern(WorkflowPattern::MixedDAG)
    }

    pub fn with_task_factory(mut self, factory: TaskFactory) -> Self {
        self.task_factory = factory;
        self
    }

    pub fn with_namespace(mut self, namespace: &str) -> Self {
        self.namespace = namespace.to_string();
        self
    }

    pub fn without_dependencies(mut self) -> Self {
        self.with_dependencies = false;
        self
    }

    async fn create_linear_workflow(&self, task_id: i64, pool: &PgPool) -> FactoryResult<Vec<i64>> {
        let system = DependentSystemFactory::new()
            .with_name("linear-system")
            .with_description("System for linear workflows")
            .find_or_create(pool)
            .await?;

        let mut step_ids = Vec::new();
        let step_count = self.step_count.unwrap_or(4);

        for i in 1..=step_count {
            let step_name = format!("linear_step_{i}");
            let named_step = NamedStepFactory::new()
                .with_name(&step_name)
                .with_system(&system.name)
                .find_or_create(pool)
                .await?;

            let workflow_step = WorkflowStepFactory::new()
                .for_task(task_id)
                .with_named_step(&named_step.name)
                .with_inputs(json!({
                    "step_number": i,
                    "pattern": "linear"
                }))
                .create(pool)
                .await?;

            // Create edge from previous step if not the first
            if i > 1 && self.with_dependencies {
                WorkflowStepEdgeFactory::new()
                    .with_from_step(step_ids[i - 2])
                    .with_to_step(workflow_step.workflow_step_id)
                    .provides()
                    .create(pool)
                    .await?;
            }

            step_ids.push(workflow_step.workflow_step_id);
        }

        Ok(step_ids)
    }

    async fn create_diamond_workflow(
        &self,
        task_id: i64,
        pool: &PgPool,
    ) -> FactoryResult<Vec<i64>> {
        let system = DependentSystemFactory::new()
            .with_name("diamond-system")
            .with_description("System for diamond workflows")
            .find_or_create(pool)
            .await?;

        let mut step_ids = Vec::new();

        // Create step A
        let step_a = NamedStepFactory::new()
            .with_name("diamond_start")
            .with_system(&system.name)
            .find_or_create(pool)
            .await?;

        let ws_a = WorkflowStepFactory::new()
            .for_task(task_id)
            .with_named_step(&step_a.name)
            .with_inputs(json!({"position": "start"}))
            .create(pool)
            .await?;
        step_ids.push(ws_a.workflow_step_id);

        // Create steps B and C
        for letter in ['b', 'c'].iter() {
            let step_name = format!("diamond_branch_{letter}");
            let named_step = NamedStepFactory::new()
                .with_name(&step_name)
                .with_system(&system.name)
                .find_or_create(pool)
                .await?;

            let ws = WorkflowStepFactory::new()
                .for_task(task_id)
                .with_named_step(&named_step.name)
                .with_inputs(json!({"branch": letter.to_string()}))
                .create(pool)
                .await?;

            if self.with_dependencies {
                WorkflowStepEdgeFactory::new()
                    .with_from_step(ws_a.workflow_step_id)
                    .with_to_step(ws.workflow_step_id)
                    .provides()
                    .create(pool)
                    .await?;
            }

            step_ids.push(ws.workflow_step_id);
        }

        // Create step D
        let step_d = NamedStepFactory::new()
            .with_name("diamond_merge")
            .with_system(&system.name)
            .find_or_create(pool)
            .await?;

        let ws_d = WorkflowStepFactory::new()
            .for_task(task_id)
            .with_named_step(&step_d.name)
            .with_inputs(json!({"position": "merge"}))
            .create(pool)
            .await?;

        if self.with_dependencies {
            // B -> D and C -> D
            for &step_id in &step_ids[1..=2] {
                WorkflowStepEdgeFactory::new()
                    .with_from_step(step_id)
                    .with_to_step(ws_d.workflow_step_id)
                    .provides()
                    .create(pool)
                    .await?;
            }
        }

        step_ids.push(ws_d.workflow_step_id);

        Ok(step_ids)
    }

    async fn create_parallel_merge_workflow(
        &self,
        task_id: i64,
        pool: &PgPool,
    ) -> FactoryResult<Vec<i64>> {
        let system = DependentSystemFactory::new()
            .with_name("parallel-system")
            .with_description("System for parallel workflows")
            .find_or_create(pool)
            .await?;

        let mut step_ids = Vec::new();
        let parallel_count = self.step_count.unwrap_or(3);

        // Create parallel steps
        for i in 1..=parallel_count {
            let step_name = format!("parallel_step_{i}");
            let named_step = NamedStepFactory::new()
                .with_name(&step_name)
                .with_system(&system.name)
                .find_or_create(pool)
                .await?;

            let ws = WorkflowStepFactory::new()
                .for_task(task_id)
                .with_named_step(&named_step.name)
                .with_inputs(json!({
                    "parallel_index": i,
                    "total_parallel": parallel_count
                }))
                .create(pool)
                .await?;

            step_ids.push(ws.workflow_step_id);
        }

        // Create merge step
        let merge_step = NamedStepFactory::new()
            .with_name("parallel_merge")
            .with_system(&system.name)
            .find_or_create(pool)
            .await?;

        let ws_merge = WorkflowStepFactory::new()
            .for_task(task_id)
            .with_named_step(&merge_step.name)
            .with_inputs(json!({"merge_count": parallel_count}))
            .create(pool)
            .await?;

        if self.with_dependencies {
            // All parallel steps -> merge step
            for step_id in &step_ids {
                WorkflowStepEdgeFactory::new()
                    .with_from_step(*step_id)
                    .with_to_step(ws_merge.workflow_step_id)
                    .provides()
                    .create(pool)
                    .await?;
            }
        }

        step_ids.push(ws_merge.workflow_step_id);

        Ok(step_ids)
    }

    async fn create_tree_workflow(&self, task_id: i64, pool: &PgPool) -> FactoryResult<Vec<i64>> {
        // Implementation for tree workflow
        // A -> (B -> (D, E), C -> (F, G))
        // TODO: Implement tree pattern
        self.create_linear_workflow(task_id, pool).await
    }

    async fn create_mixed_dag_workflow(
        &self,
        task_id: i64,
        pool: &PgPool,
    ) -> FactoryResult<Vec<i64>> {
        // Implementation for mixed DAG workflow
        // Complex pattern with various dependencies
        // TODO: Implement mixed DAG pattern
        self.create_diamond_workflow(task_id, pool).await
    }
}

#[async_trait]
impl SqlxFactory<(i64, Vec<i64>)> for ComplexWorkflowFactory {
    async fn create(&self, pool: &PgPool) -> FactoryResult<(i64, Vec<i64>)> {
        // Create the task first
        let task = self.task_factory.clone().create(pool).await?;
        let task_id = task.task_id;

        // Create workflow steps based on pattern
        let step_ids = match self.pattern {
            WorkflowPattern::Linear => self.create_linear_workflow(task_id, pool).await?,
            WorkflowPattern::Diamond => self.create_diamond_workflow(task_id, pool).await?,
            WorkflowPattern::ParallelMerge => {
                self.create_parallel_merge_workflow(task_id, pool).await?
            }
            WorkflowPattern::Tree => self.create_tree_workflow(task_id, pool).await?,
            WorkflowPattern::MixedDAG => self.create_mixed_dag_workflow(task_id, pool).await?,
        };

        Ok((task_id, step_ids))
    }

    async fn find_or_create(&self, pool: &PgPool) -> FactoryResult<(i64, Vec<i64>)> {
        // Complex workflows are always unique, so just create
        self.create(pool).await
    }
}

/// Factory for creating batches of complex workflows
#[derive(Debug, Clone)]
pub struct ComplexWorkflowBatchFactory {
    batch_size: usize,
    pattern_distribution: HashMap<WorkflowPattern, f64>,
    base_task_factory: TaskFactory,
}

impl Default for ComplexWorkflowBatchFactory {
    fn default() -> Self {
        let mut distribution = HashMap::new();
        distribution.insert(WorkflowPattern::Linear, 0.3);
        distribution.insert(WorkflowPattern::Diamond, 0.25);
        distribution.insert(WorkflowPattern::ParallelMerge, 0.2);
        distribution.insert(WorkflowPattern::Tree, 0.15);
        distribution.insert(WorkflowPattern::MixedDAG, 0.1);

        Self {
            batch_size: 50,
            pattern_distribution: distribution,
            base_task_factory: TaskFactory::new().complex_workflow(),
        }
    }
}

impl ComplexWorkflowBatchFactory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    pub fn with_distribution(mut self, distribution: HashMap<WorkflowPattern, f64>) -> Self {
        self.pattern_distribution = distribution;
        self
    }

    pub async fn create(&self, pool: &PgPool) -> FactoryResult<Vec<(i64, Vec<i64>)>> {
        let mut results = Vec::new();

        // Calculate counts for each pattern
        let mut pattern_counts: Vec<(WorkflowPattern, usize)> = self
            .pattern_distribution
            .iter()
            .map(|(pattern, ratio)| (*pattern, (self.batch_size as f64 * ratio).floor() as usize))
            .collect();

        // Adjust for rounding differences
        let total_assigned: usize = pattern_counts.iter().map(|(_, count)| count).sum();
        if total_assigned < self.batch_size {
            if let Some((_, count)) = pattern_counts
                .iter_mut()
                .find(|(p, _)| *p == WorkflowPattern::Linear)
            {
                *count += self.batch_size - total_assigned;
            }
        } else if total_assigned > self.batch_size {
            // Reduce counts if we have too many due to rounding
            let excess = total_assigned - self.batch_size;
            for (pattern, count) in pattern_counts.iter_mut() {
                if *pattern == WorkflowPattern::Linear && *count >= excess {
                    *count -= excess;
                    break;
                }
            }
        }

        // Create workflows for each pattern
        for (pattern, count) in pattern_counts {
            for _ in 0..count {
                let factory = ComplexWorkflowFactory::new()
                    .with_pattern(pattern)
                    .with_task_factory(self.base_task_factory.clone());

                let result = factory.create(pool).await?;
                results.push(result);
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    #[sqlx::test]
    async fn test_create_linear_workflow(pool: PgPool) -> FactoryResult<()> {
        let (task_id, step_ids) = ComplexWorkflowFactory::new().linear().create(&pool).await?;

        assert_eq!(step_ids.len(), 4); // Default linear has 4 steps

        // Verify edges were created
        let edge_count = sqlx::query_scalar!(
            "SELECT COUNT(*) 
             FROM tasker_workflow_step_edges e
             JOIN tasker_workflow_steps s1 ON e.from_step_id = s1.workflow_step_id
             JOIN tasker_workflow_steps s2 ON e.to_step_id = s2.workflow_step_id
             WHERE s1.task_id = $1 AND s2.task_id = $1",
            task_id
        )
        .fetch_one(&pool)
        .await?
        .unwrap_or(0);

        assert_eq!(edge_count, 3); // 3 edges for 4 steps in linear pattern

        Ok(())
    }

    #[sqlx::test]
    async fn test_create_diamond_workflow(pool: PgPool) -> FactoryResult<()> {
        let (task_id, step_ids) = ComplexWorkflowFactory::new()
            .diamond()
            .create(&pool)
            .await?;

        assert_eq!(step_ids.len(), 4); // Diamond has 4 steps: A, B, C, D

        // Verify edges were created
        let edge_count = sqlx::query_scalar!(
            "SELECT COUNT(*) 
             FROM tasker_workflow_step_edges e
             JOIN tasker_workflow_steps s1 ON e.from_step_id = s1.workflow_step_id
             JOIN tasker_workflow_steps s2 ON e.to_step_id = s2.workflow_step_id
             WHERE s1.task_id = $1 AND s2.task_id = $1",
            task_id
        )
        .fetch_one(&pool)
        .await?
        .unwrap_or(0);

        assert_eq!(edge_count, 4); // 4 edges: A->B, A->C, B->D, C->D

        Ok(())
    }

    #[sqlx::test]
    async fn test_create_parallel_merge_workflow(pool: PgPool) -> FactoryResult<()> {
        let (task_id, step_ids) = ComplexWorkflowFactory::new()
            .parallel_merge()
            .create(&pool)
            .await?;

        assert_eq!(step_ids.len(), 4); // 3 parallel + 1 merge = 4 steps

        // Verify edges were created
        let edge_count = sqlx::query_scalar!(
            "SELECT COUNT(*) 
             FROM tasker_workflow_step_edges e
             JOIN tasker_workflow_steps s1 ON e.from_step_id = s1.workflow_step_id
             JOIN tasker_workflow_steps s2 ON e.to_step_id = s2.workflow_step_id
             WHERE s1.task_id = $1 AND s2.task_id = $1",
            task_id
        )
        .fetch_one(&pool)
        .await?
        .unwrap_or(0);

        assert_eq!(edge_count, 3); // 3 edges from parallel steps to merge

        Ok(())
    }

    #[sqlx::test]
    async fn test_create_workflow_batch(pool: PgPool) -> FactoryResult<()> {
        let results = ComplexWorkflowBatchFactory::new()
            .with_size(10)
            .create(&pool)
            .await?;

        assert_eq!(results.len(), 10);

        // Verify all tasks exist
        for (task_id, _) in &results {
            let exists = sqlx::query_scalar!(
                "SELECT EXISTS(SELECT 1 FROM tasker_tasks WHERE task_id = $1) as exists",
                task_id
            )
            .fetch_one(&pool)
            .await?
            .unwrap_or(false);

            assert!(exists);
        }

        Ok(())
    }
}
