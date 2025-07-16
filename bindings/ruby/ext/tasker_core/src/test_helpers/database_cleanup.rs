//! # Database Operations - Test Helpers Only
//!
//! **DEVELOPMENT ONLY**: Database migration and cleanup functions for test isolation.

use crate::context::json_to_ruby_value;
use magnus::{Error, RModule, Ruby, TryConvert, Value};
use tasker_core::database::migrations::DatabaseMigrations;
use sqlx::{PgPool, Row};
use std::time::SystemTime;

/// Run all migrations (setup test database)
fn run_migrations_wrapper(database_url_value: Value) -> Result<Value, Error> {
    let database_url: String = match String::try_convert(database_url_value) {
        Ok(url) => url,
        Err(_) => "postgresql://tasker:tasker@localhost/tasker_rust_test".to_string()
    };

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| {
            Error::new(
                Ruby::get().unwrap().exception_runtime_error(),
                format!("Failed to create tokio runtime for migration operations: {}", e),
            )
        })?;

    let result = runtime.block_on(async {
        // For migrations, use a simple direct database connection to avoid orchestration system complexity
        let pool_options = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)  // Small pool just for migrations
            .acquire_timeout(std::time::Duration::from_secs(30));
            
        let pool_result = pool_options.connect(&database_url).await;
        let pool = match pool_result {
            Ok(p) => p,
            Err(e) => {
                return serde_json::json!({
                    "status": "error",
                    "error": format!("Database connection failed: {}", e),
                    "database_url": database_url
                });
            }
        };
        
        match DatabaseMigrations::run_all(&pool).await {
                    Ok(()) => {
                        // List tables after migration to verify schema
                        let table_query = r"
                            SELECT table_name, table_type
                            FROM information_schema.tables 
                            WHERE table_schema = 'public'
                            ORDER BY table_name
                        ";
                        
                        let mut tables = Vec::new();
                        match sqlx::query(table_query).fetch_all(&pool).await {
                            Ok(rows) => {
                                for row in rows {
                                    let table_name: String = row.get("table_name");
                                    let table_type: String = row.get("table_type");
                                    tables.push(serde_json::json!({
                                        "name": table_name,
                                        "type": table_type
                                    }));
                                }
                            }
                            Err(_e) => {
                                // Just log that table listing failed, don't fail the migration
                            }
                        }
                        
                        serde_json::json!({
                            "status": "success",
                            "message": "All migrations completed successfully",
                            "database_url": database_url,
                            "tables_created": tables,
                            "table_count": tables.len(),
                            "timestamp": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()
                        })
                    }
                    Err(e) => {
                        let error_details = format!("Migration failed: {}", e);
                        eprintln!("❌ MIGRATION ERROR: {}", error_details);
                        
                        serde_json::json!({
                            "status": "error", 
                            "error": error_details,
                            "error_type": "migration_failure",
                            "database_url": database_url,
                            "timestamp": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs(),
                            "suggestions": [
                                "Check database connection parameters",
                                "Verify database exists and is accessible",
                                "Ensure user has migration permissions",
                                "Check if migrations have already been applied"
                            ]
                        })
                    }
                }
    });

    json_to_ruby_value(result)
}

/// Drop database schema (teardown test database)
fn drop_schema_wrapper(database_url_value: Value) -> Result<Value, Error> {
    let database_url: String = match String::try_convert(database_url_value) {
        Ok(url) => url,
        Err(_) => "postgresql://tasker:tasker@localhost/tasker_rust_test".to_string()
    };

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| {
            Error::new(
                Ruby::get().unwrap().exception_runtime_error(),
                format!("Failed to create tokio runtime for schema operations: {}", e),
            )
        })?;

    let result = runtime.block_on(async {
        // For database operations, use a simple direct database connection to avoid orchestration system complexity
        let pool_options = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)  // Small pool just for database operations
            .acquire_timeout(std::time::Duration::from_secs(30));
            
        let pool_result = pool_options.connect(&database_url).await;
        let pool = match pool_result {
            Ok(p) => p,
            Err(e) => {
                return serde_json::json!({
                    "status": "error",
                    "error": format!("Database operation failed: {}", e),
                    "database_url": database_url
                });
            }
        };
                match sqlx::raw_sql(
                    r"
                    DROP SCHEMA public CASCADE;
                    CREATE SCHEMA public;
                    GRANT ALL ON SCHEMA public TO PUBLIC;
                "
                ).execute(&pool).await {
                    Ok(_) => {
                        serde_json::json!({
                            "status": "success",
                            "message": "Schema dropped and recreated successfully",
                            "database_url": database_url,
                            "timestamp": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()
                        })
                    }
                    Err(e) => {
                        let error_details = format!("Schema drop failed: {}", e);
                        eprintln!("Schema drop error: {}", error_details);
                        
                        serde_json::json!({
                            "status": "error",
                            "error": error_details,
                            "error_type": "schema_drop_failure",
                            "database_url": database_url,
                            "timestamp": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs(),
                            "suggestions": [
                                "Check if database connection is still active",
                                "Verify user has DROP and CREATE schema permissions",
                                "Ensure no other processes are using the database",
                                "Check if tables have dependencies or foreign keys",
                                "Try dropping individual tables first"
                            ]
                        })
                    }
                }
    });

    json_to_ruby_value(result)
}

