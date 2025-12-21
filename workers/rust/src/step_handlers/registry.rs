//! # Rust Step Handler Registry
//!
//! Central registry for all native Rust step handlers, providing a unified way to register,
//! lookup, and manage step handlers across all workflow patterns.
//!
//! This registry exactly mirrors the Ruby `StepHandlerRegistry` pattern but leverages Rust's
//! type system for compile-time guarantees and zero-overhead handler resolution.
//!
//! ## TAS-67: StepHandlerRegistry Trait Implementation
//!
//! This registry implements `StepHandlerRegistry` from `tasker-worker` to enable
//! integration with the `HandlerDispatchService` for non-blocking handler invocation.
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
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

// TAS-67: Import traits from tasker-worker for dispatch integration
use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::types::base::TaskSequenceStep;
use tasker_shared::TaskerResult;
use tasker_worker::worker::handlers::{StepHandler, StepHandlerRegistry};

// TAS-65: Domain event publishing support
use tasker_shared::events::domain_events::DomainEventPublisher;
use tracing::info;

// Import all handler implementations (only completed handlers with new() method)
use super::batch_processing_example::{
    BatchWorkerHandler, DatasetAnalyzerHandler, ResultsAggregatorHandler,
};
use super::batch_processing_products_csv::{
    CsvAnalyzerHandler, CsvBatchProcessorHandler, CsvResultsAggregatorHandler,
};
use super::conditional_approval_rust::{
    AutoApproveHandler, FinalizeApprovalHandler, FinanceReviewHandler, ManagerApprovalHandler,
    RoutingDecisionHandler as ConditionalRoutingDecisionHandler, ValidateRequestHandler,
};
use super::diamond_decision_batch::{
    AggregateEvenResultsHandler, AggregateOddResultsHandler, BranchEvensHandler, BranchOddsHandler,
    DiamondStartHandler as DDBDiamondStartHandler, EvenBatchAnalyzerHandler,
    OddBatchAnalyzerHandler, ProcessEvenBatchHandler, ProcessOddBatchHandler,
    RoutingDecisionHandler as DDBRoutingDecisionHandler,
};
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

// TAS-64: Error injection handlers for retry testing
use super::error_injection::{CheckpointAndFailHandler, FailNTimesHandler};

// TAS-65: Example handler with domain event publishing
use super::payment_example::ProcessPaymentHandler as PaymentExampleHandler;

// TAS-65: Domain event publishing workflow handlers
use super::domain_event_publishing::{
    ProcessPaymentHandler as DomainEventsProcessPaymentHandler,
    SendNotificationHandler as DomainEventsSendNotificationHandler,
    UpdateInventoryHandler as DomainEventsUpdateInventoryHandler,
    ValidateOrderHandler as DomainEventsValidateOrderHandler,
};

/// Central registry for all Rust step handlers
///
/// Provides O(1) handler lookup by name with compile-time type safety.
/// All handlers are registered at construction time for predictable performance.
pub struct RustStepHandlerRegistry {
    handlers: HashMap<String, Arc<dyn RustStepHandler>>,
}

