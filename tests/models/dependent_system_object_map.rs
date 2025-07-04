//! Dependent System Object Map Model Tests
//!
//! Tests for the DependentSystemObjectMap model using SQLx native testing

use sqlx::PgPool;
use tasker_core::models::dependent_system::DependentSystem;
use tasker_core::models::dependent_system_object_map::{
    DependentSystemObjectMap, NewDependentSystemObjectMap,
};

#[sqlx::test]
async fn test_dependent_system_object_map_crud(pool: PgPool) -> sqlx::Result<()> {
    // Create test systems
    let system_one = DependentSystem::find_or_create_by_name(&pool, "test_system_one").await?;
    let system_two = DependentSystem::find_or_create_by_name(&pool, "test_system_two").await?;

    // Test creation
    let new_mapping = NewDependentSystemObjectMap {
        dependent_system_one_id: system_one.dependent_system_id,
        dependent_system_two_id: system_two.dependent_system_id,
        remote_id_one: "obj_123".to_string(),
        remote_id_two: "item_456".to_string(),
    };

    let mapping = DependentSystemObjectMap::create(&pool, new_mapping.clone()).await?;
    assert_eq!(
        mapping.dependent_system_one_id,
        system_one.dependent_system_id
    );
    assert_eq!(mapping.remote_id_one, "obj_123");

    // Test find by ID
    let found =
        DependentSystemObjectMap::find_by_id(&pool, mapping.dependent_system_object_map_id).await?;
    assert!(found.is_some());
    assert_eq!(found.unwrap().remote_id_two, "item_456");

    // Test find by systems and remote IDs
    let found_specific = DependentSystemObjectMap::find_by_systems_and_remote_ids(
        &pool,
        system_one.dependent_system_id,
        system_two.dependent_system_id,
        "obj_123",
        "item_456",
    )
    .await?;
    assert!(found_specific.is_some());

    // Test find_or_create (should find existing)
    let found_or_created = DependentSystemObjectMap::find_or_create(&pool, new_mapping).await?;
    assert_eq!(
        found_or_created.dependent_system_object_map_id,
        mapping.dependent_system_object_map_id
    );

    // Test find_by_systems
    let mappings = DependentSystemObjectMap::find_by_systems(
        &pool,
        system_one.dependent_system_id,
        system_two.dependent_system_id,
    )
    .await?;
    assert!(!mappings.is_empty());

    // Test mapping between systems
    let between = DependentSystemObjectMap::find_by_systems(
        &pool,
        system_one.dependent_system_id,
        system_two.dependent_system_id,
    )
    .await?;
    assert!(!between.is_empty());

    // Test find by remote ID
    let by_remote = DependentSystemObjectMap::find_by_remote_id(&pool, "obj_123").await?;
    assert!(!by_remote.is_empty());

    // Test list all
    let all = DependentSystemObjectMap::list_all(&pool, None, None).await?;
    assert!(!all.is_empty());

    // Test recent (using list_all with limit)
    let recent = DependentSystemObjectMap::list_all(&pool, None, Some(10)).await?;
    assert!(!recent.is_empty());

    // Test count between systems (using find_by_systems and count)
    let mappings_count = DependentSystemObjectMap::find_by_systems(
        &pool,
        system_one.dependent_system_id,
        system_two.dependent_system_id,
    )
    .await?;
    assert!(mappings_count.len() > 0);

    // No cleanup needed - SQLx will roll back the test transaction automatically!
    Ok(())
}

#[sqlx::test]
async fn test_mapping_stats(pool: PgPool) -> sqlx::Result<()> {
    // Create systems for mapping
    let system_a = DependentSystem::find_or_create_by_name(&pool, "system_a").await?;
    let system_b = DependentSystem::find_or_create_by_name(&pool, "system_b").await?;
    let system_c = DependentSystem::find_or_create_by_name(&pool, "system_c").await?;

    // Create multiple mappings
    for i in 1..=3 {
        let mapping_ab = NewDependentSystemObjectMap {
            dependent_system_one_id: system_a.dependent_system_id,
            dependent_system_two_id: system_b.dependent_system_id,
            remote_id_one: format!("a_{}", i),
            remote_id_two: format!("b_{}", i),
        };
        DependentSystemObjectMap::create(&pool, mapping_ab).await?;

        let mapping_ac = NewDependentSystemObjectMap {
            dependent_system_one_id: system_a.dependent_system_id,
            dependent_system_two_id: system_c.dependent_system_id,
            remote_id_one: format!("a_{}", i),
            remote_id_two: format!("c_{}", i),
        };
        DependentSystemObjectMap::create(&pool, mapping_ac).await?;
    }

    // Test mappings with systems
    let with_stats = DependentSystemObjectMap::find_with_systems(&pool, None).await?;
    assert!(with_stats.len() >= 6);

    // Test mapping stats aggregation
    let stats = DependentSystemObjectMap::get_mapping_stats(&pool).await?;
    assert!(!stats.is_empty());

    Ok(())
}
