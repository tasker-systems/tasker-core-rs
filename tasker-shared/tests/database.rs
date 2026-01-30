//! Database integration tests
//!
//! Tests for database-level functionality including SQL functions,
//! connection pooling, and migration handling.

#[path = "database/sql_functions.rs"]
pub mod db_sql_functions;
#[path = "database/pool_tests.rs"]
pub mod pool_tests;
