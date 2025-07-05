//! Query Builder Tests Module
//!
//! This module contains all query builder tests using SQLx native testing
//! for automatic database isolation. These tests cover the high-performance
//! query construction components.

// Query builder component tests
pub mod builder;
pub mod conditions;
pub mod joins;
pub mod pagination;
pub mod scopes;
