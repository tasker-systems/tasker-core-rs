//! Handle Lifecycle Management Tests
//!
//! Tests for the enhanced handle validation and renewal functionality

use std::time::{Duration, SystemTime};
use tasker_core::ffi::shared::handles::SharedOrchestrationHandle;

#[tokio::test]
async fn test_handle_lifecycle_methods() {
    // Create a fresh handle
    let handle = SharedOrchestrationHandle::get_global();

    // Test basic validation
    assert!(handle.validate().is_ok(), "Fresh handle should be valid");
    assert!(!handle.is_expired(), "Fresh handle should not be expired");

    // Test expiry information
    let expires_in = handle.expires_in();
    assert!(
        expires_in.is_some(),
        "Fresh handle should have expiry duration"
    );
    assert!(
        expires_in.unwrap().as_secs() > 0,
        "Fresh handle should have positive expiry time"
    );

    // Test expiry time
    let expires_at = handle.expires_at();
    let now = SystemTime::now();
    assert!(expires_at > now, "Handle expiry should be in the future");

    // Test handle info includes expiry details
    let info = handle.info();
    assert_eq!(
        info.status, "active",
        "Fresh handle should have active status"
    );
    assert!(
        info.expires_in_seconds > 0,
        "Handle info should show positive expiry time"
    );
    assert!(
        info.expires_at > 0,
        "Handle info should have expiry timestamp"
    );

    println!("✅ Handle lifecycle methods working correctly");
    println!("   Handle ID: {}", info.handle_id);
    println!("   Expires in: {} seconds", info.expires_in_seconds);
    println!("   Status: {}", info.status);
}

#[tokio::test]
async fn test_handle_refresh() {
    // Get initial handle
    let original_handle = SharedOrchestrationHandle::get_global();
    let original_id = original_handle.handle_id.clone();

    tokio::time::sleep(Duration::from_millis(10)).await;
    // Create a refreshed handle
    let refreshed_handle =
        SharedOrchestrationHandle::refresh().expect("Handle refresh should succeed");
    let refreshed_id = refreshed_handle.handle_id.clone();

    // Verify they are different handles
    assert_ne!(
        original_id, refreshed_id,
        "Refreshed handle should have different ID"
    );

    // Both should be valid
    assert!(
        original_handle.validate().is_ok(),
        "Original handle should still be valid"
    );
    assert!(
        refreshed_handle.validate().is_ok(),
        "Refreshed handle should be valid"
    );

    println!("✅ Handle refresh working correctly");
    println!("   Original ID: {original_id}");
    println!("   Refreshed ID: {refreshed_id}");
}

#[tokio::test]
async fn test_validate_or_refresh_with_valid_handle() {
    // Get a fresh handle
    let handle = SharedOrchestrationHandle::get_global();

    // Test validate_or_refresh with a valid handle
    let result = handle
        .validate_or_refresh()
        .expect("validate_or_refresh should succeed with valid handle");

    // Should return a handle (could be same or different Arc)
    assert!(result.validate().is_ok(), "Returned handle should be valid");
    assert!(
        !result.is_expired(),
        "Returned handle should not be expired"
    );

    println!("✅ validate_or_refresh with valid handle working correctly");
    println!("   Returned handle ID: {}", result.handle_id);
}

#[test]
fn test_validate_or_refresh_with_expired_handle() {
    // Create an expired handle
    let orchestration_system =
        tasker_core::ffi::shared::orchestration_system::initialize_unified_orchestration_system();

    let expired_handle = SharedOrchestrationHandle {
        orchestration_system,
        handle_id: "test_expired_auto_refresh".to_string(),
        // Set created_at to 3 hours ago (past the 2-hour limit)
        created_at: std::time::SystemTime::now() - std::time::Duration::from_secs(3 * 3600),
    };

    // Verify it's expired
    assert!(expired_handle.is_expired(), "Test handle should be expired");
    assert!(
        expired_handle.validate().is_err(),
        "Expired handle validation should fail"
    );

    // Test validate_or_refresh with expired handle
    let result = expired_handle
        .validate_or_refresh()
        .expect("validate_or_refresh should succeed even with expired handle");

    // Should return a fresh, valid handle
    assert!(
        result.validate().is_ok(),
        "Auto-refreshed handle should be valid"
    );
    assert!(
        !result.is_expired(),
        "Auto-refreshed handle should not be expired"
    );
    assert_ne!(
        expired_handle.handle_id, result.handle_id,
        "Auto-refreshed handle should have different ID"
    );

    println!("✅ validate_or_refresh with expired handle working correctly");
    println!("   Original expired ID: {}", expired_handle.handle_id);
    println!("   Auto-refreshed ID: {}", result.handle_id);
}

#[test]
fn test_handle_expiry_validation_message() {
    // Create a handle with a past timestamp to simulate expiry
    let orchestration_system =
        tasker_core::ffi::shared::orchestration_system::initialize_unified_orchestration_system();

    let expired_handle = SharedOrchestrationHandle {
        orchestration_system,
        handle_id: "test_expired".to_string(),
        // Set created_at to 3 hours ago (past the 2-hour limit)
        created_at: SystemTime::now() - Duration::from_secs(3 * 3600),
    };

    // Test expiry detection
    assert!(
        expired_handle.is_expired(),
        "Handle should be detected as expired"
    );

    // Test validation error message
    let validation_result = expired_handle.validate();
    assert!(
        validation_result.is_err(),
        "Expired handle validation should fail"
    );

    let error_message = format!("{}", validation_result.unwrap_err());
    assert!(
        error_message.contains("expired"),
        "Error should mention expiry"
    );
    assert!(
        error_message.contains("refresh()"),
        "Error should suggest refresh method"
    );

    println!("✅ Handle expiry validation and error messages working correctly");
    println!("   Error message: {error_message}");
}
