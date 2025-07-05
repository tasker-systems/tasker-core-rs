//! Step DAG Relationship Tests
//!
//! Tests for the StepDagRelationship model using SQLx native testing

use sqlx::PgPool;
use tasker_core::models::orchestration::step_dag_relationship::StepDagRelationship;

#[sqlx::test]
async fn test_get_step_dag_relationships(pool: PgPool) -> sqlx::Result<()> {
    // Test getting all DAG relationships
    let relationships = StepDagRelationship::get_all(&pool)
        .await
        .expect("Failed to get DAG relationships");

    // Should return without error (might be empty for new system)
    // Length is always non-negative, so just verify we got a valid result

    // If we have relationships, test the helper methods
    if let Some(rel) = relationships.first() {
        let _parent_ids = rel.parent_ids();
        let _child_ids = rel.child_ids();
        let _can_execute = rel.can_execute_immediately();
        let _is_exit = rel.is_workflow_exit();
        let _level = rel.execution_level();
        let _is_orphaned = rel.is_orphaned();
    }

    Ok(())
}

#[sqlx::test]
async fn test_get_by_task(pool: PgPool) -> sqlx::Result<()> {
    // Test with a hypothetical task ID
    let relationships = StepDagRelationship::get_by_task(&pool, 999999)
        .await
        .expect("Failed to get DAG relationships by task");

    // Should return empty for non-existent task
    assert!(relationships.is_empty());

    Ok(())
}

#[sqlx::test]
async fn test_get_root_and_leaf_steps(pool: PgPool) -> sqlx::Result<()> {
    // Test with a hypothetical task ID
    let root_steps = StepDagRelationship::get_root_steps(&pool, 999999)
        .await
        .expect("Failed to get root steps");
    let leaf_steps = StepDagRelationship::get_leaf_steps(&pool, 999999)
        .await
        .expect("Failed to get leaf steps");

    // Should return empty for non-existent task
    assert!(root_steps.is_empty());
    assert!(leaf_steps.is_empty());

    Ok(())
}

#[test]
fn test_helper_methods() {
    let relationship = StepDagRelationship {
        workflow_step_id: 1,
        task_id: 1,
        named_step_id: 1,
        parent_step_ids: serde_json::json!([10, 20, 30]),
        child_step_ids: serde_json::json!([40, 50]),
        parent_count: 3,
        child_count: 2,
        is_root_step: false,
        is_leaf_step: false,
        min_depth_from_root: Some(2),
    };

    assert_eq!(relationship.parent_ids(), vec![10, 20, 30]);
    assert_eq!(relationship.child_ids(), vec![40, 50]);
    assert!(!relationship.can_execute_immediately());
    assert!(!relationship.is_workflow_exit());
    assert_eq!(relationship.execution_level(), Some(2));
    assert!(!relationship.is_orphaned());
}
