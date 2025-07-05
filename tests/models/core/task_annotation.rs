use sqlx::PgPool;
use tasker_core::models::task_annotation::{NewTaskAnnotation, TaskAnnotation};

#[sqlx::test]
async fn test_task_annotation_crud(pool: PgPool) -> sqlx::Result<()> {
    // Create test dependencies
    let namespace = tasker_core::models::task_namespace::TaskNamespace::create(
        &pool,
        tasker_core::models::task_namespace::NewTaskNamespace {
            name: "test_namespace_annotation_crud".to_string(),
            description: None,
        },
    )
    .await?;

    let named_task = tasker_core::models::named_task::NamedTask::create(
        &pool,
        tasker_core::models::named_task::NewNamedTask {
            name: "test_task_annotation_crud".to_string(),
            version: Some("1.0.0".to_string()),
            description: None,
            task_namespace_id: namespace.task_namespace_id as i64,
            configuration: None,
        },
    )
    .await?;

    let task = tasker_core::models::task::Task::create(
        &pool,
        tasker_core::models::task::NewTask {
            named_task_id: named_task.named_task_id,
            identity_hash: "test_hash_annotation_crud".to_string(),
            requested_at: None,
            initiator: None,
            source_system: None,
            reason: None,
            bypass_steps: None,
            tags: None,
            context: None,
        },
    )
    .await?;

    let annotation_type = tasker_core::models::annotation_type::AnnotationType::create(
        &pool,
        tasker_core::models::annotation_type::NewAnnotationType {
            name: "test_type_annotation_crud".to_string(),
            description: Some("Test annotation type".to_string()),
        },
    )
    .await?;

    // Test creation
    let new_annotation = NewTaskAnnotation {
        task_id: task.task_id,
        annotation_type_id: annotation_type.annotation_type_id,
        annotation: serde_json::json!({
            "key": "test_value",
            "priority": "high",
            "metadata": {
                "source": "test"
            }
        }),
    };

    let annotation = TaskAnnotation::create(&pool, new_annotation.clone()).await?;
    assert_eq!(annotation.task_id, task.task_id);
    assert_eq!(
        annotation.annotation_type_id,
        annotation_type.annotation_type_id
    );

    // Test find by ID
    let found = TaskAnnotation::find_by_id(&pool, annotation.task_annotation_id).await?;
    assert!(found.is_some());
    assert_eq!(found.unwrap().annotation["key"], "test_value");

    // Test find by task
    let task_annotations = TaskAnnotation::find_by_task(&pool, task.task_id).await?;
    assert!(!task_annotations.is_empty());

    // Test JSON search
    let json_search = serde_json::json!({"key": "test_value"});
    let search_results =
        TaskAnnotation::search_containing_json(&pool, &json_search, Some(10)).await?;
    assert!(!search_results.is_empty());

    // Test annotations with types
    let with_types = TaskAnnotation::find_with_types(&pool, Some(task.task_id), Some(10)).await?;
    assert!(!with_types.is_empty());
    assert_eq!(with_types[0].annotation_type_name, annotation_type.name);

    // Cleanup
    let deleted = TaskAnnotation::delete(&pool, annotation.task_annotation_id).await?;
    assert!(deleted);
    Ok(())
}

#[sqlx::test]
async fn test_json_operations(pool: PgPool) -> sqlx::Result<()> {
    // Create minimal test data
    let namespace = tasker_core::models::task_namespace::TaskNamespace::create(
        &pool,
        tasker_core::models::task_namespace::NewTaskNamespace {
            name: "test_namespace_json_ops".to_string(),
            description: None,
        },
    )
    .await?;

    let named_task = tasker_core::models::named_task::NamedTask::create(
        &pool,
        tasker_core::models::named_task::NewNamedTask {
            name: "test_task_json_ops".to_string(),
            version: Some("1.0.0".to_string()),
            description: None,
            task_namespace_id: namespace.task_namespace_id as i64,
            configuration: None,
        },
    )
    .await?;

    let task = tasker_core::models::task::Task::create(
        &pool,
        tasker_core::models::task::NewTask {
            named_task_id: named_task.named_task_id,
            identity_hash: "test_hash_json_ops".to_string(),
            requested_at: None,
            initiator: None,
            source_system: None,
            reason: None,
            bypass_steps: None,
            tags: None,
            context: None,
        },
    )
    .await?;

    let annotation_type = tasker_core::models::annotation_type::AnnotationType::create(
        &pool,
        tasker_core::models::annotation_type::NewAnnotationType {
            name: "test_type_json_ops".to_string(),
            description: None,
        },
    )
    .await?;

    // Create annotation with complex JSON structure
    let complex_json = serde_json::json!({
        "config": {
            "retry_count": 3,
            "timeout": 30
        },
        "tags": ["urgent", "customer-facing"],
        "owner": "test-team"
    });

    let new_annotation = NewTaskAnnotation {
        task_id: task.task_id,
        annotation_type_id: annotation_type.annotation_type_id,
        annotation: complex_json.clone(),
    };

    let annotation = TaskAnnotation::create(&pool, new_annotation).await?;

    // Test JSON containment search
    let search_subset = serde_json::json!({
        "config": {
            "retry_count": 3
        }
    });

    let containment_results =
        TaskAnnotation::search_containing_json(&pool, &search_subset, Some(5)).await?;
    assert!(
        !containment_results.is_empty(),
        "Should find annotation containing search subset"
    );

    // Cleanup
    TaskAnnotation::delete(&pool, annotation.task_annotation_id).await?;
    Ok(())
}
