//! Batch Worker Helper Utilities
//!
//! Provides utilities for batch worker step handlers to extract cursor configuration,
//! detect no-op/placeholder workers, and access batch processing metadata.
//!
//! # Overview
//!
//! When orchestration creates batch workers (either real workers or placeholder workers
//! for NoBatches scenarios), it stores `BatchWorkerInputs` in `workflow_step.inputs`.
//! This module provides structured access through type-safe deserialization.
//!
//! # Architecture
//!
//! The orchestration layer explicitly declares worker intent via the `is_no_op` flag:
//! - `handle_no_batches_outcome` → creates workers with `is_no_op: true`
//! - `handle_create_batches_outcome` → creates workers with `is_no_op: false`
//!
//! Workers trust this explicit declaration - no inference needed!
//!
//! # Usage
//!
//! ```rust
//! use tasker_worker::batch_processing::BatchWorkerContext;
//! use tasker_shared::types::TaskSequenceStep;
//!
//! async fn handle_batch_worker(step_data: &TaskSequenceStep) -> anyhow::Result<()> {
//!     // Deserialize BatchWorkerInputs with explicit is_no_op flag
//!     let context = BatchWorkerContext::from_step_data(step_data)?;
//!
//!     // Trust the explicit flag from orchestration
//!     if context.is_no_op() {
//!         tracing::info!("No-op worker - orchestration declared NoBatches outcome");
//!         return Ok(());
//!     }
//!
//!     // Process batch items - checkpoint_yield() when appropriate (TAS-125)
//!     let mut current_position = context.start_position();
//!     while current_position < context.end_position() {
//!         // Process items...
//!         // When handler decides to checkpoint: return checkpoint_yield(...)
//!         current_position += 100; // Handler decides checkpoint frequency
//!     }
//!
//!     Ok(())
//! }
//! ```

use anyhow::{Context as _, Result};
use tasker_shared::models::core::batch_worker::BatchWorkerInputs;
use tasker_shared::types::TaskSequenceStep;

/// Batch worker execution context wrapping BatchWorkerInputs
///
/// Provides convenient access to batch worker configuration with explicit no-op detection.
/// This wrapper deserializes `BatchWorkerInputs` from `workflow_step.inputs` and provides
/// helper methods for common batch processing operations.
///
/// The key architectural principle: **orchestration declares intent, workers trust**.
/// No inspection or inference is needed - the `is_no_op` flag explicitly tells workers
/// whether they should process data or return immediately.
#[derive(Debug, Clone)]
pub struct BatchWorkerContext {
    /// Deserialized batch worker inputs from orchestration
    inputs: BatchWorkerInputs,
}

impl BatchWorkerContext {
    /// Deserialize batch worker context from TaskSequenceStep
    ///
    /// Extracts and deserializes `BatchWorkerInputs` from `workflow_step.inputs`.
    /// This provides type-safe access to cursor configuration, batch metadata,
    /// and the explicit `is_no_op` flag set by orchestration.
    ///
    /// # Arguments
    ///
    /// * `step_data` - The workflow step data containing inputs
    ///
    /// # Returns
    ///
    /// * `Ok(BatchWorkerContext)` - Successfully deserialized context
    /// * `Err` - Missing or invalid batch worker inputs
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use tasker_worker::batch_processing::BatchWorkerContext;
    /// # use tasker_shared::types::TaskSequenceStep;
    /// # fn example(step_data: &TaskSequenceStep) -> anyhow::Result<()> {
    /// let context = BatchWorkerContext::from_step_data(step_data)?;
    /// if context.is_no_op() {
    ///     return Ok(()); // Trust orchestration's declaration
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn from_step_data(step_data: &TaskSequenceStep) -> Result<Self> {
        // Extract inputs from workflow_step (not step_definition)
        // For dynamic batch workers, instance-specific data is stored in database inputs field
        let inputs_value = step_data
            .workflow_step
            .inputs
            .as_ref()
            .context("Missing inputs in workflow step")?;

        // Deserialize the complete BatchWorkerInputs structure
        let inputs: BatchWorkerInputs = serde_json::from_value(inputs_value.clone())
            .context("Failed to deserialize BatchWorkerInputs from workflow_step.inputs")?;

        Ok(Self { inputs })
    }

    /// Check if this is a no-op/placeholder worker
    ///
    /// Returns the explicit `is_no_op` flag set by orchestration outcome handlers.
    /// No inference or inspection needed - orchestration declares intent explicitly.
    ///
    /// # Returns
    ///
    /// * `true` - Orchestration declared NoBatches outcome, return immediately
    /// * `false` - Real worker that should process data
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// # use tasker_worker::batch_processing::BatchWorkerContext;
    /// # fn example(context: &BatchWorkerContext) -> anyhow::Result<()> {
    /// if context.is_no_op() {
    ///     tracing::info!(
    ///         batch_id = %context.batch_id(),
    ///         "Skipping no-op worker (NoBatches scenario declared by orchestration)"
    ///     );
    ///     return Ok(());
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn is_no_op(&self) -> bool {
        self.inputs.is_no_op
    }

