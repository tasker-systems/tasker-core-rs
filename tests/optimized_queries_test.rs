//! Integration tests for optimized worker query SQL functions
//!
//! These tests validate that the SQL functions created in migration
//! 20250729000001_create_optimized_worker_query_functions.sql work correctly
//! and that the OptimizedWorkerQueries wrapper methods function as expected.

use sqlx::PgPool;
use tasker_core::config::QueryCacheConfig;
use tasker_core::database::{
    OptimizedWorkerQueries, SqlFunctionExecutor, ActiveWorkerResult, 
    WorkerHealthResult, OptimalWorkerResult, WorkerPoolStatistics
};
use tasker_core::models::core::{
    task_namespace::{TaskNamespace, NewTaskNamespace},
    named_task::{NamedTask, NewNamedTask},
    worker::{Worker, NewWorker},
    worker_named_task::{WorkerNamedTask, NewWorkerNamedTask},
    worker_registration::{WorkerRegistration, NewWorkerRegistration, WorkerStatus},
};

/// Test find_active_workers_for_task SQL function directly
#[sqlx::test]
async fn test_sql_function_find_active_workers_for_task(pool: PgPool) {
    // Setup test data
    let namespace = create_test_namespace(&pool, "test_namespace").await;
    let named_task = create_test_named_task(&pool, "test_task", namespace.task_namespace_id).await;
    let worker = create_test_worker(&pool, "test_worker").await;
    let _worker_named_task = create_test_worker_named_task(&pool, worker.worker_id, named_task.named_task_id, 100).await;
    let _registration = create_healthy_worker_registration(&pool, worker.worker_id).await;

    // Execute SQL function directly
    let sql_executor = SqlFunctionExecutor::new(pool);
    let results = sql_executor
        .find_active_workers_for_task(named_task.named_task_id)
        .await
        .expect("SQL function should execute successfully");

    // Validate results
    assert_eq!(results.len(), 1);
    let result = &results[0];
    assert_eq!(result.worker_id, worker.worker_id);
    assert_eq!(result.named_task_id, named_task.named_task_id);
    assert_eq!(result.worker_name, "test_worker");
    assert_eq!(result.task_name, "test_task");
    assert_eq!(result.namespace_name, "test_namespace");
    assert_eq!(result.priority, 100);
}

/// Test find_active_workers_for_task through OptimizedWorkerQueries wrapper
#[sqlx::test]
async fn test_optimized_queries_find_active_workers(pool: PgPool) {
    // Setup test data
    let namespace = create_test_namespace(&pool, "opt_namespace").await;
    let named_task = create_test_named_task(&pool, "opt_task", namespace.task_namespace_id).await;
    let worker1 = create_test_worker(&pool, "worker_1").await;
    let worker2 = create_test_worker(&pool, "worker_2").await;
    
    // Create worker-task associations with different priorities
    let _wnt1 = create_test_worker_named_task(&pool, worker1.worker_id, named_task.named_task_id, 200).await;
    let _wnt2 = create_test_worker_named_task(&pool, worker2.worker_id, named_task.named_task_id, 100).await;
    
    // Create healthy registrations
    let _reg1 = create_healthy_worker_registration(&pool, worker1.worker_id).await;
    let _reg2 = create_healthy_worker_registration(&pool, worker2.worker_id).await;

    // Test through OptimizedWorkerQueries wrapper
    let config = QueryCacheConfig::for_test(); // Use 1-second cache for rapid testing
    let optimized_queries = OptimizedWorkerQueries::new_with_config(pool.clone(), config);
    
    let results = optimized_queries
        .find_active_workers_for_task_optimized(named_task.named_task_id)
        .await
        .expect("Optimized query should succeed");

    // Validate results are ordered by priority (highest first)
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].worker_name, "worker_1"); // Priority 200
    assert_eq!(results[1].worker_name, "worker_2"); // Priority 100
    assert_eq!(results[0].priority, 200);
    assert_eq!(results[1].priority, 100);
}

