//! # Integration Tests - Factories + Scopes + DAG Scenarios
//!
//! Comprehensive integration tests that demonstrate the full power of our factory system
//! working with the complete scope system to create and test complex workflow scenarios.
//!
//! These tests focus on:
//! - Reusable factory patterns for complex dependencies
//! - Declarative workflow scenario creation
//! - End-to-end scope validation with real data
//! - DAG patterns: 3-4 step linear, 5-7 step diamond/tree

#![allow(dead_code)]

mod factories;
mod orchestration;

use factories::base::SqlxFactory;
use factories::core::{TaskFactory, WorkflowStepFactory};
use factories::foundation::{NamedTaskFactory, TaskNamespaceFactory};
use factories::relationships::WorkflowStepEdgeFactory;
use factories::states::WorkflowStepTransitionFactory;

use serde_json::json;
use sqlx::PgPool;
use std::collections::HashMap;

use tasker_core::models::{
    NamedTask, Task, TaskNamespace, WorkflowStep, WorkflowStepEdge, WorkflowStepTransition,
};
use tasker_core::scopes::*;

// =============================================================================
// REUSABLE FOUNDATION PATTERNS
// =============================================================================

/// Common foundation setup used across multiple workflow tests
/// Creates the standard set of namespaces, systems, and named tasks
pub struct StandardFoundation {
    pub namespaces: HashMap<String, TaskNamespace>,
    pub named_tasks: HashMap<String, NamedTask>,
}

impl StandardFoundation {
    pub async fn setup(pool: &PgPool) -> Result<Self, Box<dyn std::error::Error>> {
        // Create standard namespaces
        let default_ns = TaskNamespaceFactory::new()
            .with_name("default")
            .with_description("Default namespace for core workflows")
            .find_or_create(pool)
            .await?;

        let ecommerce_ns = TaskNamespaceFactory::new()
            .with_name("ecommerce")
            .with_description("E-commerce workflow patterns")
            .find_or_create(pool)
            .await?;

        let mut namespaces = HashMap::new();
        namespaces.insert("default".to_string(), default_ns.clone());
        namespaces.insert("ecommerce".to_string(), ecommerce_ns.clone());

        // Create standard named tasks for different workflow patterns
        let linear_task = NamedTaskFactory::new()
            .with_name("linear_workflow")
            .with_namespace("default")
            .find_or_create(pool)
            .await?;

        let diamond_task = NamedTaskFactory::new()
            .with_name("diamond_workflow")
            .with_namespace("default")
            .find_or_create(pool)
            .await?;

        let tree_task = NamedTaskFactory::new()
            .with_name("tree_workflow")
            .with_namespace("ecommerce")
            .find_or_create(pool)
            .await?;

        let mut named_tasks = HashMap::new();
        named_tasks.insert("linear_workflow".to_string(), linear_task);
        named_tasks.insert("diamond_workflow".to_string(), diamond_task);
        named_tasks.insert("tree_workflow".to_string(), tree_task);

        Ok(StandardFoundation {
            namespaces,
            named_tasks,
        })
    }
}

/// Linear workflow scenario: A -> B -> C (3 steps)
/// Simple sequential processing pattern
pub struct LinearWorkflowScenario {
    pub task: Task,
    pub steps: Vec<WorkflowStep>,
    pub edges: Vec<WorkflowStepEdge>,
}

