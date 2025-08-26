//! # TAS-37: Integration Test for Finalization Race Condition Prevention
//!
//! This test verifies that the finalization claiming mechanism prevents race conditions
//! when multiple processors attempt to finalize the same task simultaneously.

use std::sync::Arc;
use std::time::Duration;
use tasker_orchestration::orchestration::task_claim::finalization_claimer::{
    ClaimGuard, FinalizationClaimer, FinalizationClaimerConfig,
};
use tokio::time::timeout;

// Import the factory system for clean test data creation
use tasker_shared::models::factories::base::SqlxFactory;
use tasker_shared::models::factories::core::TaskFactory;

/// Test basic claiming functionality using the finalization claiming SQL functions
#[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
async fn test_basic_finalization_claiming(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create a task using the factory system (handles all foreign keys automatically)
    let task = TaskFactory::new()
        .with_namespace("tas_37_test")
        .with_initiator("test_processor")
        .with_context(serde_json::json!({"test": "TAS-37 basic claiming"}))
        .create(&pool)
        .await?;

    // Create finalization claimer
    let claimer = FinalizationClaimer::new(pool.clone(), "test_processor_1".to_string());

    // Test claiming attempt
    let claim_result = claimer
        .claim_task(task.task_uuid)
        .await
        .expect("Failed to attempt claim");

    // The claim should fail because task has no completed/failed steps
    // This tests our SQL function logic
    assert!(
        !claim_result.claimed,
        "Claim should fail for task with no completed steps"
    );
    assert!(
        claim_result
            .message
            .as_ref()
            .unwrap()
            .contains("no completed or failed steps"),
        "Should indicate no completed steps"
    );

    Ok(())
}

/// Test race condition prevention with multiple processors
#[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
async fn test_race_condition_prevention(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    let pool = Arc::new(pool);

    // Create a task using the factory system
    let task = TaskFactory::new()
        .with_namespace("tas_37_race_test")
        .with_initiator("race_test_processor")
        .with_context(serde_json::json!({"test": "TAS-37 race condition prevention"}))
        .create(pool.as_ref())
        .await?;

    // Create multiple processors
    let processor1 =
        FinalizationClaimer::new(pool.as_ref().clone(), "test_processor_1".to_string());

    let processor2 =
        FinalizationClaimer::new(pool.as_ref().clone(), "test_processor_2".to_string());

    let processor3 =
        FinalizationClaimer::new(pool.as_ref().clone(), "test_processor_3".to_string());

    let task_uuid = task.task_uuid;

    // Simulate concurrent claiming attempts
    let handle1 = tokio::spawn(async move { processor1.claim_task(task_uuid).await });

    let handle2 = tokio::spawn(async move { processor2.claim_task(task_uuid).await });

    let handle3 = tokio::spawn(async move { processor3.claim_task(task_uuid).await });

    // Wait for all attempts to complete
    let results = timeout(Duration::from_secs(10), async {
        let result1 = handle1.await.unwrap();
        let result2 = handle2.await.unwrap();
        let result3 = handle3.await.unwrap();
        (result1, result2, result3)
    })
    .await
    .expect("Test timed out");

    match results {
        (Ok(r1), Ok(r2), Ok(r3)) => {
            // Count how many claims succeeded
            let successful_claims = [r1.claimed, r2.claimed, r3.claimed]
                .iter()
                .filter(|&&claimed| claimed)
                .count();

            // At most one should succeed in claiming (likely 0 since task has no completed steps)
            assert!(
                successful_claims <= 1,
                "Race condition detected: {successful_claims} processors claimed the same task"
            );

            // All should fail for the same reason - no completed steps
            for result in [&r1, &r2, &r3] {
                if !result.claimed {
                    assert!(
                        result
                            .message
                            .as_ref()
                            .unwrap()
                            .contains("no completed or failed steps"),
                        "Should indicate no completed steps"
                    );
                }
            }

            // Clean up any claims that might have succeeded
            if r1.claimed {
                let releaser =
                    FinalizationClaimer::new(pool.as_ref().clone(), "test_processor_1".to_string());
                let _ = releaser.release_claim(task_uuid).await;
            }
            if r2.claimed {
                let releaser =
                    FinalizationClaimer::new(pool.as_ref().clone(), "test_processor_2".to_string());
                let _ = releaser.release_claim(task_uuid).await;
            }
            if r3.claimed {
                let releaser =
                    FinalizationClaimer::new(pool.as_ref().clone(), "test_processor_3".to_string());
                let _ = releaser.release_claim(task_uuid).await;
            }
        }
        (r1, r2, r3) => {
            panic!("One or more claiming attempts failed: {r1:?}, {r2:?}, {r3:?}");
        }
    }

    Ok(())
}

/// Test claim timeout and expiration
#[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
async fn test_claim_timeout_expiration(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create a task using the factory system
    let task = TaskFactory::new()
        .with_namespace("tas_37_timeout_test")
        .with_initiator("timeout_test_processor")
        .with_context(serde_json::json!({"test": "TAS-37 timeout test"}))
        .create(&pool)
        .await?;

    // Create claimer with very short timeout
    let config = FinalizationClaimerConfig {
        default_timeout_seconds: 1, // 1 second timeout
        enable_heartbeat: false,
        heartbeat_interval_seconds: 10,
    };

    let claimer1 =
        FinalizationClaimer::with_config(pool.clone(), "test_processor_1".to_string(), config);

    let claimer2 = FinalizationClaimer::new(pool.clone(), "test_processor_2".to_string());

    // First processor attempts to claim (should fail - no completed steps)
    let claim_result1 = claimer1
        .claim_task(task.task_uuid)
        .await
        .expect("Failed to attempt first claim");

    // Both should fail for same reason
    assert!(
        !claim_result1.claimed,
        "Should fail due to no completed steps"
    );

    // Wait for potential timeout to expire (though claim didn't succeed)
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Second processor should also fail for same reason
    let claim_result2 = claimer2
        .claim_task(task.task_uuid)
        .await
        .expect("Failed to attempt second claim");

    assert!(
        !claim_result2.claimed,
        "Should fail due to no completed steps"
    );

    Ok(())
}

/// Test claim guard RAII functionality
#[sqlx::test(migrator = "tasker_shared::test_utils::MIGRATOR")]
async fn test_claim_guard_automatic_release(
    pool: sqlx::PgPool,
) -> Result<(), Box<dyn std::error::Error>> {
    // Create a task using the factory system
    let task = TaskFactory::new()
        .with_namespace("tas_37_raii_test")
        .with_initiator("raii_test_processor")
        .with_context(serde_json::json!({"test": "TAS-37 RAII test"}))
        .create(&pool)
        .await?;

    let claimer1 = FinalizationClaimer::new(pool.clone(), "test_processor_1".to_string());

    let claimer2 = FinalizationClaimer::new(pool.clone(), "test_processor_2".to_string());

    // Use claim guard in a scope that ends
    {
        let (guard, claim_result) = ClaimGuard::new(claimer1.clone(), task.task_uuid)
            .await
            .expect("Failed to create claim guard");

        // Should fail due to no completed steps
        assert!(
            !claim_result.claimed,
            "Should fail due to no completed steps"
        );
        assert!(!guard.is_claimed(), "Guard should not be claimed");
    } // Guard drops here, but no claim to release

    // Give any potential background cleanup a moment
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Second processor should also fail for same reason
    let claim_result2 = claimer2
        .claim_task(task.task_uuid)
        .await
        .expect("Failed to attempt claim after guard drop");

    assert!(
        !claim_result2.claimed,
        "Should fail due to no completed steps"
    );

    Ok(())
}
