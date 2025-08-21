//! Annotation Type Tests
//!
//! Using SQLx native testing for automatic database isolation

use sqlx::PgPool;
use tasker_shared::models::annotation_type::{AnnotationType, NewAnnotationType};

#[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
async fn test_annotation_type_crud(pool: PgPool) -> sqlx::Result<()> {
    // Test annotation type creation
    let new_type = NewAnnotationType {
        name: "bug_report".to_string(),
        description: Some("Bug report annotations".to_string()),
    };

    let created = AnnotationType::create(&pool, new_type).await?;
    assert_eq!(created.name, "bug_report");
    assert_eq!(
        created.description,
        Some("Bug report annotations".to_string())
    );

    // Test find by ID
    let found = AnnotationType::find_by_id(&pool, created.annotation_type_id)
        .await?
        .expect("Annotation type not found");
    assert_eq!(found.annotation_type_id, created.annotation_type_id);

    // Test find by name
    let found_by_name = AnnotationType::find_by_name(&pool, "bug_report")
        .await?
        .expect("Annotation type not found");
    assert_eq!(found_by_name.annotation_type_id, created.annotation_type_id);

    // Test find_or_create_by_name (should find existing)
    let found_or_created = AnnotationType::find_or_create_by_name(&pool, "bug_report").await?;
    assert_eq!(
        found_or_created.annotation_type_id,
        created.annotation_type_id
    );

    // Test find_or_create_by_name (should create new)
    let new_type = AnnotationType::find_or_create_by_name(&pool, "feature_request").await?;
    assert_eq!(new_type.name, "feature_request");

    // Test all
    let all_types = AnnotationType::all(&pool).await?;
    assert!(all_types.len() >= 2);

    // Test with_usage_stats
    let with_stats = AnnotationType::with_usage_stats(&pool).await?;
    assert!(with_stats.len() >= 2);

    // Test search
    let search_results = AnnotationType::search(&pool, "bug_report").await?;
    assert!(!search_results.is_empty());

    // Test recent
    let recent = AnnotationType::recent(&pool, Some(5)).await?;
    assert!(!recent.is_empty());

    // Test update
    let mut annotation_type = created.clone();
    annotation_type
        .update(
            &pool,
            Some("updated_bug_report"),
            Some("Updated description"),
        )
        .await?;
    assert_eq!(annotation_type.name, "updated_bug_report");

    // Test is_in_use
    let in_use = annotation_type.is_in_use(&pool).await?;
    assert!(!in_use); // Should not be in use yet

    // Test annotation_count
    let count = annotation_type.annotation_count(&pool).await?;
    assert_eq!(count, 0);

    // No cleanup needed - SQLx will roll back the test transaction automatically!
    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
async fn test_annotation_type_constraints(pool: PgPool) -> sqlx::Result<()> {
    // Create first annotation type
    let new_type = NewAnnotationType {
        name: "unique_test".to_string(),
        description: None,
    };
    let _created = AnnotationType::create(&pool, new_type).await?;

    // Try to create duplicate - should fail
    let duplicate_type = NewAnnotationType {
        name: "unique_test".to_string(),
        description: Some("This should fail".to_string()),
    };

    let result = AnnotationType::create(&pool, duplicate_type).await;
    assert!(result.is_err(), "Should not allow duplicate names");

    Ok(())
}

// Example of parametrized testing with rstest
// Note: rstest and sqlx::test can't be combined directly,
// so we create separate test functions
#[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
async fn test_annotation_type_bug_report(pool: PgPool) -> sqlx::Result<()> {
    let new_type = NewAnnotationType {
        name: "bug_report".to_string(),
        description: Some("Bug tracking annotations".to_string()),
    };

    let created = AnnotationType::create(&pool, new_type).await?;
    assert_eq!(created.name, "bug_report");
    assert_eq!(
        created.description,
        Some("Bug tracking annotations".to_string())
    );

    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
async fn test_annotation_type_feature_request(pool: PgPool) -> sqlx::Result<()> {
    let new_type = NewAnnotationType {
        name: "feature_request".to_string(),
        description: Some("Feature request annotations".to_string()),
    };

    let created = AnnotationType::create(&pool, new_type).await?;
    assert_eq!(created.name, "feature_request");
    assert_eq!(
        created.description,
        Some("Feature request annotations".to_string())
    );

    Ok(())
}