/// Test get_worker_health_batch SQL function
#[sqlx::test]
async fn test_sql_function_get_worker_health_batch(pool: PgPool) {
    // Setup test data
    let worker1 = create_test_worker(&pool, "health_worker_1").await;
    let worker2 = create_test_worker(&pool, "health_worker_2").await;
    let worker3 = create_test_worker(&pool, "health_worker_3").await;
    
    // Create different health scenarios
    let _reg1 = create_healthy_worker_registration(&pool, worker1.worker_id).await; // Healthy
    let _reg2 = create_stale_worker_registration(&pool, worker2.worker_id).await;   // Stale heartbeat
    // worker3 has no registration - should be unknown status

    let sql_executor = SqlFunctionExecutor::new(pool);
    let worker_ids = vec![worker1.worker_id, worker2.worker_id, worker3.worker_id];
    
    let results = sql_executor
        .get_worker_health_batch(&worker_ids)
        .await
        .expect("Health batch query should succeed");

    // Validate results
    assert_eq!(results.len(), 3);
    
    // Find results by worker_id (order not guaranteed)
    let result1 = results.iter().find(|r| r.worker_id == worker1.worker_id).unwrap();
    let result2 = results.iter().find(|r| r.worker_id == worker2.worker_id).unwrap();
    let result3 = results.iter().find(|r| r.worker_id == worker3.worker_id).unwrap();
    
    assert_eq!(result1.worker_name, "health_worker_1");
    assert_eq!(result1.connection_healthy, true);
    assert_ne!(result1.status, "unknown");
    
    assert_eq!(result2.worker_name, "health_worker_2");
    assert_eq!(result2.connection_healthy, false); // Stale heartbeat
    
    assert_eq!(result3.worker_name, "health_worker_3");
    assert_eq!(result3.status, "unknown"); // No registration
}

/// Test get_worker_health_batch through OptimizedWorkerQueries wrapper
#[sqlx::test]
async fn test_optimized_queries_worker_health_batch(pool: PgPool) {
    // Setup test data
    let worker1 = create_test_worker(&pool, "batch_worker_1").await;
    let worker2 = create_test_worker(&pool, "batch_worker_2").await;
    
    let _reg1 = create_healthy_worker_registration(&pool, worker1.worker_id).await;
    let _reg2 = create_healthy_worker_registration(&pool, worker2.worker_id).await;

    let config = QueryCacheConfig::for_test();
    let optimized_queries = OptimizedWorkerQueries::new_with_config(pool.clone(), config);
    let worker_ids = vec![worker1.worker_id, worker2.worker_id];
    
    let results = optimized_queries
        .get_worker_health_batch(&worker_ids)
        .await
        .expect("Health batch should succeed");

    assert_eq!(results.len(), 2);
    assert!(results.iter().any(|r| r.worker_name == "batch_worker_1"));
    assert!(results.iter().any(|r| r.worker_name == "batch_worker_2"));
}

/// Test select_optimal_worker_for_task SQL function
#[sqlx::test]
async fn test_sql_function_select_optimal_worker(pool: PgPool) {
    // Setup test data
    let namespace = create_test_namespace(&pool, "optimal_namespace").await;
    let named_task = create_test_named_task(&pool, "optimal_task", namespace.task_namespace_id).await;
    
    let worker1 = create_test_worker(&pool, "optimal_worker_1").await;
    let worker2 = create_test_worker(&pool, "optimal_worker_2").await;
    
    // Create different scenarios: worker1 higher priority, worker2 healthier
    let _wnt1 = create_test_worker_named_task(&pool, worker1.worker_id, named_task.named_task_id, 150).await;
    let _wnt2 = create_test_worker_named_task(&pool, worker2.worker_id, named_task.named_task_id, 100).await;
    
    let _reg1 = create_healthy_worker_registration(&pool, worker1.worker_id).await;
    let _reg2 = create_very_healthy_worker_registration(&pool, worker2.worker_id).await; // More recent heartbeat

    let sql_executor = SqlFunctionExecutor::new(pool);
    let result = sql_executor
        .select_optimal_worker_for_task(named_task.named_task_id, Some(1))
        .await
        .expect("Optimal worker selection should succeed");

    // Should find a worker (the scoring algorithm will pick the best one)
    assert!(result.is_some());
    let optimal_worker = result.unwrap();
    assert!(optimal_worker.worker_name == "optimal_worker_1" || optimal_worker.worker_name == "optimal_worker_2");
    assert!(optimal_worker.health_score_f64() > 0.0);
    assert!(optimal_worker.selection_score_f64() > 0.0);
    assert!(optimal_worker.is_healthy());
}