impl LinearWorkflowScenario {
    pub async fn create(
        pool: &PgPool,
        _foundation: &StandardFoundation,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Create the task
        let task = TaskFactory::new()
            .with_named_task("linear_workflow")
            .with_context(json!({
                "workflow_type": "linear",
                "step_count": 3,
                "pattern": "sequential_processing"
            }))
            .create(pool)
            .await?;

        // Create 3 sequential steps
        let step_a = WorkflowStepFactory::new()
            .for_task(task.task_uuid)
            .with_named_step("validation_step")
            .with_inputs(json!({"data": "input_data"}))
            .create(pool)
            .await?;

        let step_b = WorkflowStepFactory::new()
            .for_task(task.task_uuid)
            .with_named_step("processing_step")
            .with_inputs(json!({"validated_data": "processed"}))
            .create(pool)
            .await?;

        let step_c = WorkflowStepFactory::new()
            .for_task(task.task_uuid)
            .with_named_step("finalization_step")
            .with_inputs(json!({"processed_data": "final"}))
            .create(pool)
            .await?;

        let steps = vec![step_a.clone(), step_b.clone(), step_c.clone()];

        // Create edges: A -> B -> C
        let edge_ab = WorkflowStepEdgeFactory::new()
            .with_from_step(step_a.workflow_step_uuid)
            .with_to_step(step_b.workflow_step_uuid)
            .provides()
            .create(pool)
            .await?;

        let edge_bc = WorkflowStepEdgeFactory::new()
            .with_from_step(step_b.workflow_step_uuid)
            .with_to_step(step_c.workflow_step_uuid)
            .provides()
            .create(pool)
            .await?;

        let edges = vec![edge_ab, edge_bc];

        Ok(LinearWorkflowScenario { task, steps, edges })
    }
}

/// Diamond workflow scenario: A -> (B, C) -> D (4 steps)
/// Fork-join pattern for parallel processing
pub struct DiamondWorkflowScenario {
    pub task: Task,
    pub steps: Vec<WorkflowStep>,
    pub edges: Vec<WorkflowStepEdge>,
}

impl DiamondWorkflowScenario {
    pub async fn create(
        pool: &PgPool,
        _foundation: &StandardFoundation,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Create the task
        let task = TaskFactory::new()
            .with_named_task("diamond_workflow")
            .with_context(json!({
                "workflow_type": "diamond",
                "step_count": 4,
                "pattern": "fork_join_parallel"
            }))
            .create(pool)
            .await?;

        // Create 4 steps in diamond pattern
        let step_a = WorkflowStepFactory::new()
            .for_task(task.task_uuid)
            .with_named_step("initialization_step")
            .create(pool)
            .await?;

        let step_b = WorkflowStepFactory::new()
            .for_task(task.task_uuid)
            .with_named_step("parallel_branch_1")
            .create(pool)
            .await?;

        let step_c = WorkflowStepFactory::new()
            .for_task(task.task_uuid)
            .with_named_step("parallel_branch_2")
            .create(pool)
            .await?;

        let step_d = WorkflowStepFactory::new()
            .for_task(task.task_uuid)
            .with_named_step("merge_step")
            .create(pool)
            .await?;

        let steps = vec![
            step_a.clone(),
            step_b.clone(),
            step_c.clone(),
            step_d.clone(),
        ];

        // Create diamond edges: A -> B, A -> C, B -> D, C -> D
        let edge_ab = WorkflowStepEdgeFactory::new()
            .with_from_step(step_a.workflow_step_uuid)
            .with_to_step(step_b.workflow_step_uuid)
            .provides()
            .create(pool)
            .await?;

        let edge_ac = WorkflowStepEdgeFactory::new()
            .with_from_step(step_a.workflow_step_uuid)
            .with_to_step(step_c.workflow_step_uuid)
            .provides()
            .create(pool)
            .await?;

        let edge_bd = WorkflowStepEdgeFactory::new()
            .with_from_step(step_b.workflow_step_uuid)
            .with_to_step(step_d.workflow_step_uuid)
            .provides()
            .create(pool)
            .await?;

        let edge_cd = WorkflowStepEdgeFactory::new()
            .with_from_step(step_c.workflow_step_uuid)
            .with_to_step(step_d.workflow_step_uuid)
            .provides()
            .create(pool)
            .await?;

        let edges = vec![edge_ab, edge_ac, edge_bd, edge_cd];

        Ok(DiamondWorkflowScenario { task, steps, edges })
    }
}

