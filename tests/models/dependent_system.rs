//! Dependent System Model Tests
//!
//! Tests for the DependentSystem model using SQLx native testing

use sqlx::PgPool;
use tasker_core::models::dependent_system::{DependentSystem, NewDependentSystem};

#[sqlx::test]
async fn test_dependent_system_crud(pool: PgPool) -> sqlx::Result<()> {
    // Test creation
    let new_system = NewDependentSystem {
        name: "test_system".to_string(),
        description: Some("Test system description".to_string()),
    };

    let created = DependentSystem::create(&pool, new_system).await?;
    assert_eq!(created.name, "test_system");
    assert_eq!(
        created.description,
        Some("Test system description".to_string())
    );

    // Test find by ID
    let found = DependentSystem::find_by_id(&pool, created.dependent_system_id)
        .await?
        .expect("Dependent system not found");
    assert_eq!(found.dependent_system_id, created.dependent_system_id);

    // Test find by name
    let found_by_name = DependentSystem::find_by_name(&pool, "test_system")
        .await?
        .expect("Dependent system not found");
    assert_eq!(
        found_by_name.dependent_system_id,
        created.dependent_system_id
    );

    // Test find_or_create_by_name (should find existing)
    let found_or_created = DependentSystem::find_or_create_by_name(&pool, "test_system").await?;
    assert_eq!(
        found_or_created.dependent_system_id,
        created.dependent_system_id
    );

    // Test find_or_create_by_name (should create new)
    let new_system = DependentSystem::find_or_create_by_name(&pool, "another_system").await?;
    assert_eq!(new_system.name, "another_system");

    // Test search
    let search_results = DependentSystem::search_by_name(&pool, "test").await?;
    assert!(!search_results.is_empty());

    // Test update
    let mut system = created.clone();
    system
        .update(&pool, Some("updated_system"), Some("Updated description"))
        .await?;
    assert_eq!(system.name, "updated_system");

    // No cleanup needed - SQLx will roll back the test transaction automatically!
    Ok(())
}

#[sqlx::test]
async fn test_dependent_system_unique_constraints(pool: PgPool) -> sqlx::Result<()> {
    // Create first dependent system
    let new_system = NewDependentSystem {
        name: "unique_system".to_string(),
        description: None,
    };
    let _created = DependentSystem::create(&pool, new_system).await?;

    // Try to create duplicate - should fail
    let duplicate_system = NewDependentSystem {
        name: "unique_system".to_string(),
        description: Some("This should fail".to_string()),
    };

    let result = DependentSystem::create(&pool, duplicate_system).await;
    assert!(result.is_err(), "Should not allow duplicate names");

    Ok(())
}
