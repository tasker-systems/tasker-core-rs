//! # Database Operations - Test Helpers Only
//!
//! **DEVELOPMENT ONLY**: Database migration and cleanup functions for test isolation.

use crate::context::json_to_ruby_value;
use magnus::{Error, RModule, Ruby, TryConvert, Value};
use tasker_core::database::migrations::DatabaseMigrations;
use sqlx::PgPool;

/// Run all migrations (setup test database)
fn run_migrations_wrapper(database_url_value: Value) -> Result<Value, Error> {
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
                "Failed to create tokio runtime for migration operations",
            )
        })?;

    let result = runtime.block_on(async {
        match PgPool::connect(&database_url).await {
            Ok(pool) => {
                match DatabaseMigrations::run_all(&pool).await {
                    Ok(()) => {
                        serde_json::json!({
                            "status": "success",
                            "message": "All migrations completed successfully",
                            "database_url": database_url
                        })
                    }
                    Err(e) => {
                        serde_json::json!({
                            "status": "error", 
                            "error": format!("Migration failed: {}", e),
                            "database_url": database_url
                        })
                    }
                }
            }
            Err(e) => {
                serde_json::json!({
                    "status": "error",
                    "error": format!("Database connection failed: {}", e),
                    "database_url": database_url
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
        .map_err(|_e| {
            Error::new(
                Ruby::get().unwrap().exception_runtime_error(),
                "Failed to create tokio runtime for schema operations",
            )
        })?;

    let result = runtime.block_on(async {
        match PgPool::connect(&database_url).await {
            Ok(pool) => {
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
                            "database_url": database_url
                        })
                    }
                    Err(e) => {
                        serde_json::json!({
                            "status": "error",
                            "error": format!("Schema drop failed: {}", e),
                            "database_url": database_url
                        })
                    }
                }
            }
            Err(e) => {
                serde_json::json!({
                    "status": "error",
                    "error": format!("Database connection failed: {}", e),
                    "database_url": database_url
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
        // TODO: Implement actual database cleanup
        // This should use proper test database cleanup patterns
        serde_json::json!({
            "status": "cleanup_acknowledged",
            "message": "Database cleanup not yet implemented - should use proper test isolation patterns"
        })
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
    
    Ok(())
}