    /// Get the batch identifier
    ///
    /// Returns the batch ID from the cursor configuration.
    /// - "000" indicates a placeholder worker (NoBatches scenario)
    /// - "001", "002", etc. indicate real batch workers
    pub fn batch_id(&self) -> &str {
        &self.inputs.cursor.batch_id
    }

    /// Get the starting position in the dataset (inclusive)
    ///
    /// Returns the numeric value of start_cursor.
    /// For cursor-based pagination, query:
    /// ```sql
    /// WHERE cursor_field > start_cursor AND cursor_field <= end_cursor
    /// ```
    pub fn start_position(&self) -> u64 {
        self.inputs.cursor.start_cursor.as_u64().unwrap_or(0)
    }

    /// Get the ending position in the dataset (exclusive)
    ///
    /// Returns the numeric value of end_cursor.
    pub fn end_position(&self) -> u64 {
        self.inputs.cursor.end_cursor.as_u64().unwrap_or(0)
    }

    /// Get total batch size (original)
    ///
    /// Returns the total number of items in this batch, regardless of checkpoint progress.
    pub fn total_batch_size(&self) -> u64 {
        let start = self.start_position();
        let end = self.end_position();
        end.saturating_sub(start)
    }

    /// Get the cursor field name for database queries
    ///
    /// The database column name to use for cursor-based pagination.
    /// Common values: "id", "created_at", "sequence_number"
    pub fn cursor_field(&self) -> &str {
        &self.inputs.batch_metadata.cursor_field
    }

    /// Get direct access to the underlying BatchWorkerInputs
    ///
    /// Useful for accessing custom fields or when you need the complete structure.
    pub fn inputs(&self) -> &BatchWorkerInputs {
        &self.inputs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Create test BatchWorkerInputs as JSON
    fn create_test_inputs(
        batch_id: &str,
        start_cursor: u64,
        end_cursor: u64,
        is_no_op: bool,
    ) -> serde_json::Value {
        json!({
            "cursor": {
                "batch_id": batch_id,
                "start_cursor": start_cursor,
                "end_cursor": end_cursor,
                "batch_size": (end_cursor - start_cursor) as u32,
            },
            "batch_metadata": {
                "cursor_field": "id",
                "failure_strategy": "continue_on_failure",
            },
            "is_no_op": is_no_op,
        })
    }

    /// Test helper that extracts context directly from JSON inputs
    fn extract_context_from_inputs(inputs: &serde_json::Value) -> Result<BatchWorkerContext> {
        let batch_inputs: BatchWorkerInputs = serde_json::from_value(inputs.clone())?;
        Ok(BatchWorkerContext {
            inputs: batch_inputs,
        })
    }

    #[test]
    fn test_deserialization_success() {
        let inputs = create_test_inputs("001", 0, 100, false);
        let context = extract_context_from_inputs(&inputs).unwrap();

        assert_eq!(context.batch_id(), "001");
        assert_eq!(context.start_position(), 0);
        assert_eq!(context.end_position(), 100);
        assert_eq!(context.cursor_field(), "id");
        assert!(!context.is_no_op());
    }

    #[test]
    fn test_is_no_op_explicit_flag_true() {
        // Placeholder worker with explicit is_no_op flag
        let inputs = create_test_inputs("000", 0, 0, true);
        let context = extract_context_from_inputs(&inputs).unwrap();

        assert!(
            context.is_no_op(),
            "Explicit is_no_op=true should return true"
        );
    }

    #[test]
    fn test_is_no_op_explicit_flag_false() {
        // Real worker with explicit is_no_op flag
        let inputs = create_test_inputs("001", 0, 100, false);
        let context = extract_context_from_inputs(&inputs).unwrap();

        assert!(
            !context.is_no_op(),
            "Explicit is_no_op=false should return false"
        );
    }

    #[test]
    fn test_total_batch_size() {
        let inputs = create_test_inputs("001", 100, 200, false);
        let context = extract_context_from_inputs(&inputs).unwrap();

        assert_eq!(
            context.total_batch_size(),
            100,
            "Total batch size from cursor range"
        );
    }

    #[test]
    fn test_total_batch_size_no_op() {
        let inputs = create_test_inputs("000", 0, 0, true);
        let context = extract_context_from_inputs(&inputs).unwrap();

        assert_eq!(context.total_batch_size(), 0, "No-op has 0 batch size");
    }

    #[test]
    fn test_cursor_field_access() {
        let inputs = create_test_inputs("001", 0, 100, false);
        let context = extract_context_from_inputs(&inputs).unwrap();

        assert_eq!(context.cursor_field(), "id", "Should return cursor field");
    }

    #[test]
    fn test_inputs_access() {
        let inputs = create_test_inputs("001", 0, 100, false);
        let context = extract_context_from_inputs(&inputs).unwrap();

        let batch_inputs = context.inputs();
        assert_eq!(batch_inputs.cursor.batch_id, "001");
        assert!(!batch_inputs.is_no_op);
    }
}
