/// Ignore state machine tests that are deferred architectural dependencies
#[macro_export]
macro_rules! ignore_state_machine_tests {
    () => {
        #[ignore = "State machine tests deferred as architectural dependency"]
    };
}

/// Generate unique test names to avoid database conflicts
#[macro_export]
macro_rules! unique_test_name {
    ($prefix:expr) => {
        format!("{}_{}", $prefix, chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0))
    };
}

/// Test with isolated transaction that rolls back automatically
#[macro_export]
macro_rules! test_with_rollback {
    ($test_name:ident, $($body:tt)*) => {
        #[tokio::test]
        async fn $test_name() {
            use crate::test_helpers::{TestSetup, TestTransaction};
            
            let setup = TestSetup::new().await;
            let mut tx = TestTransaction::new(&setup.pool).await;
            
            let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = async {
                $($body)*
                Ok(())
            }.await;
            
            tx.rollback().await;
            
            if let Err(e) = result {
                panic!("Test failed: {:?}", e);
            }
        }
    };
}

/// Test with seed data and transaction rollback
#[macro_export]
macro_rules! test_with_seed_and_rollback {
    ($test_name:ident, $($body:tt)*) => {
        #[tokio::test]
        async fn $test_name() {
            use crate::test_helpers::{TestSetup, TestTransaction};
            
            let setup = TestSetup::new().await;
            let seed_data = setup.seed_basic_data().await;
            let mut tx = TestTransaction::new(&setup.pool).await;
            let pool = tx.pool();
            
            let result: Result<(), Box<dyn std::error::Error + Send + Sync>> = async {
                $($body)*
                Ok(())
            }.await;
            
            tx.rollback().await;
            
            if let Err(e) = result {
                panic!("Test failed: {:?}", e);
            }
        }
    };
}