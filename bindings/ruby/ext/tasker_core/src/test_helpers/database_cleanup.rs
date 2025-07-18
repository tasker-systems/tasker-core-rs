//! # Database Operations - Test Helpers Only
//!
//! **DEVELOPMENT ONLY**: Database migration and cleanup functions for test isolation.

use crate::context::json_to_ruby_value;
use sqlx::Row;
use magnus::{Error, RModule, TryConvert, Value};
use tasker_core::database::migrations::DatabaseMigrations;
use std::time::SystemTime;

/// Sequential database setup wrapper - uses single pool for entire operation
async fn sequential_database_setup(database_url: &str) -> serde_json::Value {
    println!("🔍 SEQUENTIAL SETUP: Starting sequential database setup with single pool for {}", database_url);

    // Create single pool for entire operation to avoid timing issues
    let pool = crate::globals::create_temporary_database_pool();

    // Step 1: Terminate existing connections
    println!("🔍 SEQUENTIAL SETUP: Step 1 - Terminating existing connections");
    let terminate_result = sqlx::raw_sql(
        r"
        SELECT pg_terminate_backend(pid)
        FROM pg_stat_activity
        WHERE datname = current_database()
        AND pid <> pg_backend_pid()
        AND state IN ('active', 'idle', 'idle in transaction', 'idle in transaction (aborted)')
        "
    ).execute(&pool).await;

    match terminate_result {
        Ok(_) => println!("🔍 SEQUENTIAL SETUP: Terminated existing connections"),
        Err(e) => println!("🔍 SEQUENTIAL SETUP: Failed to terminate connections (continuing): {}", e),
    }

    // Step 2: Drop and recreate schema
    println!("🔍 SEQUENTIAL SETUP: Step 2 - Dropping and recreating schema");
    let schema_drop_result = sqlx::raw_sql(
        r"
        DROP SCHEMA public CASCADE;
        CREATE SCHEMA public;
        GRANT ALL ON SCHEMA public TO PUBLIC;
        "
    ).execute(&pool).await;

    match schema_drop_result {
        Ok(_) => println!("🔍 SEQUENTIAL SETUP: Schema drop and recreate completed"),
        Err(e) => {
            let error_msg = format!("Schema drop failed: {}", e);
            println!("🔍 SEQUENTIAL SETUP: Schema drop failed - {}", error_msg);

            // Close pool before returning error
            crate::globals::close_database_pool(pool);

            return serde_json::json!({
                "status": "error",
                "error": error_msg,
                "error_type": "schema_drop_failure",
                "database_url": database_url,
                "step": "schema_drop"
            });
        }
    }

    // Step 3: Create migration tracking table
    println!("🔍 SEQUENTIAL SETUP: Step 3 - Creating migration tracking table");
    let migration_result = sqlx::raw_sql(
        r"
        CREATE TABLE IF NOT EXISTS tasker_schema_migrations (
            version VARCHAR(14) PRIMARY KEY,
            applied_at TIMESTAMP WITHOUT TIME ZONE DEFAULT NOW()
        )
        "
    ).execute(&pool).await;

    // Close pool after all operations complete
    crate::globals::close_database_pool(pool);

    match migration_result {
        Ok(_) => {
            println!("🔍 SEQUENTIAL SETUP: All steps completed successfully");
            serde_json::json!({
                "status": "success",
                "message": "Sequential database setup completed successfully",
                "database_url": database_url,
                "steps_completed": ["terminate_connections", "schema_drop", "migration_setup"],
                "timestamp": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()
            })
        }
        Err(e) => {
            let error_msg = format!("Migration setup failed: {}", e);
            println!("🔍 SEQUENTIAL SETUP: Migration setup failed - {}", error_msg);
            serde_json::json!({
                "status": "error",
                "error": error_msg,
                "error_type": "migration_setup_failure",
                "database_url": database_url,
                "step": "migration_setup"
            })
        }
    }
}

