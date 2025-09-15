//! # Rust Step Handler Registry
//!
//! Central registry for all native Rust step handlers, providing a unified way to register,
//! lookup, and manage step handlers across all workflow patterns.
//!
//! This registry exactly mirrors the Ruby `StepHandlerRegistry` pattern but leverages Rust's
//! type system for compile-time guarantees and zero-overhead handler resolution.
//!
//! ## Usage
//!
//! ```ignore
//! use tasker_worker_rust::step_handlers::RustStepHandlerRegistry;
//! use anyhow::Result;
//!
//! async fn example() -> Result<()> {
//!     // Create registry with all handlers pre-registered
//!     let registry = RustStepHandlerRegistry::new();
//!
//!     // Lookup handler by name
//!     let handler = registry.get_handler("linear_step_1")?;
//!
//!     // Execute handler (step_data would be provided by the system)
//!     // let result = handler.call(&step_data).await?;
//!
//!     Ok(())
//! }
//! ```

use super::{RustStepHandler, RustStepHandlerError, StepHandlerConfig};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;

// Import all handler implementations (only completed handlers with new() method)
use super::diamond_workflow::{
    DiamondBranchBHandler, DiamondBranchCHandler, DiamondEndHandler, DiamondStartHandler,
};
use super::linear_workflow::{
    LinearStep1Handler, LinearStep2Handler, LinearStep3Handler, LinearStep4Handler,
};
use super::mixed_dag_workflow::{
    DagAnalyzeHandler, DagFinalizeHandler, DagInitHandler, DagProcessLeftHandler,
    DagProcessRightHandler, DagTransformHandler, DagValidateHandler,
};
use super::order_fulfillment::{
    ProcessPaymentHandler, ReserveInventoryHandler, ShipOrderHandler, ValidateOrderHandler,
};
use super::tree_workflow::{
    TreeBranchLeftHandler, TreeBranchRightHandler, TreeFinalConvergenceHandler, TreeLeafDHandler,
    TreeLeafEHandler, TreeLeafFHandler, TreeLeafGHandler, TreeRootHandler,
};

/// Central registry for all Rust step handlers
///
/// Provides O(1) handler lookup by name with compile-time type safety.
/// All handlers are registered at construction time for predictable performance.
pub struct RustStepHandlerRegistry {
    handlers: HashMap<String, Arc<dyn RustStepHandler>>,
}

impl RustStepHandlerRegistry {
    /// Create a new registry with all handlers pre-registered
    ///
    /// This method registers all 27 step handlers across 5 workflow patterns:
    /// - Linear Workflow (4 handlers)
    /// - Diamond Workflow (4 handlers)
    /// - Tree Workflow (8 handlers)
    /// - Mixed DAG Workflow (7 handlers)
    /// - Order Fulfillment (4 handlers)
    pub fn new() -> Self {
        let mut registry = Self {
            handlers: HashMap::new(),
        };

        registry.register_all_handlers();
        registry
    }

    /// Get a handler by name
    ///
    /// Returns a reference to the handler if found, or an error if not registered.
    /// Handler lookup is O(1) with no allocations.
    pub fn get_handler(
        &self,
        name: &str,
    ) -> Result<Arc<dyn RustStepHandler>, RustStepHandlerError> {
        self.handlers
            .get(name)
            .cloned()
            .ok_or_else(|| RustStepHandlerError::SystemError {
                message: format!("Handler '{}' not found in registry", name),
            })
    }

    /// Check if a handler is registered
    pub fn has_handler(&self, name: &str) -> bool {
        self.handlers.contains_key(name)
    }

