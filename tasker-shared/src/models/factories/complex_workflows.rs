//! Complex workflow pattern factories that build on the existing factory system
//!
//! This module provides factories for creating complex DAG workflow patterns
//! like linear, diamond, parallel merge, tree, and mixed structures.

use super::base::*;
use super::core::*;
use super::foundation::*;
use super::relationships::WorkflowStepEdgeFactory;
use async_trait::async_trait;
use serde_json::json;
use sqlx::{types::Uuid, PgPool};
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
    pattern: WorkflowPattern,
    task_factory: TaskFactory,
    step_count: Option<usize>,
    with_dependencies: bool,
}

impl Default for ComplexWorkflowFactory {
    fn default() -> Self {
        Self {
            pattern: WorkflowPattern::Linear,
            task_factory: TaskFactory::new().complex_workflow(),
            step_count: None,
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

    async fn create_linear_workflow(
        &self,
        task_uuid: Uuid,
        pool: &PgPool,
    ) -> FactoryResult<Vec<Uuid>> {
        let system = DependentSystemFactory::new()
            .with_name("linear-system")
            .with_description("System for linear workflows")
            .find_or_create(pool)
            .await?;

        let mut step_uuids = Vec::new();
        let step_count = self.step_count.unwrap_or(4);

        for i in 1..=step_count {
            let step_name = format!("linear_step_{i}");
            let named_step = NamedStepFactory::new()
                .with_name(&step_name)
                .with_system(&system.name)
                .find_or_create(pool)
                .await?;

            let workflow_step = WorkflowStepFactory::new()
                .for_task(task_uuid)
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
                    .with_from_step(step_uuids[i - 2])
                    .with_to_step(workflow_step.workflow_step_uuid)
                    .provides()
                    .create(pool)
                    .await?;
            }

            step_uuids.push(workflow_step.workflow_step_uuid);
        }

        Ok(step_uuids)
    }

    async fn create_diamond_workflow(
        &self,
        task_uuid: Uuid,
        pool: &PgPool,
    ) -> FactoryResult<Vec<Uuid>> {
        let system = DependentSystemFactory::new()
            .with_name("diamond-system")
            .with_description("System for diamond workflows")
            .find_or_create(pool)
            .await?;

        let mut step_uuids = Vec::new();

        // Create step A
        let step_a = NamedStepFactory::new()
            .with_name("diamond_start")
            .with_system(&system.name)
            .find_or_create(pool)
            .await?;

        let ws_a = WorkflowStepFactory::new()
            .for_task(task_uuid)
            .with_named_step(&step_a.name)
            .with_inputs(json!({"position": "start"}))
            .create(pool)
            .await?;
        step_uuids.push(ws_a.workflow_step_uuid);

        // Create steps B and C
        for letter in ['b', 'c'].iter() {
            let step_name = format!("diamond_branch_{letter}");
            let named_step = NamedStepFactory::new()
                .with_name(&step_name)
                .with_system(&system.name)
                .find_or_create(pool)
                .await?;

            let ws = WorkflowStepFactory::new()
                .for_task(task_uuid)
                .with_named_step(&named_step.name)
                .with_inputs(json!({"branch": letter.to_string()}))
                .create(pool)
                .await?;

            if self.with_dependencies {
                WorkflowStepEdgeFactory::new()
                    .with_from_step(ws_a.workflow_step_uuid)
                    .with_to_step(ws.workflow_step_uuid)
                    .provides()
                    .create(pool)
                    .await?;
            }

            step_uuids.push(ws.workflow_step_uuid);
        }

        // Create step D
        let step_d = NamedStepFactory::new()
            .with_name("diamond_merge")
            .with_system(&system.name)
            .find_or_create(pool)
            .await?;

        let ws_d = WorkflowStepFactory::new()
            .for_task(task_uuid)
            .with_named_step(&step_d.name)
            .with_inputs(json!({"position": "merge"}))
            .create(pool)
            .await?;

        if self.with_dependencies {
            // B -> D and C -> D
            for &step_uuid in &step_uuids[1..=2] {
                WorkflowStepEdgeFactory::new()
                    .with_from_step(step_uuid)
                    .with_to_step(ws_d.workflow_step_uuid)
                    .provides()
                    .create(pool)
                    .await?;
            }
        }

        step_uuids.push(ws_d.workflow_step_uuid);

        Ok(step_uuids)
    }

    async fn create_parallel_merge_workflow(
        &self,
        task_uuid: Uuid,
        pool: &PgPool,
    ) -> FactoryResult<Vec<Uuid>> {
        let system = DependentSystemFactory::new()
            .with_name("parallel-system")
            .with_description("System for parallel workflows")
            .find_or_create(pool)
            .await?;

        let mut step_uuids = Vec::new();
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
                .for_task(task_uuid)
                .with_named_step(&named_step.name)
                .with_inputs(json!({
                    "parallel_index": i,
                    "total_parallel": parallel_count
                }))
                .create(pool)
                .await?;

            step_uuids.push(ws.workflow_step_uuid);
        }

        // Create merge step
        let merge_step = NamedStepFactory::new()
            .with_name("parallel_merge")
            .with_system(&system.name)
            .find_or_create(pool)
            .await?;

        let ws_merge = WorkflowStepFactory::new()
            .for_task(task_uuid)
            .with_named_step(&merge_step.name)
            .with_inputs(json!({"merge_count": parallel_count}))
            .create(pool)
            .await?;

        if self.with_dependencies {
            // All parallel steps -> merge step
            for step_uuid in &step_uuids {
                WorkflowStepEdgeFactory::new()
                    .with_from_step(*step_uuid)
                    .with_to_step(ws_merge.workflow_step_uuid)
                    .provides()
                    .create(pool)
                    .await?;
            }
        }

        step_uuids.push(ws_merge.workflow_step_uuid);

        Ok(step_uuids)
    }

