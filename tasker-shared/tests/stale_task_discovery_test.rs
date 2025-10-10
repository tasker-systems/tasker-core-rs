//! # TAS-48: Stale Task Discovery SQL Function Tests
//!
//! Tests for get_next_ready_tasks() priority decay and staleness exclusion.
//! Validates that fresh tasks have higher computed priority than aging tasks.

use anyhow::Result;
use sqlx::PgPool;
use tasker_shared::models::factories::base::{SqlxFactory, StateFactory};
use tasker_shared::models::factories::core::TaskFactory;
use tasker_shared::models::Task;

/// Helper to create a task with initial pending state and specific priority
async fn create_test_task_with_priority(pool: &PgPool, priority: i32) -> Result<Task> {
    let task = TaskFactory::new()
        .with_initial_state("pending")
        .create(pool)
        .await?;

    // Set the priority manually
    sqlx::query!(
        "UPDATE tasker_tasks SET priority = $1 WHERE task_uuid = $2",
        priority,
        task.task_uuid
    )
    .execute(pool)
    .await?;

    Ok(task)
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_priority_decay_ordering(pool: PgPool) -> sqlx::Result<()> {
    tracing::info!("🧪 TEST: Priority decay ensures fresh > aging");

    // Create fresh task with priority 5
    let fresh_task = create_test_task_with_priority(&pool, 5)
        .await
        .map_err(|e| sqlx::Error::Protocol(format!("{e}")))?;

    // Create aging task with same priority 5
    let aging_task = create_test_task_with_priority(&pool, 5)
        .await
        .map_err(|e| sqlx::Error::Protocol(format!("{e}")))?;

    // Age the aging task to 6 hours old
    sqlx::query!(
        "UPDATE tasker_task_transitions
         SET created_at = NOW() - INTERVAL '6 hours'
         WHERE task_uuid = $1 AND most_recent = true",
        aging_task.task_uuid
    )
    .execute(&pool)
    .await?;

    // Get discovered tasks with computed priority
    let discovered =
        sqlx::query!("SELECT task_uuid, computed_priority FROM get_next_ready_tasks(10)")
            .fetch_all(&pool)
            .await?;

    let fresh_priority = discovered
        .iter()
        .find(|r| r.task_uuid == Some(fresh_task.task_uuid))
        .map(|r| r.computed_priority.clone().unwrap())
        .expect("Fresh task should be discovered");

    let aging_priority = discovered
        .iter()
        .find(|r| r.task_uuid == Some(aging_task.task_uuid))
        .map(|r| r.computed_priority.clone().unwrap())
        .expect("Aging task should be discovered");

    // Validate: Fresh priority > Aging priority (due to exponential decay)
    assert!(
        fresh_priority > aging_priority,
        "Fresh priority ({}) should be > aging priority ({}) due to exponential decay",
        fresh_priority,
        aging_priority
    );

    tracing::info!(
        "✅ Priority decay working: fresh={}, aging={}",
        fresh_priority,
        aging_priority
    );
    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_normal_priority_ordering_unchanged(pool: PgPool) -> sqlx::Result<()> {
    tracing::info!("🧪 TEST: Normal priority ordering preserved for fresh tasks");

    // Create high priority task (priority 10)
    let high_task = create_test_task_with_priority(&pool, 10)
        .await
        .map_err(|e| sqlx::Error::Protocol(format!("{e}")))?;

    // Create low priority task (priority 2)
    let low_task = create_test_task_with_priority(&pool, 2)
        .await
        .map_err(|e| sqlx::Error::Protocol(format!("{e}")))?;

    // Get discovered tasks (should be ordered by computed priority DESC)
    let discovered = sqlx::query!(
        "SELECT task_uuid, computed_priority
         FROM get_next_ready_tasks(10)
         ORDER BY computed_priority DESC"
    )
    .fetch_all(&pool)
    .await?;

    // Validate ordering: high priority should come first
    assert!(!discovered.is_empty(), "Should have discovered tasks");
    assert_eq!(
        discovered[0].task_uuid,
        Some(high_task.task_uuid),
        "High priority task should be discovered first"
    );
    assert_eq!(
        discovered[1].task_uuid,
        Some(low_task.task_uuid),
        "Low priority task should be discovered second"
    );

    tracing::info!("✅ Normal priority ordering preserved");
    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_fresh_tasks_always_discoverable(pool: PgPool) -> sqlx::Result<()> {
    tracing::info!("🧪 TEST: Fresh tasks are always discoverable");

    // Create 10 tasks with varying priorities
    for i in 0..10 {
        create_test_task_with_priority(&pool, i)
            .await
            .map_err(|e| sqlx::Error::Protocol(format!("{e}")))?;
    }

    // Discovery should find tasks
    let discovered = sqlx::query!("SELECT task_uuid FROM get_next_ready_tasks(10)")
        .fetch_all(&pool)
        .await?;

    let discovered_count = discovered.len();

    assert!(
        discovered_count > 0,
        "Should discover at least some tasks (found {})",
        discovered_count
    );

    tracing::info!(
        "✅ Discovered {} fresh tasks out of 10 created",
        discovered_count
    );
    Ok(())
}

#[sqlx::test(migrator = "tasker_shared::database::migrator::MIGRATOR")]
async fn test_priority_decay_over_time(pool: PgPool) -> sqlx::Result<()> {
    tracing::info!("🧪 TEST: Priority decays exponentially over time");

    // Create 3 tasks with same base priority but different ages
    let very_fresh_task = create_test_task_with_priority(&pool, 5)
        .await
        .map_err(|e| sqlx::Error::Protocol(format!("{e}")))?;

    let slightly_aged_task = create_test_task_with_priority(&pool, 5)
        .await
        .map_err(|e| sqlx::Error::Protocol(format!("{e}")))?;

    sqlx::query!(
        "UPDATE tasker_task_transitions
         SET created_at = NOW() - INTERVAL '1 hour'
         WHERE task_uuid = $1 AND most_recent = true",
        slightly_aged_task.task_uuid
    )
    .execute(&pool)
    .await?;

    let very_aged_task = create_test_task_with_priority(&pool, 5)
        .await
        .map_err(|e| sqlx::Error::Protocol(format!("{e}")))?;

    sqlx::query!(
        "UPDATE tasker_task_transitions
         SET created_at = NOW() - INTERVAL '12 hours'
         WHERE task_uuid = $1 AND most_recent = true",
        very_aged_task.task_uuid
    )
    .execute(&pool)
    .await?;

    // Get discovered tasks with computed priority
    let discovered =
        sqlx::query!("SELECT task_uuid, computed_priority FROM get_next_ready_tasks(10)")
            .fetch_all(&pool)
            .await?;

    let very_fresh_priority = discovered
        .iter()
        .find(|r| r.task_uuid == Some(very_fresh_task.task_uuid))
        .map(|r| r.computed_priority.clone().unwrap())
        .expect("Very fresh task should be discovered");

    let slightly_aged_priority = discovered
        .iter()
        .find(|r| r.task_uuid == Some(slightly_aged_task.task_uuid))
        .map(|r| r.computed_priority.clone().unwrap())
        .expect("Slightly aged task should be discovered");

    let very_aged_priority = discovered
        .iter()
        .find(|r| r.task_uuid == Some(very_aged_task.task_uuid))
        .map(|r| r.computed_priority.clone().unwrap())
        .expect("Very aged task should be discovered");

    // Validate: very_fresh > slightly_aged > very_aged (exponential decay)
    assert!(
        very_fresh_priority > slightly_aged_priority,
        "Very fresh ({}) should be > slightly aged ({})",
        very_fresh_priority,
        slightly_aged_priority
    );
    assert!(
        slightly_aged_priority > very_aged_priority,
        "Slightly aged ({}) should be > very aged ({})",
        slightly_aged_priority,
        very_aged_priority
    );

    tracing::info!(
        "✅ Priority decay verified: very_fresh={}, slightly_aged={}, very_aged={}",
        very_fresh_priority,
        slightly_aged_priority,
        very_aged_priority
    );
    Ok(())
}
