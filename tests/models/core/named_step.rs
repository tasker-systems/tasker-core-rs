//! Named Step Model Tests
//!
//! Tests for the NamedStep model using SQLx native testing

use sqlx::PgPool;
use tasker_core::models::dependent_system::DependentSystem;
use tasker_core::models::named_step::{NamedStep, NewNamedStep};

#[sqlx::test]
async fn test_named_step_crud(pool: PgPool) -> sqlx::Result<()> {
    // Create test system first
    let system = DependentSystem::find_or_create_by_name(&pool, "test_system").await?;

    // Test creation
    let new_step = NewNamedStep {
        dependent_system_uuid: system.dependent_system_uuid,
        name: "test_step".to_string(),
        description: Some("Test step description".to_string()),
    };

    let step = NamedStep::create(&pool, new_step.clone()).await?;
    assert_eq!(step.dependent_system_uuid, system.dependent_system_uuid);
    assert_eq!(step.description, Some("Test step description".to_string()));

    // Test find by ID
    let found = NamedStep::find_by_uuid(&pool, step.named_step_uuid).await?;
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, step.name);

    // Test find by system and name
    let found_specific =
        NamedStep::find_by_system_and_name(&pool, system.dependent_system_uuid, &step.name).await?;
    assert!(found_specific.is_some());

    // Test find_or_create (should find existing)
    let found_or_created = NamedStep::find_or_create(&pool, new_step).await?;
    assert_eq!(found_or_created.named_step_uuid, step.named_step_uuid);

    // Test find by system
    let system_steps = NamedStep::find_by_system(&pool, system.dependent_system_uuid).await?;
    assert!(!system_steps.is_empty());

    // Test search by name
    let search_results = NamedStep::search_by_name(&pool, "test", Some(10)).await?;
    assert!(!search_results.is_empty());

    // Test count by system
    let count = NamedStep::count_by_system(&pool, system.dependent_system_uuid).await?;
    assert!(count > 0);

    // Test delete
    let deleted = NamedStep::delete(&pool, step.named_step_uuid).await?;
    assert!(deleted);

    // No cleanup needed - SQLx will roll back the test transaction automatically!
    Ok(())
}

#[sqlx::test]
async fn test_unique_constraint(pool: PgPool) -> sqlx::Result<()> {
    let system = DependentSystem::find_or_create_by_name(&pool, "test_system_unique").await?;

    let step_name = "unique_test_step".to_string();

    // Create first step
    let new_step = NewNamedStep {
        dependent_system_uuid: system.dependent_system_uuid,
        name: step_name.clone(),
        description: None,
    };

    let _step1 = NamedStep::create(&pool, new_step.clone()).await?;

    // Try to create duplicate - should fail
    let duplicate_result = NamedStep::create(&pool, new_step).await;
    assert!(
        duplicate_result.is_err(),
        "Should not allow duplicate system + name"
    );

    // No cleanup needed - SQLx will roll back the test transaction automatically!
    Ok(())
}