    async fn create_tree_workflow(
        &self,
        task_uuid: Uuid,
        pool: &PgPool,
    ) -> FactoryResult<Vec<Uuid>> {
        // Implementation for tree workflow
        // A -> (B -> (D, E), C -> (F, G))
        let system = DependentSystemFactory::new()
            .with_name("tree-system")
            .with_description("System for tree workflows")
            .find_or_create(pool)
            .await?;

        let mut step_uuids = Vec::new();

        // Create root step A
        let step_a = NamedStepFactory::new()
            .with_name("tree_root")
            .with_system(&system.name)
            .find_or_create(pool)
            .await?;

        let ws_a = WorkflowStepFactory::new()
            .for_task(task_uuid)
            .with_named_step(&step_a.name)
            .with_inputs(json!({"position": "root", "tree_level": 0}))
            .create(pool)
            .await?;
        step_uuids.push(ws_a.workflow_step_uuid);

        // Create branch steps B and C
        let step_b = NamedStepFactory::new()
            .with_name("tree_branch_left")
            .with_system(&system.name)
            .find_or_create(pool)
            .await?;

        let ws_b = WorkflowStepFactory::new()
            .for_task(task_uuid)
            .with_named_step(&step_b.name)
            .with_inputs(json!({"branch": "left", "tree_level": 1}))
            .create(pool)
            .await?;

        let step_c = NamedStepFactory::new()
            .with_name("tree_branch_right")
            .with_system(&system.name)
            .find_or_create(pool)
            .await?;

        let ws_c = WorkflowStepFactory::new()
            .for_task(task_uuid)
            .with_named_step(&step_c.name)
            .with_inputs(json!({"branch": "right", "tree_level": 1}))
            .create(pool)
            .await?;

        step_uuids.push(ws_b.workflow_step_uuid);
        step_uuids.push(ws_c.workflow_step_uuid);

        // Create dependencies: A -> B, A -> C
        if self.with_dependencies {
            WorkflowStepEdgeFactory::new()
                .with_from_step(ws_a.workflow_step_uuid)
                .with_to_step(ws_b.workflow_step_uuid)
                .provides()
                .create(pool)
                .await?;

            WorkflowStepEdgeFactory::new()
                .with_from_step(ws_a.workflow_step_uuid)
                .with_to_step(ws_c.workflow_step_uuid)
                .provides()
                .create(pool)
                .await?;
        }

        // Create leaf steps: D, E (children of B)
        for (i, child_name) in ["tree_leaf_d", "tree_leaf_e"].iter().enumerate() {
            let leaf_step = NamedStepFactory::new()
                .with_name(child_name)
                .with_system(&system.name)
                .find_or_create(pool)
                .await?;

            let ws_leaf = WorkflowStepFactory::new()
                .for_task(task_uuid)
                .with_named_step(&leaf_step.name)
                .with_inputs(json!({
                    "parent": "left",
                    "leaf_index": i,
                    "tree_level": 2
                }))
                .create(pool)
                .await?;

            if self.with_dependencies {
                WorkflowStepEdgeFactory::new()
                    .with_from_step(ws_b.workflow_step_uuid)
                    .with_to_step(ws_leaf.workflow_step_uuid)
                    .provides()
                    .create(pool)
                    .await?;
            }

            step_uuids.push(ws_leaf.workflow_step_uuid);
        }

        // Create leaf steps: F, G (children of C)
        for (i, child_name) in ["tree_leaf_f", "tree_leaf_g"].iter().enumerate() {
            let leaf_step = NamedStepFactory::new()
                .with_name(child_name)
                .with_system(&system.name)
                .find_or_create(pool)
                .await?;

            let ws_leaf = WorkflowStepFactory::new()
                .for_task(task_uuid)
                .with_named_step(&leaf_step.name)
                .with_inputs(json!({
                    "parent": "right",
                    "leaf_index": i,
                    "tree_level": 2
                }))
                .create(pool)
                .await?;

            if self.with_dependencies {
                WorkflowStepEdgeFactory::new()
                    .with_from_step(ws_c.workflow_step_uuid)
                    .with_to_step(ws_leaf.workflow_step_uuid)
                    .provides()
                    .create(pool)
                    .await?;
            }

            step_uuids.push(ws_leaf.workflow_step_uuid);
        }

        Ok(step_uuids)
    }