/// Tree workflow scenario: A -> (B -> (D, E), C -> (F, G)) (7 steps)
/// Hierarchical branching pattern for complex orchestration
pub struct TreeWorkflowScenario {
    pub task: Task,
    pub steps: Vec<WorkflowStep>,
    pub edges: Vec<WorkflowStepEdge>,
}

impl TreeWorkflowScenario {
    pub async fn create(
        pool: &PgPool,
        _foundation: &StandardFoundation,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        // Create the task
        let task = TaskFactory::new()
            .with_named_task("tree_workflow")
            .with_context(json!({
                "workflow_type": "tree",
                "step_count": 7,
                "pattern": "hierarchical_branching"
            }))
            .create(pool)
            .await?;

        // Create 7 steps in tree pattern
        let step_a = WorkflowStepFactory::new()
            .for_task(task.task_uuid)
            .with_named_step("root_step")
            .create(pool)
            .await?;

        let step_b = WorkflowStepFactory::new()
            .for_task(task.task_uuid)
            .with_named_step("branch_1_main")
            .create(pool)
            .await?;

        let step_c = WorkflowStepFactory::new()
            .for_task(task.task_uuid)
            .with_named_step("branch_2_main")
            .create(pool)
            .await?;

        let step_d = WorkflowStepFactory::new()
            .for_task(task.task_uuid)
            .with_named_step("branch_1_leaf_1")
            .create(pool)
            .await?;

        let step_e = WorkflowStepFactory::new()
            .for_task(task.task_uuid)
            .with_named_step("branch_1_leaf_2")
            .create(pool)
            .await?;

        let step_f = WorkflowStepFactory::new()
            .for_task(task.task_uuid)
            .with_named_step("branch_2_leaf_1")
            .create(pool)
            .await?;

        let step_g = WorkflowStepFactory::new()
            .for_task(task.task_uuid)
            .with_named_step("branch_2_leaf_2")
            .create(pool)
            .await?;

        let steps = vec![
            step_a.clone(),
            step_b.clone(),
            step_c.clone(),
            step_d.clone(),
            step_e.clone(),
            step_f.clone(),
            step_g.clone(),
        ];

        // Create tree edges: A -> B, A -> C, B -> D, B -> E, C -> F, C -> G
        let edges = vec![
            WorkflowStepEdgeFactory::new()
                .with_from_step(step_a.workflow_step_uuid)
                .with_to_step(step_b.workflow_step_uuid)
                .provides()
                .create(pool)
                .await?,
            WorkflowStepEdgeFactory::new()
                .with_from_step(step_a.workflow_step_uuid)
                .with_to_step(step_c.workflow_step_uuid)
                .provides()
                .create(pool)
                .await?,
            WorkflowStepEdgeFactory::new()
                .with_from_step(step_b.workflow_step_uuid)
                .with_to_step(step_d.workflow_step_uuid)
                .provides()
                .create(pool)
                .await?,
            WorkflowStepEdgeFactory::new()
                .with_from_step(step_b.workflow_step_uuid)
                .with_to_step(step_e.workflow_step_uuid)
                .provides()
                .create(pool)
                .await?,
            WorkflowStepEdgeFactory::new()
                .with_from_step(step_c.workflow_step_uuid)
                .with_to_step(step_f.workflow_step_uuid)
                .provides()
                .create(pool)
                .await?,
            WorkflowStepEdgeFactory::new()
                .with_from_step(step_c.workflow_step_uuid)
                .with_to_step(step_g.workflow_step_uuid)
                .provides()
                .create(pool)
                .await?,
        ];

        Ok(TreeWorkflowScenario { task, steps, edges })
    }
}

// =============================================================================
// INTEGRATION TESTS - LINEAR WORKFLOW
// =============================================================================

