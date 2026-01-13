//! Utility macros for common patterns across the tasker system
//!
//! This module provides macros to reduce boilerplate for repetitive implementations,
//! particularly for Debug implementations of types containing non-Debug database types.

/// Implement Debug for a type containing a PgPool field
///
/// PgPool doesn't implement Debug, so we show it as the string "PgPool" instead.
///
/// # Examples
///
/// ```
/// use tasker_shared::debug_with_pgpool;
/// use sqlx::PgPool;
///
/// pub struct MyService {
///     pool: PgPool,
///     name: String,
/// }
///
/// debug_with_pgpool!(MyService { pool: PgPool, name });
/// ```
#[macro_export]
macro_rules! debug_with_pgpool {
    // Pattern: StructName { field1: PgPool, field2, field3 }
    ($struct_name:ident { $pool_field:ident: PgPool $(, $field:ident)* $(,)? }) => {
        impl std::fmt::Debug for $struct_name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($struct_name))
                    .field(stringify!($pool_field), &"PgPool")
                    $(
                        .field(stringify!($field), &self.$field)
                    )*
                    .finish()
            }
        }
    };
}

/// Implement Debug for a type containing a QueryBuilder field
///
/// QueryBuilder doesn't implement Debug, so we show it as the string "QueryBuilder" instead.
///
/// # Examples
///
/// ```ignore
/// use tasker_shared::debug_with_query_builder;
/// use sqlx::query_builder::QueryBuilder;
/// use sqlx::Postgres;
///
/// pub struct MyScope {
///     query: QueryBuilder<'static, Postgres>,
/// }
///
/// debug_with_query_builder!(MyScope { query: QueryBuilder });
/// ```
#[macro_export]
macro_rules! debug_with_query_builder {
    // Pattern: StructName { field1: QueryBuilder }
    ($struct_name:ident { $qb_field:ident: QueryBuilder $(, $field:ident)* $(,)? }) => {
        impl std::fmt::Debug for $struct_name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($struct_name))
                    .field(stringify!($qb_field), &"QueryBuilder")
                    $(
                        .field(stringify!($field), &self.$field)
                    )*
                    .finish()
            }
        }
    };
}

/// Implement Debug for a type containing both PgPool and other non-Debug fields
///
/// This is a more general macro that can handle multiple database-related fields.
///
/// # Examples
///
/// ```
/// use tasker_shared::debug_with_db_types;
/// use sqlx::PgPool;
///
/// pub struct MyComplexService {
///     pool: PgPool,
///     connection: PgPool,
/// }
///
/// debug_with_db_types!(MyComplexService {
///     pool => "PgPool",
///     connection => "PgPool"
/// });
/// ```
#[macro_export]
macro_rules! debug_with_db_types {
    // Pattern: StructName { field1 => "display", field2, field3 }
    ($struct_name:ident {
        $( $db_field:ident => $db_display:expr ),* $(,)?
        $( ; $field:ident ),* $(,)?
    }) => {
        impl std::fmt::Debug for $struct_name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!($struct_name))
                    $(
                        .field(stringify!($db_field), &$db_display)
                    )*
                    $(
                        .field(stringify!($field), &self.$field)
                    )*
                    .finish()
            }
        }
    };
}

#[cfg(test)]
mod tests {
    use sqlx::PgPool;

    // Test struct with PgPool
    #[expect(dead_code, reason = "Test struct for validating debug_with_pgpool macro")]
    pub struct TestServiceWithPool {
        pool: PgPool,
        name: String,
        count: usize,
    }

    debug_with_pgpool!(TestServiceWithPool {
        pool: PgPool,
        name,
        count
    });

    #[tokio::test]
    async fn test_debug_with_pgpool_macro() {
        let service = TestServiceWithPool {
            pool: PgPool::connect_lazy("postgresql://test").unwrap(),
            name: "test_service".to_string(),
            count: 42,
        };

        let debug_str = format!("{:?}", service);
        assert!(debug_str.contains("TestServiceWithPool"));
        assert!(debug_str.contains("pool"));
        assert!(debug_str.contains("PgPool"));
        assert!(debug_str.contains("test_service"));
        assert!(debug_str.contains("42"));
    }

    // Test struct with multiple fields including PgPool
    #[expect(dead_code, reason = "Test struct for validating debug_with_pgpool macro with different field names")]
    pub struct AnotherService {
        database: PgPool,
        enabled: bool,
    }

    debug_with_pgpool!(AnotherService {
        database: PgPool,
        enabled
    });

    #[tokio::test]
    async fn test_debug_with_different_pool_field_name() {
        let service = AnotherService {
            database: PgPool::connect_lazy("postgresql://test").unwrap(),
            enabled: true,
        };

        let debug_str = format!("{:?}", service);
        assert!(debug_str.contains("AnotherService"));
        assert!(debug_str.contains("database"));
        assert!(debug_str.contains("PgPool"));
        assert!(debug_str.contains("true"));
    }
}
