//! # Base Factory Infrastructure
//!
//! Core traits and utilities for the factory system that integrates with SQLx.
//!
//! This module provides the foundation for all factories, including:
//! - Database persistence integration
//! - Dependency resolution
//! - State management
//! - Relationship handling

use async_trait::async_trait;
use serde_json::Value;
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;

/// Result type for factory operations
pub type FactoryResult<T> = Result<T, FactoryError>;

/// Factory operation errors
#[derive(Debug, thiserror::Error)]
pub enum FactoryError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Dependency resolution failed: {message}")]
    DependencyResolution { message: String },

    #[error("State transition failed: {reason}")]
    StateTransition { reason: String },

    #[error("Invalid configuration: {details}")]
    InvalidConfig { details: String },
}

/// Core trait for all factories with SQLx integration
#[async_trait]
pub trait SqlxFactory<T: Send> {
    /// Create the entity in the database
    async fn create(&self, pool: &PgPool) -> FactoryResult<T>;

    /// Create multiple entities efficiently
    async fn create_batch(&self, pool: &PgPool, count: usize) -> FactoryResult<Vec<T>> {
        let mut results = Vec::with_capacity(count);
        for _ in 0..count {
            results.push(self.create(pool).await?);
        }
        Ok(results)
    }

    /// Find or create pattern for idempotent operations
    async fn find_or_create(&self, pool: &PgPool) -> FactoryResult<T>;
}

/// Trait for factories that can build relationships with other entities
#[async_trait]
pub trait RelationshipFactory<T> {
    /// Add a dependency on another entity
    fn depends_on(self, entity_uuid: Uuid) -> Self;

    /// Add multiple dependencies
    fn with_dependencies(self, entity_uuids: &[Uuid]) -> Self;

    /// Set up relationships after creation
    async fn setup_relationships(&self, entity: &T, pool: &PgPool) -> FactoryResult<()>;
}

/// Trait for factories that manage state transitions
#[async_trait]
pub trait StateFactory<T> {
    /// Apply state transitions after creation
    async fn apply_state_transitions(&self, entity: &T, pool: &PgPool) -> FactoryResult<()>;

    /// Set initial state
    fn with_initial_state(self, state: &str) -> Self;

    /// Add state transition sequence
    fn with_state_sequence(self, states: Vec<String>) -> Self;
}

/// Context builder for flexible attribute setting
#[derive(Debug, Clone, Default)]
pub struct FactoryContext {
    pub attributes: HashMap<String, Value>,
    pub relationships: HashMap<String, Vec<Uuid>>,
    pub states: Vec<String>,
    pub metadata: HashMap<String, Value>,
}

impl FactoryContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set<T: Into<Value>>(mut self, key: &str, value: T) -> Self {
        self.attributes.insert(key.to_string(), value.into());
        self
    }

    pub fn with_relationship(mut self, name: &str, ids: Vec<Uuid>) -> Self {
        self.relationships.insert(name.to_string(), ids);
        self
    }

    pub fn with_states(mut self, states: Vec<String>) -> Self {
        self.states = states;
        self
    }

    pub fn with_metadata<T: Into<Value>>(mut self, key: &str, value: T) -> Self {
        self.metadata.insert(key.to_string(), value.into());
        self
    }

    pub fn get<T>(&self, key: &str) -> Option<T>
    where
        T: for<'de> serde::Deserialize<'de>,
    {
        self.attributes
            .get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }
}

/// Trait for factories that use the context system
pub trait ContextualFactory {
    /// Apply context to the factory
    fn with_context(self, context: FactoryContext) -> Self;

    /// Extract context for use during creation
    fn context(&self) -> &FactoryContext;
}

/// Base factory struct that other factories can inherit from
#[derive(Debug, Clone)]
pub struct BaseFactory {
    pub context: FactoryContext,
    pub dependencies: Vec<Uuid>,
    pub auto_create_dependencies: bool,
}

impl Default for BaseFactory {
    fn default() -> Self {
        Self {
            context: FactoryContext::new(),
            dependencies: Vec::new(),
            auto_create_dependencies: true,
        }
    }
}

impl BaseFactory {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_auto_dependencies(mut self, enabled: bool) -> Self {
        self.auto_create_dependencies = enabled;
        self
    }
}

impl ContextualFactory for BaseFactory {
    fn with_context(mut self, context: FactoryContext) -> Self {
        self.context = context;
        self
    }

    fn context(&self) -> &FactoryContext {
        &self.context
    }
}

/// Utility functions for common factory operations
pub mod utils {
    use super::*;
    use crate::validation;
    use chrono::Utc;

    /// Generate unique identity hash for testing
    pub fn generate_test_identity_hash() -> String {
        format!("test_{}", fastrand::u64(..))
    }

    /// Generate test context with common patterns
    pub fn generate_test_context() -> Value {
        serde_json::json!({
            "test_run_id": generate_test_identity_hash(),
            "created_at": Utc::now(),
            "environment": "test",
            "auto_generated": true
        })
    }

    /// Sanitize context using validation rules
    pub fn sanitize_context(context: Value) -> FactoryResult<Value> {
        // sanitize_json always succeeds, it just cleans the input
        Ok(validation::sanitize_json(context))
    }

    /// Validate JSONB input before database insertion
    pub fn validate_jsonb(value: &Value) -> FactoryResult<()> {
        validation::validate_jsonb_input(value).map_err(|e| FactoryError::InvalidConfig {
            details: format!("JSONB validation failed: {e}"),
        })
    }

    /// Generate sequential names for batch creation
    pub fn generate_sequential_names(base: &str, count: usize) -> Vec<String> {
        (1..=count).map(|i| format!("{base}_{i:03}")).collect()
    }

    /// Create realistic test metadata
    pub fn generate_test_metadata(entity_type: &str) -> Value {
        serde_json::json!({
            "entity_type": entity_type,
            "factory_created": true,
            "test_suite": "factories",
            "created_at": Utc::now(),
            "test_metadata": {
                "auto_generated": true,
                "factory_version": "1.0.0"
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_factory_context_builder() {
        let context = FactoryContext::new()
            .set("name", "test_entity")
            .set("count", 42)
            .with_relationship(
                "dependencies",
                vec![Uuid::now_v7(), Uuid::now_v7(), Uuid::now_v7()],
            )
            .with_states(vec!["pending".to_string(), "active".to_string()])
            .with_metadata("test_flag", true);

        assert_eq!(
            context.get::<String>("name"),
            Some("test_entity".to_string())
        );
        assert_eq!(context.get::<i32>("count"), Some(42));
        assert_eq!(context.relationships.get("dependencies").unwrap().len(), 3);
        assert_eq!(context.states.len(), 2);
        assert!(context.metadata.contains_key("test_flag"));
    }

    #[test]
    fn test_utils_functions() {
        let hash = utils::generate_test_identity_hash();
        assert!(hash.starts_with("test_"));

        let context = utils::generate_test_context();
        assert!(context.is_object());
        assert!(context.get("test_run_id").is_some());

        let names = utils::generate_sequential_names("entity", 3);
        assert_eq!(names, vec!["entity_001", "entity_002", "entity_003"]);
    }
}