#[sqlx::test]
async fn test_linear_workflow_scope_integration(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Setup foundation
    let foundation = StandardFoundation::setup(&pool).await?;

    // Create linear workflow
    let scenario = LinearWorkflowScenario::create(&pool, &foundation).await?;

    // Test Task scopes with real data
    let tasks_in_default = Task::scope()
        .in_namespace("default".to_string())
        .all(&pool)
        .await?;
    assert!(!tasks_in_default.is_empty());

    // Test WorkflowStep scopes
    let steps_for_task = WorkflowStep::scope()
        .for_task(scenario.task.task_uuid)
        .all(&pool)
        .await?;
    assert_eq!(steps_for_task.len(), 3);

    // Test WorkflowStepEdge scopes - children relationship
    let children_of_first = WorkflowStepEdge::scope()
        .children_of(scenario.steps[0].workflow_step_uuid)
        .all(&pool)
        .await?;
    assert_eq!(children_of_first.len(), 1);
    assert_eq!(
        children_of_first[0].to_step_uuid,
        scenario.steps[1].workflow_step_uuid
    );

    // Test parents relationship
    let parents_of_last = WorkflowStepEdge::scope()
        .parents_of(scenario.steps[2].workflow_step_uuid)
        .all(&pool)
        .await?;
    assert_eq!(parents_of_last.len(), 1);
    assert_eq!(
        parents_of_last[0].from_step_uuid,
        scenario.steps[1].workflow_step_uuid
    );

    // Test NamedTask scopes
    let tasks_in_default_ns = NamedTask::scope()
        .in_namespace("default".to_string())
        .all(&pool)
        .await?;
    assert!(tasks_in_default_ns.len() >= 2); // Should have linear and diamond

    Ok(())
}

#[sqlx::test]
async fn test_linear_workflow_with_state_transitions(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Setup foundation and workflow
    let foundation = StandardFoundation::setup(&pool).await?;
    let scenario = LinearWorkflowScenario::create(&pool, &foundation).await?;

    // Create state transitions for the linear workflow
    // Step A: pending -> in_progress -> complete
    WorkflowStepTransitionFactory::create_complete_lifecycle(
        scenario.steps[0].workflow_step_uuid,
        &pool,
    )
    .await?;

    // Step B: pending -> in_progress -> error -> pending -> in_progress -> complete (retry scenario)
    WorkflowStepTransitionFactory::create_failed_lifecycle(
        scenario.steps[1].workflow_step_uuid,
        "Network timeout during processing",
        &pool,
    )
    .await?;

    WorkflowStepTransitionFactory::create_retry_lifecycle(
        scenario.steps[1].workflow_step_uuid,
        2, // Second attempt
        &pool,
    )
    .await?;

    // Step C: pending (waiting for dependencies)
    WorkflowStepTransitionFactory::new()
        .for_workflow_step(scenario.steps[2].workflow_step_uuid)
        .to_state("pending")
        .create(&pool)
        .await?;

    // Test WorkflowStepTransition scopes
    let recent_transitions = WorkflowStepTransition::scope().recent().all(&pool).await?;
    assert!(recent_transitions.len() >= 6); // At least 6 transitions total

    let complete_transitions = WorkflowStepTransition::scope()
        .to_state("complete".to_string())
        .all(&pool)
        .await?;
    assert_eq!(complete_transitions.len(), 2); // Steps A and B completed

    let error_transitions = WorkflowStepTransition::scope()
        .to_state("error".to_string())
        .all(&pool)
        .await?;
    assert_eq!(error_transitions.len(), 1); // Step B failed once

    let transitions_for_task = WorkflowStepTransition::scope()
        .for_task(scenario.task.task_uuid)
        .all(&pool)
        .await?;
    assert!(transitions_for_task.len() >= 6); // All transitions for this task

    let retry_transitions = WorkflowStepTransition::scope()
        .with_metadata_key("retry_attempt".to_string())
        .all(&pool)
        .await?;
    assert_eq!(retry_transitions.len(), 1); // One retry transition

    Ok(())
}

// =============================================================================
// INTEGRATION TESTS - DIAMOND WORKFLOW
// =============================================================================

