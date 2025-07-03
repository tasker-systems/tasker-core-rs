pub mod simple_factories;
pub mod macros;

use sqlx::{PgPool, Postgres, Transaction};
use std::sync::Once;
use tokio::sync::Mutex;
use std::sync::Arc;

static INIT: Once = Once::new();
static mut TEST_DB_POOL: Option<Arc<Mutex<PgPool>>> = None;

/// Initialize test database with .env.test configuration
pub async fn init_test_db() -> PgPool {
    unsafe {
        if let Some(pool) = &TEST_DB_POOL {
            return pool.lock().await.clone();
        }
    }

    INIT.call_once(|| {
        // Load .env.test file
        if std::path::Path::new(".env.test").exists() {
            dotenv::from_filename(".env.test").ok();
        }
    });

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:password@localhost:5432/tasker_rust_test".to_string());

    // Create test database if it doesn't exist
    create_test_database(&database_url).await;

    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to connect to test database");

    unsafe {
        TEST_DB_POOL = Some(Arc::new(Mutex::new(pool.clone())));
    }

    pool
}

/// Create test database if it doesn't exist
async fn create_test_database(database_url: &str) {
    // Parse the database URL to get connection info without the database name
    let url = url::Url::parse(database_url).expect("Invalid database URL");
    let db_name = url.path().trim_start_matches('/');
    
    // Connect to postgres database to create our test database
    let mut base_url = url.clone();
    base_url.set_path("/postgres");
    
    if let Ok(pool) = PgPool::connect(base_url.as_str()).await {
        // Try to create the database, ignore error if it already exists
        let create_query = format!("CREATE DATABASE {}", db_name);
        sqlx::query(&create_query).execute(&pool).await.ok();
        pool.close().await;
    }
}

/// Test transaction wrapper for isolated tests
pub struct TestTransaction<'a> {
    pub tx: Transaction<'a, Postgres>,
}

impl<'a> TestTransaction<'a> {
    pub async fn new(pool: &'a PgPool) -> Self {
        let tx = pool.begin().await.expect("Failed to start transaction");
        Self { tx }
    }

    pub fn pool(&mut self) -> &mut Transaction<'a, Postgres> {
        &mut self.tx
    }

    pub async fn rollback(self) {
        self.tx.rollback().await.expect("Failed to rollback transaction");
    }
}

/// Test setup for model tests - creates base data that can be shared across tests
pub struct TestSetup {
    pub pool: PgPool,
}

impl TestSetup {
    pub async fn new() -> Self {
        let pool = init_test_db().await;
        Self { pool }
    }

    /// Clean up all test data (for use between test suites)
    pub async fn cleanup(&self) {
        let cleanup_queries = vec![
            "DELETE FROM tasker_workflow_step_transitions",
            "DELETE FROM tasker_task_transitions", 
            "DELETE FROM tasker_workflow_step_edges",
            "DELETE FROM tasker_workflow_steps",
            "DELETE FROM tasker_task_annotations",
            "DELETE FROM tasker_task_diagrams",
            "DELETE FROM tasker_task_execution_contexts",
            "DELETE FROM tasker_step_readiness_statuses",
            "DELETE FROM tasker_named_tasks_named_steps",
            "DELETE FROM tasker_dependent_system_object_maps",
            "DELETE FROM tasker_tasks",
            "DELETE FROM tasker_named_tasks",
            "DELETE FROM tasker_named_steps",
            "DELETE FROM tasker_annotation_types",
            "DELETE FROM tasker_dependent_systems",
            "DELETE FROM tasker_task_namespaces",
        ];

        for query in cleanup_queries {
            sqlx::query(query).execute(&self.pool).await.ok();
        }
    }

    /// Create seed data that can be shared across tests in a suite
    pub async fn seed_basic_data(&self) -> SeedData {
        let namespace = tasker_core::models::task_namespace::TaskNamespace::create(
            &self.pool,
            tasker_core::models::task_namespace::NewTaskNamespace {
                name: unique_name("test_seed_namespace"),
                description: Some("Seed namespace for testing".to_string()),
            },
        ).await.expect("Failed to create seed namespace");

        let dependent_system = tasker_core::models::dependent_system::DependentSystem::create(
            &self.pool,
            tasker_core::models::dependent_system::NewDependentSystem {
                name: unique_name("test_seed_system"),
                description: Some("Seed system for testing".to_string()),
            },
        ).await.expect("Failed to create seed system");

        let annotation_type = tasker_core::models::annotation_type::AnnotationType::create(
            &self.pool,
            tasker_core::models::annotation_type::NewAnnotationType {
                name: unique_name("test_seed_annotation"),
                description: Some("Seed annotation type for testing".to_string()),
            },
        ).await.expect("Failed to create seed annotation type");

        SeedData {
            namespace,
            dependent_system,
            annotation_type,
        }
    }
}

/// Shared seed data for tests
pub struct SeedData {
    pub namespace: tasker_core::models::task_namespace::TaskNamespace,
    pub dependent_system: tasker_core::models::dependent_system::DependentSystem,
    pub annotation_type: tasker_core::models::annotation_type::AnnotationType,
}

/// Generate unique names for test data to avoid conflicts
pub fn unique_name(prefix: &str) -> String {
    format!("{}_{}", prefix, chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0))
}

/// Test runner macro that handles setup, transaction, and cleanup
#[macro_export]
macro_rules! test_with_transaction {
    ($test_name:ident, $test_body:expr) => {
        #[tokio::test]
        async fn $test_name() {
            let setup = TestSetup::new().await;
            let mut tx = TestTransaction::new(&setup.pool).await;
            
            let result = {
                $test_body(&mut tx, &setup).await
            };
            
            tx.rollback().await;
            
            if let Err(e) = result {
                panic!("Test failed: {:?}", e);
            }
        }
    };
}

/// Test runner macro for tests that need seed data
#[macro_export]
macro_rules! test_with_seed_data {
    ($test_name:ident, $test_body:expr) => {
        #[tokio::test]
        async fn $test_name() {
            let setup = TestSetup::new().await;
            let seed_data = setup.seed_basic_data().await;
            let mut tx = TestTransaction::new(&setup.pool).await;
            
            let result = {
                $test_body(&mut tx, &setup, &seed_data).await
            };
            
            tx.rollback().await;
            
            if let Err(e) = result {
                panic!("Test failed: {:?}", e);
            }
        }
    };
}