    async fn create_mixed_dag_workflow(
        &self,
        task_uuid: Uuid,
        pool: &PgPool,
    ) -> FactoryResult<Vec<Uuid>> {
        // Implementation for mixed DAG workflow
        // Complex pattern with various dependency types:
        // A -> B, A -> C, B -> D, C -> D, B -> E, C -> F, (D,E,F) -> G
        let system = DependentSystemFactory::new()
            .with_name("mixed-dag-system")
            .with_description("System for mixed DAG workflows")
            .find_or_create(pool)
            .await?;

        let mut step_uuids = Vec::new();

        // Create all named steps first
        let step_names = [
            "dag_init",
            "dag_process_left",
            "dag_process_right",
            "dag_validate",
            "dag_transform",
            "dag_analyze",
            "dag_finalize",
        ];

        for name in step_names.iter() {
            NamedStepFactory::new()
                .with_name(name)
                .with_system(&system.name)
                .find_or_create(pool)
                .await?;
        }

        // Create workflow steps
        let mut workflow_step_uuids = Vec::new();
        for (i, name) in step_names.iter().enumerate() {
            let ws = WorkflowStepFactory::new()
                .for_task(task_uuid)
                .with_named_step(name)
                .with_inputs(json!({
                    "step_type": name,
                    "dag_position": i,
                    "complexity": "mixed"
                }))
                .create(pool)
                .await?;
            workflow_step_uuids.push(ws.workflow_step_uuid);
            step_uuids.push(ws.workflow_step_uuid);
        }

        // Create complex dependency structure
        if self.with_dependencies {
            let init_id = workflow_step_uuids[0];
            let left_id = workflow_step_uuids[1];
            let right_id = workflow_step_uuids[2];
            let validate_id = workflow_step_uuids[3];
            let transform_id = workflow_step_uuids[4];
            let analyze_id = workflow_step_uuids[5];
            let final_id = workflow_step_uuids[6];

            // A -> B (init -> left)
            WorkflowStepEdgeFactory::new()
                .with_from_step(init_id)
                .with_to_step(left_id)
                .provides()
                .create(pool)
                .await?;

            // A -> C (init -> right)
            WorkflowStepEdgeFactory::new()
                .with_from_step(init_id)
                .with_to_step(right_id)
                .provides()
                .create(pool)
                .await?;

            // B -> D (left -> validate)
            WorkflowStepEdgeFactory::new()
                .with_from_step(left_id)
                .with_to_step(validate_id)
                .provides()
                .create(pool)
                .await?;

            // C -> D (right -> validate)
            WorkflowStepEdgeFactory::new()
                .with_from_step(right_id)
                .with_to_step(validate_id)
                .provides()
                .create(pool)
                .await?;

            // B -> E (left -> transform)
            WorkflowStepEdgeFactory::new()
                .with_from_step(left_id)
                .with_to_step(transform_id)
                .provides()
                .create(pool)
                .await?;

            // C -> F (right -> analyze)
            WorkflowStepEdgeFactory::new()
                .with_from_step(right_id)
                .with_to_step(analyze_id)
                .provides()
                .create(pool)
                .await?;

            // D -> G (validate -> final)
            WorkflowStepEdgeFactory::new()
                .with_from_step(validate_id)
                .with_to_step(final_id)
                .provides()
                .create(pool)
                .await?;

            // E -> G (transform -> final)
            WorkflowStepEdgeFactory::new()
                .with_from_step(transform_id)
                .with_to_step(final_id)
                .provides()
                .create(pool)
                .await?;

            // F -> G (analyze -> final)
            WorkflowStepEdgeFactory::new()
                .with_from_step(analyze_id)
                .with_to_step(final_id)
                .provides()
                .create(pool)
                .await?;
        }

        Ok(step_uuids)
    }
}