#[sqlx::test]
async fn test_diamond_workflow_scope_integration(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Setup foundation and workflow
    let foundation = StandardFoundation::setup(&pool).await?;
    let scenario = DiamondWorkflowScenario::create(&pool, &foundation).await?;

    // Test the critical siblings_of scope with diamond pattern
    // In diamond: B and C are siblings (same parent A, same children D)
    let siblings_of_b = WorkflowStepEdge::scope()
        .siblings_of(scenario.steps[1].workflow_step_uuid) // Step B
        .all(&pool)
        .await?;

    // Should find edges pointing to step C (sibling of B)
    assert_eq!(siblings_of_b.len(), 1);
    assert_eq!(
        siblings_of_b[0].to_step_uuid,
        scenario.steps[2].workflow_step_uuid
    ); // Points to C

    let siblings_of_c = WorkflowStepEdge::scope()
        .siblings_of(scenario.steps[2].workflow_step_uuid) // Step C
        .all(&pool)
        .await?;

    // Should find edges pointing to step B (sibling of C)
    assert_eq!(siblings_of_c.len(), 1);
    assert_eq!(
        siblings_of_c[0].to_step_uuid,
        scenario.steps[1].workflow_step_uuid
    ); // Points to B

    // Test provides_to_children scope
    let provides_to_children_of_a = WorkflowStepEdge::scope()
        .provides_to_children(scenario.steps[0].workflow_step_uuid) // Step A
        .all(&pool)
        .await?;

    // In diamond pattern: A provides to B,C and B,C provide to D
    // So A's children (B,C) provide to their children (D), expecting 2 edges: B->D, C->D
    assert_eq!(provides_to_children_of_a.len(), 2);

    // Test children and parents with diamond structure
    let children_of_a = WorkflowStepEdge::scope()
        .children_of(scenario.steps[0].workflow_step_uuid)
        .all(&pool)
        .await?;
    assert_eq!(children_of_a.len(), 2); // A has 2 children: B and C

    let parents_of_d = WorkflowStepEdge::scope()
        .parents_of(scenario.steps[3].workflow_step_uuid)
        .all(&pool)
        .await?;
    assert_eq!(parents_of_d.len(), 2); // D has 2 parents: B and C

    Ok(())
}

// =============================================================================
// INTEGRATION TESTS - TREE WORKFLOW
// =============================================================================

#[sqlx::test]
async fn test_tree_workflow_scope_integration(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Setup foundation and workflow
    let foundation = StandardFoundation::setup(&pool).await?;
    let scenario = TreeWorkflowScenario::create(&pool, &foundation).await?;

    // Test tree structure with scopes
    let children_of_root = WorkflowStepEdge::scope()
        .children_of(scenario.steps[0].workflow_step_uuid) // Root A
        .all(&pool)
        .await?;
    assert_eq!(children_of_root.len(), 2); // A has 2 children: B and C

    let children_of_b = WorkflowStepEdge::scope()
        .children_of(scenario.steps[1].workflow_step_uuid) // Branch 1 main (B)
        .all(&pool)
        .await?;
    assert_eq!(children_of_b.len(), 2); // B has 2 children: D and E

    let children_of_c = WorkflowStepEdge::scope()
        .children_of(scenario.steps[2].workflow_step_uuid) // Branch 2 main (C)
        .all(&pool)
        .await?;
    assert_eq!(children_of_c.len(), 2); // C has 2 children: F and G

    // Test siblings at leaf level
    // D and E are siblings (both children of B)
    let siblings_of_d = WorkflowStepEdge::scope()
        .siblings_of(scenario.steps[3].workflow_step_uuid) // D
        .all(&pool)
        .await?;
    assert_eq!(siblings_of_d.len(), 1); // Should find edge to E
    assert_eq!(
        siblings_of_d[0].to_step_uuid,
        scenario.steps[4].workflow_step_uuid
    ); // Points to E

    // F and G are siblings (both children of C)
    let siblings_of_f = WorkflowStepEdge::scope()
        .siblings_of(scenario.steps[5].workflow_step_uuid) // F
        .all(&pool)
        .await?;
    assert_eq!(siblings_of_f.len(), 1); // Should find edge to G
    assert_eq!(
        siblings_of_f[0].to_step_uuid,
        scenario.steps[6].workflow_step_uuid
    ); // Points to G

    // Test that non-siblings don't show up (D and F are NOT siblings - different parents)
    let siblings_cross_branch = WorkflowStepEdge::scope()
        .siblings_of(scenario.steps[3].workflow_step_uuid) // D
        .all(&pool)
        .await?;

    // Should only find E, not F or G
    for edge in &siblings_cross_branch {
        assert_ne!(edge.to_step_uuid, scenario.steps[5].workflow_step_uuid); // Not F
        assert_ne!(edge.to_step_uuid, scenario.steps[6].workflow_step_uuid); // Not G
    }

    Ok(())
}

