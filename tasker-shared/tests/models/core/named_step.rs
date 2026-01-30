//! Named Step Model Tests
//!
//! Tests for the NamedStep model using SQLx native testing

use sqlx::PgPool;
use tasker_shared::models::named_step::{NamedStep, NewNamedStep};

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_named_step_crud(pool: PgPool) -> sqlx::Result<()> {
    // Test creation
    let new_step = NewNamedStep {
        name: "test_step".to_string(),
        description: Some("Test step description".to_string()),
    };

    let step = NamedStep::create(&pool, new_step.clone()).await?;
    assert_eq!(step.name, "test_step");
    assert_eq!(step.description, Some("Test step description".to_string()));

    // Test find by ID
    let found = NamedStep::find_by_uuid(&pool, step.named_step_uuid).await?;
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, step.name);

    // Test find by name
    let found_by_name = NamedStep::find_by_name(&pool, &step.name).await?;
    assert!(found_by_name.is_some());
    assert_eq!(found_by_name.unwrap().named_step_uuid, step.named_step_uuid);

    // Test find_or_create (should find existing)
    let found_or_created = NamedStep::find_or_create(&pool, new_step).await?;
    assert_eq!(found_or_created.named_step_uuid, step.named_step_uuid);

    // Test search by name
    let search_results = NamedStep::search_by_name(&pool, "test", Some(10)).await?;
    assert!(!search_results.is_empty());

    // Test delete
    let deleted = NamedStep::delete(&pool, step.named_step_uuid).await?;
    assert!(deleted);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_unique_constraint(pool: PgPool) -> sqlx::Result<()> {
    let step_name = "unique_test_step".to_string();

    // Create first step
    let new_step = NewNamedStep {
        name: step_name.clone(),
        description: None,
    };

    let _step1 = NamedStep::create(&pool, new_step.clone()).await?;

    // Try to create duplicate - should fail
    let duplicate_result = NamedStep::create(&pool, new_step).await;
    assert!(
        duplicate_result.is_err(),
        "Should not allow duplicate step name"
    );

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_named_step_find_by_name(pool: PgPool) -> sqlx::Result<()> {
    // Create a named step
    let new_step = NewNamedStep {
        name: "find_by_name_step".to_string(),
        description: Some("Step to find by name".to_string()),
    };
    let created = NamedStep::create(&pool, new_step).await?;

    // Find by existing name
    let found = NamedStep::find_by_name(&pool, "find_by_name_step").await?;
    assert!(found.is_some());
    let found = found.unwrap();
    assert_eq!(found.named_step_uuid, created.named_step_uuid);
    assert_eq!(found.name, "find_by_name_step");
    assert_eq!(found.description, Some("Step to find by name".to_string()));

    // Find by non-existent name
    let not_found = NamedStep::find_by_name(&pool, "nonexistent_step").await?;
    assert!(not_found.is_none());

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_named_step_find_by_uuids(pool: PgPool) -> sqlx::Result<()> {
    // Create multiple steps
    let step1 = NamedStep::create(
        &pool,
        NewNamedStep {
            name: "batch_step_1".to_string(),
            description: Some("First batch step".to_string()),
        },
    )
    .await?;

    let step2 = NamedStep::create(
        &pool,
        NewNamedStep {
            name: "batch_step_2".to_string(),
            description: Some("Second batch step".to_string()),
        },
    )
    .await?;

    let step3 = NamedStep::create(
        &pool,
        NewNamedStep {
            name: "batch_step_3".to_string(),
            description: None,
        },
    )
    .await?;

    // Find by multiple UUIDs
    let uuids = vec![
        step1.named_step_uuid,
        step2.named_step_uuid,
        step3.named_step_uuid,
    ];
    let found = NamedStep::find_by_uuids(&pool, &uuids).await?;
    assert_eq!(found.len(), 3);

    // Find by subset of UUIDs
    let subset = vec![step1.named_step_uuid, step3.named_step_uuid];
    let found_subset = NamedStep::find_by_uuids(&pool, &subset).await?;
    assert_eq!(found_subset.len(), 2);

    // Find by empty list
    let found_empty = NamedStep::find_by_uuids(&pool, &[]).await?;
    assert!(found_empty.is_empty());

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_named_step_list_all(pool: PgPool) -> sqlx::Result<()> {
    // Create several steps
    for i in 1..=5 {
        NamedStep::create(
            &pool,
            NewNamedStep {
                name: format!("list_step_{i:02}"),
                description: Some(format!("List step {i}")),
            },
        )
        .await?;
    }

    // List all with default pagination
    let all_steps = NamedStep::list_all(&pool, None, None).await?;
    assert!(all_steps.len() >= 5);

    // List with limit
    let limited = NamedStep::list_all(&pool, None, Some(3)).await?;
    assert_eq!(limited.len(), 3);

    // List with offset
    let offset_steps = NamedStep::list_all(&pool, Some(2), Some(2)).await?;
    assert_eq!(offset_steps.len(), 2);

    // Verify ordering is by name
    let ordered = NamedStep::list_all(&pool, None, Some(100)).await?;
    for window in ordered.windows(2) {
        assert!(
            window[0].name <= window[1].name,
            "Steps should be ordered by name"
        );
    }

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_named_step_find_or_create(pool: PgPool) -> sqlx::Result<()> {
    let step_name = "idempotent_step";

    // First call should create
    let new_step = NewNamedStep {
        name: step_name.to_string(),
        description: Some("Created by find_or_create".to_string()),
    };
    let created = NamedStep::find_or_create(&pool, new_step).await?;
    assert_eq!(created.name, step_name);

    // Second call with same name should find existing
    let new_step_again = NewNamedStep {
        name: step_name.to_string(),
        description: Some("Different description".to_string()),
    };
    let found = NamedStep::find_or_create(&pool, new_step_again).await?;
    assert_eq!(found.named_step_uuid, created.named_step_uuid);
    // Should keep original description since it found existing
    assert_eq!(
        found.description,
        Some("Created by find_or_create".to_string())
    );

    // Also test find_or_create_by_name
    let by_name = NamedStep::find_or_create_by_name(&pool, step_name).await?;
    assert_eq!(by_name.named_step_uuid, created.named_step_uuid);

    // Test find_or_create_by_name with a new name
    let new_by_name = NamedStep::find_or_create_by_name(&pool, "auto_created_step").await?;
    assert_eq!(new_by_name.name, "auto_created_step");
    assert!(new_by_name
        .description
        .as_ref()
        .unwrap()
        .contains("Auto-created"));

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_named_step_update(pool: PgPool) -> sqlx::Result<()> {
    // Create a step
    let step = NamedStep::create(
        &pool,
        NewNamedStep {
            name: "update_test_step".to_string(),
            description: Some("Original description".to_string()),
        },
    )
    .await?;

    // Update the step
    let updated_data = NewNamedStep {
        name: "updated_step_name".to_string(),
        description: Some("Updated description".to_string()),
    };
    let updated = NamedStep::update(&pool, step.named_step_uuid, updated_data).await?;
    assert!(updated.is_some());
    let updated = updated.unwrap();
    assert_eq!(updated.name, "updated_step_name");
    assert_eq!(updated.description, Some("Updated description".to_string()));
    assert_eq!(updated.named_step_uuid, step.named_step_uuid);

    // Verify the update persisted
    let found = NamedStep::find_by_uuid(&pool, step.named_step_uuid).await?;
    assert!(found.is_some());
    assert_eq!(found.unwrap().name, "updated_step_name");

    // Update a non-existent step
    let non_existent = NamedStep::update(
        &pool,
        uuid::Uuid::now_v7(),
        NewNamedStep {
            name: "ghost_step".to_string(),
            description: None,
        },
    )
    .await?;
    assert!(non_existent.is_none());

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_named_step_search(pool: PgPool) -> sqlx::Result<()> {
    // Create steps with varied names for search testing
    let names = vec![
        "payment_validate",
        "payment_process",
        "payment_refund",
        "email_send",
        "email_queue",
        "data_transform",
    ];
    for name in &names {
        NamedStep::create(
            &pool,
            NewNamedStep {
                name: name.to_string(),
                description: None,
            },
        )
        .await?;
    }

    // Search for "payment" - should find 3
    let payment_results = NamedStep::search_by_name(&pool, "payment", None).await?;
    assert_eq!(payment_results.len(), 3);

    // Search for "email" - should find 2
    let email_results = NamedStep::search_by_name(&pool, "email", None).await?;
    assert_eq!(email_results.len(), 2);

    // Search for "data" - should find 1
    let data_results = NamedStep::search_by_name(&pool, "data", None).await?;
    assert_eq!(data_results.len(), 1);

    // Search with limit
    let limited = NamedStep::search_by_name(&pool, "payment", Some(2)).await?;
    assert_eq!(limited.len(), 2);

    // Search for non-matching pattern
    let no_results = NamedStep::search_by_name(&pool, "zzz_nonexistent", None).await?;
    assert!(no_results.is_empty());

    // Case-insensitive search (ILIKE)
    let upper_results = NamedStep::search_by_name(&pool, "PAYMENT", None).await?;
    assert_eq!(upper_results.len(), 3);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_named_step_delete_nonexistent(pool: PgPool) -> sqlx::Result<()> {
    // Deleting a non-existent step should return false
    let deleted = NamedStep::delete(&pool, uuid::Uuid::now_v7()).await?;
    assert!(!deleted);

    // Create and delete, then try to delete again
    let step = NamedStep::create(
        &pool,
        NewNamedStep {
            name: "delete_twice_step".to_string(),
            description: None,
        },
    )
    .await?;

    let first_delete = NamedStep::delete(&pool, step.named_step_uuid).await?;
    assert!(first_delete);

    let second_delete = NamedStep::delete(&pool, step.named_step_uuid).await?;
    assert!(!second_delete);

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_named_step_find_or_create_by_name_simple(pool: PgPool) -> sqlx::Result<()> {
    // Test the simple alias version
    let step = NamedStep::find_or_create_by_name_simple(&pool, "simple_alias_step").await?;
    assert_eq!(step.name, "simple_alias_step");

    // Calling again should return the same step
    let same_step = NamedStep::find_or_create_by_name_simple(&pool, "simple_alias_step").await?;
    assert_eq!(same_step.named_step_uuid, step.named_step_uuid);

    Ok(())
}