#[async_trait]
impl SqlxFactory<(Uuid, Vec<Uuid>)> for ComplexWorkflowFactory {
    async fn create(&self, pool: &PgPool) -> FactoryResult<(Uuid, Vec<Uuid>)> {
        // Create the task first
        let task = self.task_factory.clone().create(pool).await?;
        let task_uuid = task.task_uuid;

        // Create workflow steps based on pattern
        let step_uuids = match self.pattern {
            WorkflowPattern::Linear => self.create_linear_workflow(task_uuid, pool).await?,
            WorkflowPattern::Diamond => self.create_diamond_workflow(task_uuid, pool).await?,
            WorkflowPattern::ParallelMerge => {
                self.create_parallel_merge_workflow(task_uuid, pool).await?
            }
            WorkflowPattern::Tree => self.create_tree_workflow(task_uuid, pool).await?,
            WorkflowPattern::MixedDAG => self.create_mixed_dag_workflow(task_uuid, pool).await?,
        };

        Ok((task_uuid, step_uuids))
    }

    async fn find_or_create(&self, pool: &PgPool) -> FactoryResult<(Uuid, Vec<Uuid>)> {
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

    pub async fn create(&self, pool: &PgPool) -> FactoryResult<Vec<(Uuid, Vec<Uuid>)>> {
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
            for i in 0..count {
                // Create unique task factory for each workflow to avoid identity hash collisions
                let unique_task = self.base_task_factory.clone()
                    .with_context(serde_json::json!({
                        "workflow_type": "complex",
                        "pattern": format!("{:?}", pattern),
                        "batch_index": i,
                        "unique_id": format!("{:?}_{}_{}_{}", pattern, i, std::process::id(), chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)),
                        "timestamp": chrono::Utc::now().to_rfc3339()
                    }));

                let factory = ComplexWorkflowFactory::new()
                    .with_pattern(pattern)
                    .with_task_factory(unique_task);

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

    #[sqlx::test(migrator = "crate::database::migrator::MIGRATOR")]
    async fn test_create_linear_workflow(pool: PgPool) -> FactoryResult<()> {
        let (task_uuid, step_uuids) = ComplexWorkflowFactory::new().linear().create(&pool).await?;

        assert_eq!(step_uuids.len(), 4); // Default linear has 4 steps

        // Verify edges were created
        let edge_count = sqlx::query_scalar!(
            "SELECT COUNT(*)
             FROM tasker.workflow_step_edges e
             JOIN tasker.workflow_steps s1 ON e.from_step_uuid = s1.workflow_step_uuid
             JOIN tasker.workflow_steps s2 ON e.to_step_uuid = s2.workflow_step_uuid
             WHERE s1.task_uuid = $1 AND s2.task_uuid = $1",
            task_uuid
        )
        .fetch_one(&pool)
        .await?
        .unwrap_or(0);

        assert_eq!(edge_count, 3); // 3 edges for 4 steps in linear pattern

        Ok(())
    }

    #[sqlx::test(migrator = "crate::database::migrator::MIGRATOR")]
    async fn test_create_diamond_workflow(pool: PgPool) -> FactoryResult<()> {
        let (task_uuid, step_uuids) = ComplexWorkflowFactory::new()
            .diamond()
            .create(&pool)
            .await?;

        assert_eq!(step_uuids.len(), 4); // Diamond has 4 steps: A, B, C, D

        // Verify edges were created
        let edge_count = sqlx::query_scalar!(
            "SELECT COUNT(*)
             FROM tasker.workflow_step_edges e
             JOIN tasker.workflow_steps s1 ON e.from_step_uuid = s1.workflow_step_uuid
             JOIN tasker.workflow_steps s2 ON e.to_step_uuid = s2.workflow_step_uuid
             WHERE s1.task_uuid = $1 AND s2.task_uuid = $1",
            task_uuid
        )
        .fetch_one(&pool)
        .await?
        .unwrap_or(0);

        assert_eq!(edge_count, 4); // 4 edges: A->B, A->C, B->D, C->D

        Ok(())
    }

    #[sqlx::test(migrator = "crate::database::migrator::MIGRATOR")]
    async fn test_create_parallel_merge_workflow(pool: PgPool) -> FactoryResult<()> {
        let (task_uuid, step_uuids) = ComplexWorkflowFactory::new()
            .parallel_merge()
            .create(&pool)
            .await?;

        assert_eq!(step_uuids.len(), 4); // 3 parallel + 1 merge = 4 steps

        // Verify edges were created
        let edge_count = sqlx::query_scalar!(
            "SELECT COUNT(*)
             FROM tasker.workflow_step_edges e
             JOIN tasker.workflow_steps s1 ON e.from_step_uuid = s1.workflow_step_uuid
             JOIN tasker.workflow_steps s2 ON e.to_step_uuid = s2.workflow_step_uuid
             WHERE s1.task_uuid = $1 AND s2.task_uuid = $1",
            task_uuid
        )
        .fetch_one(&pool)
        .await?
        .unwrap_or(0);

        assert_eq!(edge_count, 3); // 3 edges from parallel steps to merge

        Ok(())
    }

    #[sqlx::test(migrator = "crate::database::migrator::MIGRATOR")]
    async fn test_create_tree_workflow(pool: PgPool) -> FactoryResult<()> {
        let (task_uuid, step_uuids) = ComplexWorkflowFactory::new().tree().create(&pool).await?;

        assert_eq!(step_uuids.len(), 7); // Root(1) + Branches(2) + Leaves(4) = 7 steps

        // Verify edges were created for tree structure
        let edge_count = sqlx::query_scalar!(
            "SELECT COUNT(*)
             FROM tasker.workflow_step_edges e
             JOIN tasker.workflow_steps s1 ON e.from_step_uuid = s1.workflow_step_uuid
             JOIN tasker.workflow_steps s2 ON e.to_step_uuid = s2.workflow_step_uuid
             WHERE s1.task_uuid = $1 AND s2.task_uuid = $1",
            task_uuid
        )
        .fetch_one(&pool)
        .await?
        .unwrap_or(0);

        assert_eq!(edge_count, 6); // A->B, A->C, B->D, B->E, C->F, C->G = 6 edges

        Ok(())
    }

    #[sqlx::test(migrator = "crate::database::migrator::MIGRATOR")]
    async fn test_create_mixed_dag_workflow(pool: PgPool) -> FactoryResult<()> {
        let (task_uuid, step_uuids) = ComplexWorkflowFactory::new()
            .mixed_dag()
            .create(&pool)
            .await?;

        assert_eq!(step_uuids.len(), 7); // Complex DAG with 7 steps

        // Verify edges were created for complex DAG structure
        let edge_count = sqlx::query_scalar!(
            "SELECT COUNT(*)
             FROM tasker.workflow_step_edges e
             JOIN tasker.workflow_steps s1 ON e.from_step_uuid = s1.workflow_step_uuid
             JOIN tasker.workflow_steps s2 ON e.to_step_uuid = s2.workflow_step_uuid
             WHERE s1.task_uuid = $1 AND s2.task_uuid = $1",
            task_uuid
        )
        .fetch_one(&pool)
        .await?
        .unwrap_or(0);

        assert_eq!(edge_count, 9); // Complex dependency structure with 9 edges

        Ok(())
    }

    #[sqlx::test(migrator = "crate::database::migrator::MIGRATOR")]
    async fn test_create_workflow_batch(pool: PgPool) -> FactoryResult<()> {
        let results = ComplexWorkflowBatchFactory::new()
            .with_size(10)
            .create(&pool)
            .await?;

        assert_eq!(results.len(), 10);

        // Verify all tasks exist
        for (task_uuid, _) in &results {
            let exists = sqlx::query_scalar!(
                "SELECT EXISTS(SELECT 1 FROM tasker.tasks WHERE task_uuid = $1) as exists",
                task_uuid
            )
            .fetch_one(&pool)
            .await?
            .unwrap_or(false);

            assert!(exists);
        }

        Ok(())
    }
}