// =============================================================================
// INTEGRATION TESTS - CROSS-SCENARIO SCOPES
// =============================================================================

#[sqlx::test]
async fn test_cross_scenario_namespace_and_task_scopes(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Setup foundation
    let foundation = StandardFoundation::setup(&pool).await?;

    // Create multiple workflow scenarios
    let _linear = LinearWorkflowScenario::create(&pool, &foundation).await?;
    let _diamond = DiamondWorkflowScenario::create(&pool, &foundation).await?;
    let _tree = TreeWorkflowScenario::create(&pool, &foundation).await?;

    // Test TaskNamespace scopes
    let custom_namespaces = TaskNamespace::scope().custom().all(&pool).await?;

    // Should have ecommerce namespace (not default)
    assert!(custom_namespaces.iter().any(|ns| ns.name == "ecommerce"));
    assert!(!custom_namespaces.iter().any(|ns| ns.name == "default"));

    // Test NamedTask scopes across namespaces
    let tasks_in_default = NamedTask::scope()
        .in_namespace("default".to_string())
        .all(&pool)
        .await?;
    assert!(tasks_in_default.len() >= 2); // linear and diamond

    let tasks_in_ecommerce = NamedTask::scope()
        .in_namespace("ecommerce".to_string())
        .all(&pool)
        .await?;
    assert!(!tasks_in_ecommerce.is_empty()); // tree

    // Test latest versions
    let latest_tasks = NamedTask::scope().latest_versions().all(&pool).await?;
    assert!(latest_tasks.len() >= 3); // At least our 3 workflow types

    // Create initial workflow step transitions to make tasks "active"
    // For this test we just need at least one step per task to be in a non-complete state
    WorkflowStepTransitionFactory::new()
        .for_workflow_step(_linear.steps[0].workflow_step_uuid)
        .to_state("pending")
        .create(&pool)
        .await?;

    WorkflowStepTransitionFactory::new()
        .for_workflow_step(_diamond.steps[0].workflow_step_uuid)
        .to_state("pending")
        .create(&pool)
        .await?;

    WorkflowStepTransitionFactory::new()
        .for_workflow_step(_tree.steps[0].workflow_step_uuid)
        .to_state("pending")
        .create(&pool)
        .await?;

    // Test Task scopes across multiple workflows
    let all_tasks = Task::scope().active().all(&pool).await?;
    assert_eq!(all_tasks.len(), 3); // Linear, diamond, tree tasks

    Ok(())
}