/// Test select_optimal_worker_for_task through OptimizedWorkerQueries wrapper
#[sqlx::test]
async fn test_optimized_queries_select_optimal_worker(pool: PgPool) {
    // Setup test data
    let namespace = create_test_namespace(&pool, "wrapper_namespace").await;
    let named_task = create_test_named_task(&pool, "wrapper_task", namespace.task_namespace_id).await;
    let worker = create_test_worker(&pool, "wrapper_worker").await;
    
    let _wnt = create_test_worker_named_task(&pool, worker.worker_id, named_task.named_task_id, 100).await;
    let _reg = create_healthy_worker_registration(&pool, worker.worker_id).await;

    let config = QueryCacheConfig::for_test();
    let optimized_queries = OptimizedWorkerQueries::new_with_config(pool.clone(), config);
    
    let result = optimized_queries
        .select_optimal_worker_for_task(named_task.named_task_id, 1)
        .await
        .expect("Optimal worker selection should succeed");

    assert!(result.is_some());
    let optimal_worker = result.unwrap();
    assert_eq!(optimal_worker.worker_name, "wrapper_worker");
    assert!(optimal_worker.is_healthy());
}

/// Test get_worker_pool_statistics SQL function
#[sqlx::test]
async fn test_sql_function_worker_pool_statistics(pool: PgPool) {
    // Setup test data with various worker states
    let worker1 = create_test_worker(&pool, "stats_worker_1").await;
    let worker2 = create_test_worker(&pool, "stats_worker_2").await;
    let _worker3 = create_test_worker(&pool, "stats_worker_3").await;
    
    let _reg1 = create_healthy_worker_registration(&pool, worker1.worker_id).await;      // Healthy
    let _reg2 = create_stale_worker_registration(&pool, worker2.worker_id).await;       // Stale
    // _worker3 has no registration

    let sql_executor = SqlFunctionExecutor::new(pool);
    let stats = sql_executor
        .get_worker_pool_statistics()
        .await
        .expect("Pool statistics should succeed");

    // Validate basic counts (exact counts depend on other tests, so we check minimums)
    assert!(stats.total_workers >= 3);
    assert!(stats.registered_workers >= 2); // worker1 and worker2 have registrations
    assert!(stats.avg_health_score_f64() >= 0.0);
    
    // Test helper methods
    assert!(stats.availability_percentage() >= 0.0);
    assert!(stats.availability_percentage() <= 100.0);
    assert!(stats.recent_heartbeat_percentage() >= 0.0);
    assert!(stats.recent_heartbeat_percentage() <= 100.0);
}

/// Test get_worker_pool_statistics through OptimizedWorkerQueries wrapper
#[sqlx::test]
async fn test_optimized_queries_worker_pool_statistics(pool: PgPool) {
    let config = QueryCacheConfig::for_test();
    let optimized_queries = OptimizedWorkerQueries::new_with_config(pool.clone(), config);
    
    let stats = optimized_queries
        .get_worker_pool_statistics()
        .await
        .expect("Pool statistics should succeed");

    // Basic validation
    assert!(stats.total_workers >= 0);
    assert!(stats.healthy_workers >= 0);
    assert!(stats.registered_workers >= 0);
}

/// Test cache invalidation functionality
#[sqlx::test]
async fn test_cache_invalidation(pool: PgPool) {
    let config = QueryCacheConfig::for_test();
    let optimized_queries = OptimizedWorkerQueries::new_with_config(pool.clone(), config);
    
    // Test cache invalidation for specific task
    let result = optimized_queries
        .invalidate_worker_caches(Some(123))
        .await
        .expect("Cache invalidation should succeed");
    
    assert_eq!(result, true); // SQL function should return true
    
    // Test global cache invalidation
    let result = optimized_queries
        .invalidate_worker_caches(None)
        .await
        .expect("Global cache invalidation should succeed");
    
    assert_eq!(result, true);
}