/// Run all migrations (setup test database)
fn run_migrations_wrapper(database_url_value: Value) -> Result<Value, Error> {
    let _database_url: String = match String::try_convert(database_url_value) {
        Ok(url) => url,
        Err(_) => "postgresql://tasker:tasker@localhost/tasker_rust_test".to_string()
    };

    let result = crate::globals::execute_async(async {
        let database_url = std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgresql://tasker:tasker@localhost/tasker_rust_test".to_string());

        // Check if we're in test environment
        let is_test_env = std::env::var("RAILS_ENV").unwrap_or_default() == "test"
            || std::env::var("APP_ENV").unwrap_or_default() == "test";

        if is_test_env {
            // Use sequential database setup for test environment
            println!("🔍 MIGRATION TRACE: Using sequential database setup for test environment");
            sequential_database_setup(&database_url).await
        } else {
            // For non-test environments, run normal migrations with dedicated pool
            println!("🔍 MIGRATION TRACE: Running normal migrations for non-test environment");
            let pool = crate::globals::create_temporary_database_pool();

            match DatabaseMigrations::run_all(&pool).await {
                Ok(()) => {
                    println!("🔍 MIGRATION TRACE: DatabaseMigrations::run_all completed successfully (pool size: {})", pool.size());

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

                    // CRITICAL: Explicitly close the temporary pool to ensure connections are returned
                    crate::globals::close_database_pool(pool);

                    serde_json::json!({
                        "status": "success",
                        "message": "All migrations completed successfully",
                        "database_url": _database_url,
                        "tables_created": tables,
                        "table_count": tables.len(),
                        "timestamp": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()
                    })
                }
                Err(e) => {
                    let error_details = format!("Migration failed: {}", e);
                    eprintln!("❌ MIGRATION ERROR: {}", error_details);

                    // CRITICAL: Explicitly close the temporary pool even on error
                    crate::globals::close_database_pool(pool);

                    serde_json::json!({
                        "status": "error",
                        "error": error_details,
                        "error_type": "migration_failure",
                        "database_url": _database_url,
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

    let result = crate::globals::execute_async(async {
        println!("🔍 SCHEMA DROP: Starting schema drop for {}", database_url);

        // Use single temporary pool for entire operation
        let pool = crate::globals::create_temporary_database_pool();

        // Step 1: Terminate existing connections
        let terminate_result = sqlx::raw_sql(
            r"
            SELECT pg_terminate_backend(pid)
            FROM pg_stat_activity
            WHERE datname = current_database()
            AND pid <> pg_backend_pid()
            AND state IN ('active', 'idle', 'idle in transaction', 'idle in transaction (aborted)')
            "
        ).execute(&pool).await;

        match terminate_result {
            Ok(_) => println!("🔍 SCHEMA DROP: Terminated existing connections"),
            Err(e) => println!("🔍 SCHEMA DROP: Failed to terminate connections (continuing): {}", e),
        }

        // Step 2: Drop and recreate schema
        let drop_result = sqlx::raw_sql(
            r"
            DROP SCHEMA public CASCADE;
            CREATE SCHEMA public;
            GRANT ALL ON SCHEMA public TO PUBLIC;
            "
        ).execute(&pool).await;

        // Step 3: Close pool and return result
        crate::globals::close_database_pool(pool);

        match drop_result {
            Ok(_) => {
                println!("🔍 SCHEMA DROP: Schema drop and recreate completed successfully");
                serde_json::json!({
                    "status": "success",
                    "message": "Schema dropped and recreated successfully",
                    "database_url": database_url,
                    "timestamp": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs()
                })
            }
            Err(e) => {
                let error_details = format!("Schema drop failed: {}", e);
                println!("🔍 SCHEMA DROP: {}", error_details);
                serde_json::json!({
                    "status": "error",
                    "error": error_details,
                    "error_type": "schema_drop_failure",
                    "database_url": database_url,
                    "timestamp": SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs(),
                    "suggestions": [
                        "Check if database connection is still active",
                        "Verify user has DROP and CREATE schema permissions",
                        "Ensure no other processes are using the database"
                    ]
                })
            }
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

    let result = crate::globals::execute_async(async {
        // CRITICAL: Use temporary pool for table listing to ensure proper connection cleanup
        let pool = crate::globals::create_temporary_database_pool();

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

                // CRITICAL: Explicitly close the temporary pool to ensure connections are returned
                crate::globals::close_database_pool(pool);

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

                // CRITICAL: Explicitly close the temporary pool even on error
                crate::globals::close_database_pool(pool);

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
        "list_database_tables",
        magnus::function!(list_database_tables_wrapper, 1),
    )?;

    Ok(())
}