    /// Get all registered handler names
    ///
    /// Returns a sorted vector of all handler names for debugging and introspection.
    pub fn get_all_handler_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.handlers.keys().cloned().collect();
        names.sort();
        names
    }

    /// Get the number of registered handlers
    pub fn handler_count(&self) -> usize {
        self.handlers.len()
    }

    /// Get handlers grouped by workflow type
    ///
    /// Returns a map of workflow patterns to their handler names for documentation
    /// and debugging purposes.
    pub fn get_handlers_by_workflow(&self) -> HashMap<String, Vec<String>> {
        let mut workflows = HashMap::new();

        // Linear Workflow
        workflows.insert(
            "linear_workflow".to_string(),
            vec![
                "linear_step_1".to_string(),
                "linear_step_2".to_string(),
                "linear_step_3".to_string(),
                "linear_step_4".to_string(),
            ],
        );

        // Diamond Workflow
        workflows.insert(
            "diamond_workflow".to_string(),
            vec![
                "diamond_start".to_string(),
                "diamond_branch_b".to_string(),
                "diamond_branch_c".to_string(),
                "diamond_end".to_string(),
            ],
        );

        // Tree Workflow
        workflows.insert(
            "tree_workflow".to_string(),
            vec![
                "tree_root".to_string(),
                "tree_branch_left".to_string(),
                "tree_branch_right".to_string(),
                "tree_leaf_d".to_string(),
                "tree_leaf_e".to_string(),
                "tree_leaf_f".to_string(),
                "tree_leaf_g".to_string(),
                "tree_final_convergence".to_string(),
            ],
        );

        // Mixed DAG Workflow
        workflows.insert(
            "mixed_dag_workflow".to_string(),
            vec![
                "dag_init".to_string(),
                "dag_process_left".to_string(),
                "dag_process_right".to_string(),
                "dag_validate".to_string(),
                "dag_transform".to_string(),
                "dag_analyze".to_string(),
                "dag_finalize".to_string(),
            ],
        );

        // Order Fulfillment
        workflows.insert(
            "order_fulfillment".to_string(),
            vec![
                "validate_order".to_string(),
                "reserve_inventory".to_string(),
                "process_payment".to_string(),
                "ship_order".to_string(),
            ],
        );

        workflows
    }

    /// Register all handlers in the registry
    ///
    /// This method is called during construction to populate the registry with
    /// all available step handlers. Each handler is wrapped in an Arc for
    /// efficient sharing across async tasks.
    fn register_all_handlers(&mut self) {
        let empty_config = StepHandlerConfig::empty();

        // Linear Workflow Handlers (4)
        self.register_handler(Arc::new(LinearStep1Handler::new(empty_config.clone())));
        self.register_handler(Arc::new(LinearStep2Handler::new(empty_config.clone())));
        self.register_handler(Arc::new(LinearStep3Handler::new(empty_config.clone())));
        self.register_handler(Arc::new(LinearStep4Handler::new(empty_config.clone())));

        // Diamond Workflow Handlers (4)
        self.register_handler(Arc::new(DiamondStartHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(DiamondBranchBHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(DiamondBranchCHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(DiamondEndHandler::new(empty_config.clone())));

        // Tree Workflow Handlers (8)
        self.register_handler(Arc::new(TreeRootHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(TreeBranchLeftHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(TreeBranchRightHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(TreeLeafDHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(TreeLeafEHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(TreeLeafFHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(TreeLeafGHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(TreeFinalConvergenceHandler::new(
            empty_config.clone(),
        )));

        // Mixed DAG Workflow Handlers (7)
        self.register_handler(Arc::new(DagInitHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(DagAnalyzeHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(DagFinalizeHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(DagProcessLeftHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(DagProcessRightHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(DagTransformHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(DagValidateHandler::new(empty_config.clone())));

        // Order Fulfillment Handlers (4)
        self.register_handler(Arc::new(ValidateOrderHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(ShipOrderHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(ReserveInventoryHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(ProcessPaymentHandler::new(empty_config.clone())));
    }

    /// Register a single handler in the registry
    ///
    /// Uses the handler's `name()` method as the registry key.
    fn register_handler(&mut self, handler: Arc<dyn RustStepHandler>) {
        let name = handler.name().to_string();
        self.handlers.insert(name, handler);
    }
}

impl Default for RustStepHandlerRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Thread-safe singleton access to the registry
///
/// Provides a global registry instance that can be accessed from anywhere
/// in the application. The registry is initialized once and reused.
pub struct GlobalRustStepHandlerRegistry;

impl GlobalRustStepHandlerRegistry {
    /// Get the global registry instance
    ///
    /// Creates the registry on first access and returns the same instance
    /// on subsequent calls. Thread-safe initialization guaranteed.
    pub fn instance() -> &'static RustStepHandlerRegistry {
        static INSTANCE: std::sync::OnceLock<RustStepHandlerRegistry> = std::sync::OnceLock::new();
        INSTANCE.get_or_init(RustStepHandlerRegistry::new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = RustStepHandlerRegistry::new();

        // Should have all 27 handlers (4+4+8+7+4)
        assert_eq!(registry.handler_count(), 27);
    }

    #[test]
    fn test_handler_lookup() {
        let registry = RustStepHandlerRegistry::new();

        // Test linear workflow handlers
        assert!(registry.has_handler("linear_step_1"));
        assert!(registry.has_handler("linear_step_2"));
        assert!(registry.has_handler("linear_step_3"));
        assert!(registry.has_handler("linear_step_4"));

        // Test diamond workflow handlers
        assert!(registry.has_handler("diamond_start"));
        assert!(registry.has_handler("diamond_branch_b"));
        assert!(registry.has_handler("diamond_branch_c"));
        assert!(registry.has_handler("diamond_end"));

        // Test tree workflow handlers
        assert!(registry.has_handler("tree_root"));
        assert!(registry.has_handler("tree_final_convergence"));

        // Test mixed DAG workflow handlers
        assert!(registry.has_handler("dag_init"));
        assert!(registry.has_handler("dag_finalize"));

        // Test order fulfillment handlers
        assert!(registry.has_handler("validate_order"));
        assert!(registry.has_handler("reserve_inventory"));
        assert!(registry.has_handler("process_payment"));
        assert!(registry.has_handler("ship_order"));

        // Test non-existent handler
        assert!(!registry.has_handler("nonexistent_handler"));
    }

    #[test]
    fn test_get_handler_success() {
        let registry = RustStepHandlerRegistry::new();

        let handler = registry.get_handler("linear_step_1").unwrap();
        assert_eq!(handler.name(), "linear_step_1");
    }

    #[test]
    fn test_get_handler_failure() {
        let registry = RustStepHandlerRegistry::new();

        let result = registry.get_handler("nonexistent_handler");
        assert!(result.is_err());
    }

    #[test]
    fn test_handlers_by_workflow() {
        let registry = RustStepHandlerRegistry::new();
        let workflows = registry.get_handlers_by_workflow();

        assert_eq!(workflows.len(), 5);
        assert_eq!(workflows["linear_workflow"].len(), 4);
        assert_eq!(workflows["diamond_workflow"].len(), 4);
        assert_eq!(workflows["tree_workflow"].len(), 8);
        assert_eq!(workflows["mixed_dag_workflow"].len(), 7);
        assert_eq!(workflows["order_fulfillment"].len(), 4);
    }

    #[test]
    fn test_global_registry() {
        let registry1 = GlobalRustStepHandlerRegistry::instance();
        let registry2 = GlobalRustStepHandlerRegistry::instance();

        // Should be the same instance
        assert_eq!(registry1 as *const _, registry2 as *const _);
        assert_eq!(registry1.handler_count(), 27);
    }

    #[test]
    fn test_all_handler_names_sorted() {
        let registry = RustStepHandlerRegistry::new();
        let names = registry.get_all_handler_names();

        // Should have 27 handlers
        assert_eq!(names.len(), 27);

        // Should be sorted
        let mut sorted_names = names.clone();
        sorted_names.sort();
        assert_eq!(names, sorted_names);

        // Should include key handlers from each workflow
        assert!(names.contains(&"linear_step_1".to_string()));
        assert!(names.contains(&"diamond_start".to_string()));
        assert!(names.contains(&"tree_root".to_string()));
        assert!(names.contains(&"dag_init".to_string()));
        assert!(names.contains(&"validate_order".to_string()));
    }
}