#[sqlx::test]
async fn test_comprehensive_state_transition_scenarios(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Setup foundation and create one of each workflow type
    let foundation = StandardFoundation::setup(&pool).await?;
    let linear = LinearWorkflowScenario::create(&pool, &foundation).await?;
    let diamond = DiamondWorkflowScenario::create(&pool, &foundation).await?;

    // Create diverse state transition scenarios

    // Linear workflow: Complete success
    for step in &linear.steps {
        WorkflowStepTransitionFactory::create_complete_lifecycle(step.workflow_step_uuid, &pool)
            .await?;
    }

    // Diamond workflow: Mixed states with manual resolution
    WorkflowStepTransitionFactory::create_complete_lifecycle(
        diamond.steps[0].workflow_step_uuid, // A completes
        &pool,
    )
    .await?;

    WorkflowStepTransitionFactory::create_failed_lifecycle(
        diamond.steps[1].workflow_step_uuid, // B fails
        "Downstream API unavailable",
        &pool,
    )
    .await?;

    WorkflowStepTransitionFactory::create_complete_lifecycle(
        diamond.steps[2].workflow_step_uuid, // C completes
        &pool,
    )
    .await?;

    WorkflowStepTransitionFactory::create_manual_resolution_lifecycle(
        diamond.steps[1].workflow_step_uuid, // B manually resolved
        "ops-team@company.com",
        &pool,
    )
    .await?;

    // Test comprehensive transition queries
    let all_complete = WorkflowStepTransition::scope()
        .to_state("complete".to_string())
        .all(&pool)
        .await?;
    assert!(all_complete.len() >= 5); // 3 linear + 2 diamond (A, C)

    let all_errors = WorkflowStepTransition::scope()
        .to_state("error".to_string())
        .all(&pool)
        .await?;
    assert_eq!(all_errors.len(), 1); // Diamond B failed

    let manual_resolutions = WorkflowStepTransition::scope()
        .to_state("resolved_manually".to_string())
        .all(&pool)
        .await?;
    assert_eq!(manual_resolutions.len(), 1); // Diamond B resolved

    let transitions_with_errors = WorkflowStepTransition::scope()
        .with_metadata_key("error_message".to_string())
        .all(&pool)
        .await?;
    assert!(!transitions_with_errors.is_empty());

    let transitions_with_resolvers = WorkflowStepTransition::scope()
        .with_metadata_key("resolved_by".to_string())
        .all(&pool)
        .await?;
    assert_eq!(transitions_with_resolvers.len(), 1);

    // Test task-level transition queries
    let linear_transitions = WorkflowStepTransition::scope()
        .for_task(linear.task.task_uuid)
        .all(&pool)
        .await?;
    assert_eq!(linear_transitions.len(), 9); // 3 steps × 3 transitions each

    let diamond_transitions = WorkflowStepTransition::scope()
        .for_task(diamond.task.task_uuid)
        .all(&pool)
        .await?;
    assert!(diamond_transitions.len() >= 10); // 4 steps with various transition counts

    Ok(())
}

// =============================================================================
// LEGACY FLAG INTEGRATION TESTS
// =============================================================================

/// Test that Task.complete is properly updated when state transitions occur
#[sqlx::test]
async fn test_task_complete_flag_integration_with_state_transitions(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    use tasker_core::events::publisher::EventPublisher;
    use tasker_core::models::core::task::Task;
    use tasker_core::state_machine::events::TaskEvent;
    use tasker_core::state_machine::states::TaskState;
    use tasker_core::state_machine::task_state_machine::TaskStateMachine;

    // Setup foundation and create a simple task (no steps for this test)
    let _foundation = StandardFoundation::setup(&pool).await?;

    // Create a simple task without workflow steps to test the complete flag
    use factories::core::TaskFactory;
    let test_task = TaskFactory::new()
        .with_named_task("linear_workflow")
        .with_context(serde_json::json!({"test": "task_complete_flag"}))
        .create(&pool)
        .await?;

    // Get the task before state transition
    let task_before = Task::find_by_id(&pool, test_task.task_uuid).await?.unwrap();
    assert!(!task_before.complete, "Task should initially be incomplete");

    // Create state machine and transition to complete
    let event_publisher = EventPublisher::default();
    let mut state_machine = TaskStateMachine::new(test_task.clone(), pool.clone(), event_publisher);

    // First transition to in_progress state
    let result = state_machine.transition(TaskEvent::Start).await;
    match &result {
        Ok(state) => {
            assert_eq!(
                *state,
                TaskState::InProgress,
                "Task should be in in_progress state"
            );
        }
        Err(e) => {
            panic!("State transition to in_progress should succeed, but got error: {e:?}");
        }
    }

    // Then transition to complete state
    let result = state_machine.transition(TaskEvent::Complete).await;
    match &result {
        Ok(state) => {
            assert_eq!(
                *state,
                TaskState::Complete,
                "Task should be in complete state"
            );
        }
        Err(e) => {
            panic!("State transition to complete should succeed, but got error: {e:?}");
        }
    }

    // Verify the task.complete flag was updated in the database
    let task_after = Task::find_by_id(&pool, test_task.task_uuid).await?.unwrap();
    assert!(
        task_after.complete,
        "Task.complete should be true after state transition"
    );

    Ok(())
}