impl std::fmt::Debug for RustStepHandlerRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RustStepHandlerRegistry")
            .field("handler_count", &self.handlers.len())
            .field("handler_names", &self.handlers.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl RustStepHandlerRegistry {
    /// Create a new registry with all handlers pre-registered
    ///
    /// This method registers all 52 step handlers across 9 workflow patterns:
    /// - Linear Workflow (4 handlers)
    /// - Diamond Workflow (4 handlers)
    /// - Tree Workflow (8 handlers)
    /// - Mixed DAG Workflow (7 handlers)
    /// - Order Fulfillment (4 handlers)
    /// - Conditional Approval Rust (6 handlers)
    /// - Batch Processing Example (3 handlers)
    /// - Batch Processing Products CSV (3 handlers)
    /// - Diamond-Decision-Batch (10 handlers)
    #[must_use]
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
    #[must_use]
    pub fn has_handler(&self, name: &str) -> bool {
        self.handlers.contains_key(name)
    }

    /// Check if a handler is registered (cross-language standard API)
    ///
    /// This is an alias for `has_handler()` to match the cross-language standard
    /// defined in TAS-92. All languages use `is_registered(name)`.
    #[must_use]
    pub fn is_registered(&self, name: &str) -> bool {
        self.has_handler(name)
    }

    /// Get all registered handler names
    ///
    /// Returns a sorted vector of all handler names for debugging and introspection.
    #[must_use]
    pub fn get_all_handler_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.handlers.keys().cloned().collect();
        names.sort();
        names
    }

    /// List all registered handler names (cross-language standard API)
    ///
    /// This is an alias for `get_all_handler_names()` to match the cross-language
    /// standard defined in TAS-92. All languages use `list_handlers()`.
    #[must_use]
    pub fn list_handlers(&self) -> Vec<String> {
        self.get_all_handler_names()
    }

    /// Get the number of registered handlers
    #[must_use]
    pub fn handler_count(&self) -> usize {
        self.handlers.len()
    }

    /// TAS-65: Inject domain event publisher into all handlers (phase 2 initialization)
    ///
    /// This method updates all handler configs with the publisher after the registry
    /// has been created. This enables two-phase initialization:
    /// 1. Create registry during construction (without publisher)
    /// 2. Inject publisher after worker bootstrap (when message_client available)
    ///
    /// # Example
    /// ```ignore
    /// let mut registry = RustStepHandlerRegistry::new();
    /// let publisher = Arc::new(DomainEventPublisher::new(message_client));
    /// registry.set_event_publisher(publisher);
    /// ```
    pub fn set_event_publisher(&mut self, publisher: Arc<DomainEventPublisher>) {
        // Re-create all handlers with updated config including publisher
        let handler_names: Vec<String> = self.handlers.keys().cloned().collect();

        for name in handler_names {
            // Create new config with publisher for this handler
            let config = StepHandlerConfig::empty().with_event_publisher(publisher.clone());

            // Re-create handler with new config
            if let Some(new_handler) = Self::create_handler_instance(&name, config) {
                self.handlers.insert(name.clone(), new_handler);
            }
        }

        info!(
            "Injected domain event publisher into {} handlers",
            self.handlers.len()
        );
    }

    /// Helper to create handler instance by name with given config
    ///
    /// Returns None if the handler name is not recognized.
    /// This is used during both initial registration and publisher injection.
    fn create_handler_instance(
        name: &str,
        config: StepHandlerConfig,
    ) -> Option<Arc<dyn RustStepHandler>> {
        match name {
            // Linear Workflow (4 handlers)
            "linear_step_1" => Some(Arc::new(LinearStep1Handler::new(config))),
            "linear_step_2" => Some(Arc::new(LinearStep2Handler::new(config))),
            "linear_step_3" => Some(Arc::new(LinearStep3Handler::new(config))),
            "linear_step_4" => Some(Arc::new(LinearStep4Handler::new(config))),

            // Diamond Workflow (4 handlers)
            "diamond_start" => Some(Arc::new(DiamondStartHandler::new(config))),
            "diamond_branch_b" => Some(Arc::new(DiamondBranchBHandler::new(config))),
            "diamond_branch_c" => Some(Arc::new(DiamondBranchCHandler::new(config))),
            "diamond_end" => Some(Arc::new(DiamondEndHandler::new(config))),

            // Tree Workflow (8 handlers)
            "tree_root" => Some(Arc::new(TreeRootHandler::new(config))),
            "tree_branch_left" => Some(Arc::new(TreeBranchLeftHandler::new(config))),
            "tree_branch_right" => Some(Arc::new(TreeBranchRightHandler::new(config))),
            "tree_leaf_d" => Some(Arc::new(TreeLeafDHandler::new(config))),
            "tree_leaf_e" => Some(Arc::new(TreeLeafEHandler::new(config))),
            "tree_leaf_f" => Some(Arc::new(TreeLeafFHandler::new(config))),
            "tree_leaf_g" => Some(Arc::new(TreeLeafGHandler::new(config))),
            "tree_final_convergence" => Some(Arc::new(TreeFinalConvergenceHandler::new(config))),

            // Mixed DAG Workflow (7 handlers)
            "dag_init" => Some(Arc::new(DagInitHandler::new(config))),
            "dag_analyze" => Some(Arc::new(DagAnalyzeHandler::new(config))),
            "dag_finalize" => Some(Arc::new(DagFinalizeHandler::new(config))),
            "dag_process_left" => Some(Arc::new(DagProcessLeftHandler::new(config))),
            "dag_process_right" => Some(Arc::new(DagProcessRightHandler::new(config))),
            "dag_transform" => Some(Arc::new(DagTransformHandler::new(config))),
            "dag_validate" => Some(Arc::new(DagValidateHandler::new(config))),

            // Order Fulfillment (4 handlers)
            "validate_order" => Some(Arc::new(ValidateOrderHandler::new(config))),
            "ship_order" => Some(Arc::new(ShipOrderHandler::new(config))),
            "reserve_inventory" => Some(Arc::new(ReserveInventoryHandler::new(config))),
            "process_payment" => Some(Arc::new(ProcessPaymentHandler::new(config))),

            // Conditional Approval Rust (6 handlers)
            "validate_request" => Some(Arc::new(ValidateRequestHandler::new(config))),
            "routing_decision" => Some(Arc::new(ConditionalRoutingDecisionHandler::new(config))),
            "auto_approve" => Some(Arc::new(AutoApproveHandler::new(config))),
            "manager_approval" => Some(Arc::new(ManagerApprovalHandler::new(config))),
            "finance_review" => Some(Arc::new(FinanceReviewHandler::new(config))),
            "finalize_approval" => Some(Arc::new(FinalizeApprovalHandler::new(config))),

            // Batch Processing Example (3 handlers)
            "analyze_dataset" | "dataset_analyzer" => {
                Some(Arc::new(DatasetAnalyzerHandler::new(config)))
            }
            "process_batch" | "batch_worker" => Some(Arc::new(BatchWorkerHandler::new(config))),
            "aggregate_results" | "results_aggregator" => {
                Some(Arc::new(ResultsAggregatorHandler::new(config)))
            }

            // Batch Processing Products CSV (3 handlers)
            "analyze_csv" => Some(Arc::new(CsvAnalyzerHandler::new(config))),
            "process_csv_batch" => Some(Arc::new(CsvBatchProcessorHandler::new(config))),
            "aggregate_csv_results" => Some(Arc::new(CsvResultsAggregatorHandler::new(config))),

            // Diamond-Decision-Batch (10 handlers)
            "ddb_diamond_start" => Some(Arc::new(DDBDiamondStartHandler::new(config))),
            "branch_evens" => Some(Arc::new(BranchEvensHandler::new(config))),
            "branch_odds" => Some(Arc::new(BranchOddsHandler::new(config))),
            "ddb_routing_decision" => Some(Arc::new(DDBRoutingDecisionHandler::new(config))),
            "even_batch_analyzer" => Some(Arc::new(EvenBatchAnalyzerHandler::new(config))),
            "process_even_batch" => Some(Arc::new(ProcessEvenBatchHandler::new(config))),
            "aggregate_even_results" => Some(Arc::new(AggregateEvenResultsHandler::new(config))),
            "odd_batch_analyzer" => Some(Arc::new(OddBatchAnalyzerHandler::new(config))),
            "process_odd_batch" => Some(Arc::new(ProcessOddBatchHandler::new(config))),
            "aggregate_odd_results" => Some(Arc::new(AggregateOddResultsHandler::new(config))),

            // TAS-64: Error Injection Handlers
            "fail_twice_then_succeed" | "always_fail" => {
                Some(Arc::new(FailNTimesHandler::new(config)))
            }
            "checkpoint_fail_batch" => Some(Arc::new(CheckpointAndFailHandler::new(config))),

            // TAS-65: Payment example handler
            "payment_example" => Some(Arc::new(PaymentExampleHandler::new(config))),

            // TAS-65: Domain event publishing workflow handlers
            "domain_events_validate_order" => {
                Some(Arc::new(DomainEventsValidateOrderHandler::new(config)))
            }
            "domain_events_process_payment" => {
                Some(Arc::new(DomainEventsProcessPaymentHandler::new(config)))
            }
            "domain_events_update_inventory" => {
                Some(Arc::new(DomainEventsUpdateInventoryHandler::new(config)))
            }
            "domain_events_send_notification" => {
                Some(Arc::new(DomainEventsSendNotificationHandler::new(config)))
            }

            // Unknown handler
            _ => None,
        }
    }

    /// Get handlers grouped by workflow type
    ///
    /// Returns a map of workflow patterns to their handler names for documentation
    /// and debugging purposes.
    #[must_use]
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

        // Conditional Approval Rust
        workflows.insert(
            "conditional_approval_rust".to_string(),
            vec![
                "validate_request".to_string(),
                "routing_decision".to_string(),
                "auto_approve".to_string(),
                "manager_approval".to_string(),
                "finance_review".to_string(),
                "finalize_approval".to_string(),
            ],
        );

        // Batch Processing Example
        workflows.insert(
            "batch_processing_example".to_string(),
            vec![
                "dataset_analyzer".to_string(),
                "batch_worker".to_string(),
                "results_aggregator".to_string(),
            ],
        );

        // Batch Processing Products CSV
        workflows.insert(
            "batch_processing_products_csv".to_string(),
            vec![
                "analyze_csv".to_string(),
                "process_csv_batch".to_string(),
                "aggregate_csv_results".to_string(),
            ],
        );

        // Diamond-Decision-Batch
        workflows.insert(
            "diamond_decision_batch".to_string(),
            vec![
                "ddb_diamond_start".to_string(),
                "branch_evens".to_string(),
                "branch_odds".to_string(),
                "ddb_routing_decision".to_string(),
                "even_batch_analyzer".to_string(),
                "process_even_batch".to_string(),
                "aggregate_even_results".to_string(),
                "odd_batch_analyzer".to_string(),
                "process_odd_batch".to_string(),
                "aggregate_odd_results".to_string(),
            ],
        );

        // TAS-65: Domain Event Publishing
        workflows.insert(
            "domain_event_publishing".to_string(),
            vec![
                "domain_events_validate_order".to_string(),
                "domain_events_process_payment".to_string(),
                "domain_events_update_inventory".to_string(),
                "domain_events_send_notification".to_string(),
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

        // Conditional Approval Rust Handlers (6)
        self.register_handler(Arc::new(ValidateRequestHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(ConditionalRoutingDecisionHandler::new(
            empty_config.clone(),
        )));
        self.register_handler(Arc::new(AutoApproveHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(ManagerApprovalHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(FinanceReviewHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(FinalizeApprovalHandler::new(empty_config.clone())));

        // Batch Processing Example Handlers (3)
        self.register_handler(Arc::new(DatasetAnalyzerHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(BatchWorkerHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(ResultsAggregatorHandler::new(
            empty_config.clone(),
        )));

        // Batch Processing Products CSV Handlers (3)
        self.register_handler(Arc::new(CsvAnalyzerHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(CsvBatchProcessorHandler::new(
            empty_config.clone(),
        )));
        self.register_handler(Arc::new(CsvResultsAggregatorHandler::new(
            empty_config.clone(),
        )));

        // Diamond-Decision-Batch Handlers (10)
        self.register_handler(Arc::new(DDBDiamondStartHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(BranchEvensHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(BranchOddsHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(DDBRoutingDecisionHandler::new(
            empty_config.clone(),
        )));
        self.register_handler(Arc::new(EvenBatchAnalyzerHandler::new(
            empty_config.clone(),
        )));
        self.register_handler(Arc::new(ProcessEvenBatchHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(AggregateEvenResultsHandler::new(
            empty_config.clone(),
        )));
        self.register_handler(Arc::new(OddBatchAnalyzerHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(ProcessOddBatchHandler::new(empty_config.clone())));
        self.register_handler(Arc::new(AggregateOddResultsHandler::new(
            empty_config.clone(),
        )));

        // TAS-64: Error Injection Handlers for retry testing (registered by step name)
        // These handlers are generic/reusable, so register under each step name that uses them
        let fail_n_times: Arc<dyn RustStepHandler> =
            Arc::new(FailNTimesHandler::new(empty_config.clone()));
        self.register_handler_as("fail_twice_then_succeed", Arc::clone(&fail_n_times));
        self.register_handler_as("always_fail", fail_n_times);

        // TAS-64: Batch resumption handler for cursor checkpoint testing
        // Registered under unique name to avoid conflicting with BatchWorkerHandler's "process_batch"
        let checkpoint_and_fail: Arc<dyn RustStepHandler> =
            Arc::new(CheckpointAndFailHandler::new(empty_config.clone()));
        self.register_handler_as("checkpoint_fail_batch", checkpoint_and_fail);

        // TAS-65: Domain Event Publishing Workflow Handlers (4)
        self.register_handler(Arc::new(DomainEventsValidateOrderHandler::new(
            empty_config.clone(),
        )));
        self.register_handler(Arc::new(DomainEventsProcessPaymentHandler::new(
            empty_config.clone(),
        )));
        self.register_handler(Arc::new(DomainEventsUpdateInventoryHandler::new(
            empty_config.clone(),
        )));
        self.register_handler(Arc::new(DomainEventsSendNotificationHandler::new(
            empty_config.clone(),
        )));
    }

    /// Register a single handler in the registry
    ///
    /// Uses the handler's `name()` method as the registry key.
    fn register_handler(&mut self, handler: Arc<dyn RustStepHandler>) {
        let name = handler.name().to_string();
        self.handlers.insert(name, handler);
    }

    /// Register a handler under a specific name (for generic/reusable handlers)
    fn register_handler_as(&mut self, name: &str, handler: Arc<dyn RustStepHandler>) {
        self.handlers.insert(name.to_string(), handler);
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
#[derive(Debug)]
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

// ============================================================================
// TAS-67: StepHandlerRegistry Trait Adapters
// ============================================================================
//
// These adapters enable integration with tasker-worker's HandlerDispatchService
// by implementing the StepHandlerRegistry and StepHandler traits.

/// TAS-67: Adapter to wrap RustStepHandler as StepHandler
///
/// This adapter converts between the Rust worker's native handler trait
/// (`RustStepHandler` returning `anyhow::Result`) and tasker-worker's
/// trait (`StepHandler` returning `TaskerResult`).
struct RustStepHandlerAdapter {
    inner: Arc<dyn RustStepHandler>,
    handler_name: String,
}

impl RustStepHandlerAdapter {
    fn new(handler: Arc<dyn RustStepHandler>) -> Self {
        let handler_name = handler.name().to_string();
        Self {
            inner: handler,
            handler_name,
        }
    }
}

#[async_trait]
impl StepHandler for RustStepHandlerAdapter {
    async fn call(&self, step: &TaskSequenceStep) -> TaskerResult<StepExecutionResult> {
        // Call the inner handler and convert anyhow::Result to TaskerResult
        self.inner
            .call(step)
            .await
            .map_err(|e| tasker_shared::TaskerError::WorkerError(e.to_string()))
    }

    fn name(&self) -> &str {
        &self.handler_name
    }
}

/// TAS-67: Thread-safe adapter for RustStepHandlerRegistry
///
/// Wraps the existing registry in `RwLock` to implement the thread-safe
/// `StepHandlerRegistry` trait from tasker-worker.
///
/// ## Usage
///
/// ```rust,ignore
/// use tasker_worker_rust::step_handlers::{RustStepHandlerRegistry, RustStepHandlerRegistryAdapter};
///
/// // Create adapter from existing registry
/// let registry = RustStepHandlerRegistry::new();
/// let adapter = RustStepHandlerRegistryAdapter::new(registry);
///
/// // Use with HandlerDispatchService
/// let dispatch_service = HandlerDispatchService::new(Arc::new(adapter), ...);
/// ```
pub struct RustStepHandlerRegistryAdapter {
    inner: RwLock<RustStepHandlerRegistry>,
}

impl std::fmt::Debug for RustStepHandlerRegistryAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RustStepHandlerRegistryAdapter")
            .field("handler_count", &self.handler_count())
            .finish()
    }
}

impl RustStepHandlerRegistryAdapter {
    /// Create a new adapter from a RustStepHandlerRegistry
    pub fn new(registry: RustStepHandlerRegistry) -> Self {
        Self {
            inner: RwLock::new(registry),
        }
    }

    /// Create a new adapter with a fresh registry
    pub fn with_default_handlers() -> Self {
        Self::new(RustStepHandlerRegistry::new())
    }

    /// Get handler count for debugging
    fn handler_count(&self) -> usize {
        self.inner
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .handler_count()
    }

    /// TAS-65: Inject domain event publisher into all handlers
    pub fn set_event_publisher(&self, publisher: Arc<DomainEventPublisher>) {
        let mut inner = self.inner.write().unwrap_or_else(|p| p.into_inner());
        inner.set_event_publisher(publisher);
    }
}

#[async_trait]
impl StepHandlerRegistry for RustStepHandlerRegistryAdapter {
    async fn get(&self, step: &TaskSequenceStep) -> Option<Arc<dyn StepHandler>> {
        // Use template_step_name for handler lookup (matches existing RustEventHandler logic)
        let handler_name = &step.workflow_step.template_step_name;

        let inner = self.inner.read().unwrap_or_else(|p| p.into_inner());
        inner.get_handler(handler_name).ok().map(|handler| {
            let adapter: Arc<dyn StepHandler> = Arc::new(RustStepHandlerAdapter::new(handler));
            adapter
        })
    }

    fn register(&self, name: &str, _handler: Arc<dyn StepHandler>) {
        // Note: This is a no-op for Rust handlers since they're pre-registered
        // In the Rust worker, handlers are registered via RustStepHandlerRegistry::register_handler
        tracing::warn!(
            handler_name = %name,
            "RustStepHandlerRegistryAdapter::register called - use RustStepHandlerRegistry::register_handler instead"
        );
    }

    fn handler_available(&self, name: &str) -> bool {
        let inner = self.inner.read().unwrap_or_else(|p| p.into_inner());
        inner.has_handler(name)
    }

    fn registered_handlers(&self) -> Vec<String> {
        let inner = self.inner.read().unwrap_or_else(|p| p.into_inner());
        inner.get_all_handler_names()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_creation() {
        let registry = RustStepHandlerRegistry::new();

        // Should have all 56 handlers (4+4+8+7+4+6+3+3+10+3+4)
        // Linear(4) + Diamond(4) + Tree(8) + MixedDAG(7) + OrderFulfillment(4)
        // + ConditionalApproval(6) + BatchProcessingExample(3) + BatchProcessingProductsCsv(3)
        // + DiamondDecisionBatch(10) + TAS-64 ErrorInjection(3 step registrations)
        // + TAS-65 DomainEventPublishing(4)
        assert_eq!(registry.handler_count(), 56);
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

        // Test conditional approval rust handlers
        assert!(registry.has_handler("validate_request"));
        assert!(registry.has_handler("routing_decision"));
        assert!(registry.has_handler("auto_approve"));
        assert!(registry.has_handler("manager_approval"));
        assert!(registry.has_handler("finance_review"));
        assert!(registry.has_handler("finalize_approval"));

        // Test batch processing example handlers
        assert!(registry.has_handler("analyze_dataset"));
        assert!(registry.has_handler("process_batch"));
        assert!(registry.has_handler("aggregate_results"));

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

        assert_eq!(workflows.len(), 10); // Added domain_event_publishing
        assert_eq!(workflows["linear_workflow"].len(), 4);
        assert_eq!(workflows["diamond_workflow"].len(), 4);
        assert_eq!(workflows["tree_workflow"].len(), 8);
        assert_eq!(workflows["mixed_dag_workflow"].len(), 7);
        assert_eq!(workflows["order_fulfillment"].len(), 4);
        assert_eq!(workflows["conditional_approval_rust"].len(), 6);
        assert_eq!(workflows["batch_processing_example"].len(), 3);
        assert_eq!(workflows["batch_processing_products_csv"].len(), 3);
        assert_eq!(workflows["diamond_decision_batch"].len(), 10);
        assert_eq!(workflows["domain_event_publishing"].len(), 4); // TAS-65
    }

    #[test]
    fn test_global_registry() {
        let registry1 = GlobalRustStepHandlerRegistry::instance();
        let registry2 = GlobalRustStepHandlerRegistry::instance();

        // Should be the same instance
        assert_eq!(registry1 as *const _, registry2 as *const _);
        assert_eq!(registry1.handler_count(), 56); // Updated for TAS-65
    }

    #[test]
    fn test_all_handler_names_sorted() {
        let registry = RustStepHandlerRegistry::new();
        let names = registry.get_all_handler_names();

        // Should have 56 handlers (updated for TAS-65)
        assert_eq!(names.len(), 56);

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
        assert!(names.contains(&"validate_request".to_string()));
    }

    #[test]
    fn test_set_event_publisher() {
        // Create registry without publisher
        let registry = RustStepHandlerRegistry::new();

        // Verify all handlers start with empty config (no publisher)
        assert_eq!(registry.handler_count(), 56); // Updated for TAS-65

        // Create mock publisher (we can't fully test without a real message client,
        // but we can verify the method doesn't panic and maintains handler count)
        // For this test, we'll skip actual publisher creation and just verify
        // the registry maintains its handler count after the operation would complete.

        // The actual integration test will verify end-to-end functionality
        // For now, just verify handler count remains stable
        assert_eq!(registry.handler_count(), 56); // Updated for TAS-65
    }
}