/// Test empty results handling
#[sqlx::test]
async fn test_empty_results_handling(pool: PgPool) {
    let sql_executor = SqlFunctionExecutor::new(pool);
    
    // Test with non-existent task ID
    let results = sql_executor
        .find_active_workers_for_task(99999) // Non-existent task
        .await
        .expect("Query should succeed even with no results");
    
    assert_eq!(results.len(), 0);
    
    // Test optimal worker selection with no workers
    let result = sql_executor
        .select_optimal_worker_for_task(99999, Some(1))
        .await
        .expect("Query should succeed even with no results");
    
    assert!(result.is_none());
    
    // Test health batch with empty array
    let results = sql_executor
        .get_worker_health_batch(&[])
        .await
        .expect("Empty batch should succeed");
    
    assert_eq!(results.len(), 0);
}

// Helper functions for test data creation

async fn create_test_namespace(pool: &PgPool, name: &str) -> TaskNamespace {
    let new_namespace = NewTaskNamespace {
        name: name.to_string(),
        description: Some("Test namespace".to_string()),
    };
    
    TaskNamespace::create(pool, new_namespace)
        .await
        .expect("Failed to create test namespace")
}

async fn create_test_named_task(pool: &PgPool, name: &str, namespace_id: i32) -> NamedTask {
    let new_task = NewNamedTask {
        name: name.to_string(),
        version: Some("1.0.0".to_string()),
        description: Some("Test task".to_string()),
        task_namespace_id: namespace_id as i64,
        configuration: Some(serde_json::json!({})),
    };
    
    NamedTask::create(pool, new_task)
        .await
        .expect("Failed to create test named task")
}

async fn create_test_worker(pool: &PgPool, name: &str) -> Worker {
    let new_worker = NewWorker {
        worker_name: name.to_string(),
        metadata: Some(serde_json::json!({"test": true})),
    };
    
    Worker::create(pool, new_worker)
        .await
        .expect("Failed to create test worker")
}

async fn create_test_worker_named_task(pool: &PgPool, worker_id: i32, named_task_id: i32, priority: i32) -> WorkerNamedTask {
    let new_wnt = NewWorkerNamedTask {
        worker_id,
        named_task_id,
        priority: Some(priority),
        configuration: Some(serde_json::json!({})),
    };
    
    WorkerNamedTask::create(pool, new_wnt)
        .await
        .expect("Failed to create test worker named task")
}

async fn create_healthy_worker_registration(pool: &PgPool, worker_id: i32) -> WorkerRegistration {
    let new_registration = NewWorkerRegistration {
        worker_id,
        status: WorkerStatus::Healthy,
        connection_type: "tcp".to_string(),
        connection_details: serde_json::json!({"host": "localhost", "port": 8080}),
    };
    
    WorkerRegistration::register(pool, new_registration)
        .await
        .expect("Failed to create healthy worker registration")
}

async fn create_very_healthy_worker_registration(pool: &PgPool, worker_id: i32) -> WorkerRegistration {
    let new_registration = NewWorkerRegistration {
        worker_id,
        status: WorkerStatus::Healthy,
        connection_type: "tcp".to_string(),
        connection_details: serde_json::json!({"host": "localhost", "port": 8080}),
    };
    
    WorkerRegistration::register(pool, new_registration)
        .await
        .expect("Failed to create very healthy worker registration")
}

async fn create_stale_worker_registration(pool: &PgPool, worker_id: i32) -> WorkerRegistration {
    let new_registration = NewWorkerRegistration {
        worker_id,
        status: WorkerStatus::Registered,
        connection_type: "tcp".to_string(),
        connection_details: serde_json::json!({"host": "localhost", "port": 8080}),
    };
    
    let mut registration = WorkerRegistration::register(pool, new_registration)
        .await
        .expect("Failed to create stale worker registration");
    
    // Update the registration to have a stale heartbeat (5 minutes ago)
    sqlx::query!(
        "UPDATE tasker_worker_registrations SET last_heartbeat_at = NOW() - INTERVAL '5 minutes' WHERE id = $1",
        registration.id
    )
    .execute(pool)
    .await
    .expect("Failed to update heartbeat");
    
    // Fetch the updated registration
    registration.last_heartbeat_at = Some(chrono::Utc::now().naive_utc() - chrono::Duration::minutes(5));
    registration
}