/// Test that WorkflowStep boolean flags are properly updated when state transitions occur
#[sqlx::test]
async fn test_workflow_step_flags_integration_with_state_transitions(
    pool: PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    use tasker_core::events::publisher::EventPublisher;
    use tasker_core::models::core::workflow_step::WorkflowStep;
    use tasker_core::state_machine::events::StepEvent;
    use tasker_core::state_machine::states::WorkflowStepState;
    use tasker_core::state_machine::step_state_machine::StepStateMachine;

    // Setup foundation
    let foundation = StandardFoundation::setup(&pool).await?;
    let scenario = LinearWorkflowScenario::create(&pool, &foundation).await?;
    let test_step = &scenario.steps[0];

    // Get the step before state transitions
    let step_before = WorkflowStep::find_by_id(&pool, test_step.workflow_step_uuid)
        .await?
        .unwrap();
    assert!(
        !step_before.in_process,
        "Step should initially not be in process"
    );
    assert!(
        !step_before.processed,
        "Step should initially not be processed"
    );
    assert!(
        step_before.processed_at.is_none(),
        "Step should have no processed_at timestamp"
    );

    // Create state machine
    let event_publisher = EventPublisher::default();
    let mut state_machine = StepStateMachine::new(test_step.clone(), pool.clone(), event_publisher);

    // Transition to in_progress state
    let result = state_machine.transition(StepEvent::Start).await;
    assert!(
        result.is_ok(),
        "State transition to in_progress should succeed"
    );
    assert_eq!(
        result.unwrap(),
        WorkflowStepState::InProgress,
        "Step should be in in_progress state"
    );

    // Verify the step.in_process flag was updated
    let step_in_progress = WorkflowStep::find_by_id(&pool, test_step.workflow_step_uuid)
        .await?
        .unwrap();
    assert!(
        step_in_progress.in_process,
        "Step.in_process should be true after transitioning to in_progress"
    );
    assert!(
        !step_in_progress.processed,
        "Step.processed should still be false"
    );

    // Transition to complete state
    let results = Some(serde_json::json!({"result": "success"}));
    let result = state_machine
        .transition(StepEvent::Complete(results.clone()))
        .await;
    assert!(
        result.is_ok(),
        "State transition to complete should succeed"
    );
    assert_eq!(
        result.unwrap(),
        WorkflowStepState::Complete,
        "Step should be in complete state"
    );

    // Verify the step.processed and processed_at flags were updated
    let step_complete = WorkflowStep::find_by_id(&pool, test_step.workflow_step_uuid)
        .await?
        .unwrap();
    assert!(
        step_complete.processed,
        "Step.processed should be true after transitioning to complete"
    );
    assert!(
        step_complete.processed_at.is_some(),
        "Step.processed_at should be set after transitioning to complete"
    );

    // Verify the in_process flag is now false (step is no longer in process after completion)
    assert!(
        !step_complete.in_process,
        "Step.in_process should be false after completion"
    );

    Ok(())
}