/// Clean up test database
fn cleanup_test_database_wrapper() -> Result<Value, Error> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_e| {
            Error::new(
                Ruby::get().unwrap().exception_runtime_error(),
                "Failed to create tokio runtime for cleanup operations",
            )
        })?;

    let result = runtime.block_on(async {
        // Get database URL from environment
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://tasker:tasker@localhost/tasker_rust_test".to_string());

        // For database operations, use a simple direct database connection to avoid orchestration system complexity
        let pool_options = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)  // Small pool just for database operations
            .acquire_timeout(std::time::Duration::from_secs(30));
            
        let pool_result = pool_options.connect(&database_url).await;
        let pool = match pool_result {
            Ok(p) => p,
            Err(e) => {
                return serde_json::json!({
                    "status": "error",
                    "error": format!("Database operation failed: {}", e),
                    "database_url": database_url
                });
            }
        };
                // Truncate all tables in the correct order (respecting foreign key constraints)
                let cleanup_queries = vec![
                    "TRUNCATE TABLE task_transitions CASCADE",
                    "TRUNCATE TABLE workflow_steps CASCADE", 
                    "TRUNCATE TABLE tasks CASCADE",
                    "TRUNCATE TABLE named_tasks CASCADE",
                    "TRUNCATE TABLE named_steps CASCADE",
                    "TRUNCATE TABLE task_namespaces CASCADE",
                    "TRUNCATE TABLE dependent_systems CASCADE",
                    // Reset sequences to start from 1
                    "ALTER SEQUENCE task_transitions_task_transition_id_seq RESTART WITH 1",
                    "ALTER SEQUENCE workflow_steps_workflow_step_id_seq RESTART WITH 1",
                    "ALTER SEQUENCE tasks_task_id_seq RESTART WITH 1",
                    "ALTER SEQUENCE named_tasks_named_task_id_seq RESTART WITH 1",
                    "ALTER SEQUENCE named_steps_named_step_id_seq RESTART WITH 1",
                    "ALTER SEQUENCE task_namespaces_task_namespace_id_seq RESTART WITH 1",
                    "ALTER SEQUENCE dependent_systems_dependent_system_id_seq RESTART WITH 1",
                ];

                let mut success_count = 0;
                let mut errors = Vec::new();

                for query in cleanup_queries {
                    match sqlx::raw_sql(query).execute(&pool).await {
                        Ok(_) => {
                            success_count += 1;
                        }
                        Err(e) => {
                            errors.push(format!("Query '{}' failed: {}", query, e));
                        }
                    }
                }

                if errors.is_empty() {
                    serde_json::json!({
                        "status": "success",
                        "message": "Database cleanup completed successfully",
                        "cleaned_tables": success_count,
                        "database_url": database_url
                    })
                } else {
                    serde_json::json!({
                        "status": "partial_success",
                        "message": format!("Database cleanup completed with {} errors", errors.len()),
                        "cleaned_tables": success_count,
                        "errors": errors,
                        "database_url": database_url
                    })
                }
    });

    json_to_ruby_value(result)
}

/// Clean up test database with custom database URL
fn cleanup_test_database_with_url_wrapper(database_url_value: Value) -> Result<Value, Error> {
    let database_url: String = match String::try_convert(database_url_value) {
        Ok(url) => url,
        Err(_) => "postgresql://tasker:tasker@localhost/tasker_rust_test".to_string()
    };

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|_e| {
            Error::new(
                Ruby::get().unwrap().exception_runtime_error(),
                "Failed to create tokio runtime for cleanup operations",
            )
        })?;

    let result = runtime.block_on(async {
        // For database operations, use a simple direct database connection to avoid orchestration system complexity
        let pool_options = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)  // Small pool just for database operations
            .acquire_timeout(std::time::Duration::from_secs(30));
            
        let pool_result = pool_options.connect(&database_url).await;
        let pool = match pool_result {
            Ok(p) => p,
            Err(e) => {
                return serde_json::json!({
                    "status": "error",
                    "error": format!("Database operation failed: {}", e),
                    "database_url": database_url
                });
            }
        };
                // Truncate all tables in the correct order (respecting foreign key constraints)
                let cleanup_queries = vec![
                    "TRUNCATE TABLE task_transitions CASCADE",
                    "TRUNCATE TABLE workflow_steps CASCADE", 
                    "TRUNCATE TABLE tasks CASCADE",
                    "TRUNCATE TABLE named_tasks CASCADE",
                    "TRUNCATE TABLE named_steps CASCADE",
                    "TRUNCATE TABLE task_namespaces CASCADE",
                    "TRUNCATE TABLE dependent_systems CASCADE",
                    // Reset sequences to start from 1
                    "ALTER SEQUENCE task_transitions_task_transition_id_seq RESTART WITH 1",
                    "ALTER SEQUENCE workflow_steps_workflow_step_id_seq RESTART WITH 1",
                    "ALTER SEQUENCE tasks_task_id_seq RESTART WITH 1",
                    "ALTER SEQUENCE named_tasks_named_task_id_seq RESTART WITH 1",
                    "ALTER SEQUENCE named_steps_named_step_id_seq RESTART WITH 1",
                    "ALTER SEQUENCE task_namespaces_task_namespace_id_seq RESTART WITH 1",
                    "ALTER SEQUENCE dependent_systems_dependent_system_id_seq RESTART WITH 1",
                ];

                let mut success_count = 0;
                let mut errors = Vec::new();

                for query in cleanup_queries {
                    match sqlx::raw_sql(query).execute(&pool).await {
                        Ok(_) => {
                            success_count += 1;
                        }
                        Err(e) => {
                            errors.push(format!("Query '{}' failed: {}", query, e));
                        }
                    }
                }

                if errors.is_empty() {
                    serde_json::json!({
                        "status": "success",
                        "message": "Database cleanup completed successfully",
                        "cleaned_tables": success_count,
                        "database_url": database_url
                    })
                } else {
                    serde_json::json!({
                        "status": "partial_success",
                        "message": format!("Database cleanup completed with {} errors", errors.len()),
                        "cleaned_tables": success_count,
                        "errors": errors,
                        "database_url": database_url
                    })
                }
    });

    json_to_ruby_value(result)
}

/// List all tables in the database (diagnostic function)
fn list_database_tables_wrapper(database_url_value: Value) -> Result<Value, Error> {
    let database_url: String = match String::try_convert(database_url_value) {
        Ok(url) => url,
        Err(_) => "postgresql://tasker:tasker@localhost/tasker_rust_test".to_string()
    };

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| {
            Error::new(
                Ruby::get().unwrap().exception_runtime_error(),
                format!("Failed to create tokio runtime for table inspection: {}", e),
            )
        })?;

    let result = runtime.block_on(async {
        // For database operations, use a simple direct database connection to avoid orchestration system complexity
        let pool_options = sqlx::postgres::PgPoolOptions::new()
            .max_connections(5)  // Small pool just for database operations
            .acquire_timeout(std::time::Duration::from_secs(30));
            
        let pool_result = pool_options.connect(&database_url).await;
        let pool = match pool_result {
            Ok(p) => p,
            Err(e) => {
                return serde_json::json!({
                    "status": "error",
                    "error": format!("Database operation failed: {}", e),
                    "database_url": database_url
                });
            }
        };
        
        // Query to get all tables in the public schema
        let query = r"
            SELECT table_name, table_type
            FROM information_schema.tables 
            WHERE table_schema = 'public'
            ORDER BY table_name
        ";
        
        match sqlx::query(query).fetch_all(&pool).await {
            Ok(rows) => {
                let mut tables = Vec::new();
                
                for row in rows {
                    let table_name: String = row.get("table_name");
                    let table_type: String = row.get("table_type");
                    tables.push(serde_json::json!({
                        "name": table_name,
                        "type": table_type
                    }));
                }
                
                serde_json::json!({
                    "status": "success",
                    "database_url": database_url,
                    "table_count": tables.len(),
                    "tables": tables,
                    "timestamp": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()
                })
            }
            Err(e) => {
                let error_details = format!("Failed to list tables: {}", e);
                eprintln!("❌ TABLE LISTING ERROR: {}", error_details);
                
                serde_json::json!({
                    "status": "error",
                    "error": error_details,
                    "error_type": "table_listing_failure",
                    "database_url": database_url,
                    "timestamp": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()
                })
            }
        }
    });

    json_to_ruby_value(result)
}

/// Register database cleanup functions
pub fn register_cleanup_functions(module: RModule) -> Result<(), Error> {
    module.define_module_function(
        "run_migrations",
        magnus::function!(run_migrations_wrapper, 1),
    )?;
    
    module.define_module_function(
        "drop_schema", 
        magnus::function!(drop_schema_wrapper, 1),
    )?;
    
    module.define_module_function(
        "cleanup_test_database",
        magnus::function!(cleanup_test_database_wrapper, 0),
    )?;
    
    module.define_module_function(
        "cleanup_test_database_with_url",
        magnus::function!(cleanup_test_database_with_url_wrapper, 1),
    )?;
    
    module.define_module_function(
        "list_database_tables",
        magnus::function!(list_database_tables_wrapper, 1),
    )?;
    
    Ok(())
}