//! Batch Processing Convergence Helpers
//!
//! Infrastructure for handling convergence step patterns in batch processing workflows.
//! Provides centralized detection logic for NoBatches vs WithBatches scenarios.

use std::collections::HashMap;
use tasker_shared::messaging::StepExecutionResult;
use tasker_shared::TaskerError;

/// Represents the two scenarios a convergence handler must handle in batch processing
///
/// # Scenarios
///
/// ## NoBatches
/// When the batchable step determines the dataset is too small or batching is not needed,
/// it returns a `BatchProcessingOutcome::no_batches()`. In this case:
/// - No batch worker steps are created
/// - The convergence step has a direct dependency on the batchable step
/// - Data should be read from the batchable step's result, not from worker results
///
/// ## WithBatches
/// When the batchable step creates batch workers for parallel processing:
/// - Multiple batch worker steps are created (e.g., `process_batch_001`, `process_batch_002`)
/// - The convergence step depends on all batch workers completing
/// - Results should be aggregated from all batch worker step results
///
/// # Usage
///
/// ```rust,ignore
/// use tasker_worker::batch_processing::BatchAggregationScenario;
///
/// let scenario = BatchAggregationScenario::detect(
///     &step_data.dependency_results,
///     "analyze_dataset",      // batchable step name
///     "process_batch_",       // batch worker prefix
/// )?;
///
/// let total = match scenario {
///     BatchAggregationScenario::NoBatches { batchable_result } => {
///         // Read dataset_size directly from batchable step
///         batchable_result.result.get("dataset_size")
///             .and_then(|v| v.as_u64()).unwrap_or(0)
///     }
///     BatchAggregationScenario::WithBatches { batch_results, .. } => {
///         // Aggregate processed_count from all workers
///         batch_results.iter()
///             .filter_map(|(_, r)| r.result.get("processed_count")?.as_u64())
///             .sum()
///     }
/// };
/// ```
#[derive(Debug)]
pub enum BatchAggregationScenario<'a> {
    /// No batch workers created - read from batchable step directly
    NoBatches {
        /// The result from the batchable step that decided no batches were needed
        batchable_result: &'a StepExecutionResult,
    },
    /// Batch workers created - aggregate their results
    WithBatches {
        /// Results from all batch worker steps (name, result pairs)
        batch_results: Vec<(&'a String, &'a StepExecutionResult)>,
        /// Number of batch workers
        worker_count: usize,
    },
}

impl<'a> BatchAggregationScenario<'a> {
    /// Detect the aggregation scenario from dependency results
    ///
    /// # Arguments
    ///
    /// * `dependency_results` - All step dependencies from TaskSequenceStep
    /// * `batchable_step_name` - Name of the batchable step (e.g., "analyze_dataset")
    /// * `batch_worker_prefix` - Prefix for batch worker step names (e.g., "process_batch_")
    ///
    /// # Returns
    ///
    /// * `Ok(BatchAggregationScenario::NoBatches)` - If batchable step returned NoBatches outcome
    /// * `Ok(BatchAggregationScenario::WithBatches)` - If batch workers were created
    /// * `Err(TaskerError)` - If detection fails (missing dependencies, invalid structure)
    ///
    /// # Detection Logic
    ///
    /// 1. Check if there's a dependency matching `batchable_step_name`
    /// 2. Look for `batch_processing_outcome` in that dependency's result
    /// 3. Check if `outcome.type == "no_batches"`
    /// 4. If yes → NoBatches scenario
    /// 5. If no → WithBatches scenario (filter dependencies by `batch_worker_prefix`)
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Batchable step dependency is missing
    /// - batch_processing_outcome structure is invalid
    /// - No batch workers found but outcome type is not "no_batches"
    pub fn detect(
        dependency_results: &'a HashMap<String, StepExecutionResult>,
        batchable_step_name: &str,
        batch_worker_prefix: &str,
    ) -> Result<Self, TaskerError> {
        // Find the batchable step dependency
        let batchable_result = dependency_results.get(batchable_step_name).ok_or_else(|| {
            TaskerError::WorkerError(format!(
                "Missing batchable step dependency: {batchable_step_name}"
            ))
        })?;

        // Check if this is a NoBatches scenario by examining batch_processing_outcome
        // The outcome structure is: { "type": "no_batches" } or { "type": "with_batches", ... }
        if let Some(outcome) = batchable_result.result.get("batch_processing_outcome") {
            if let Some(outcome_type) = outcome.get("type").and_then(|v| v.as_str()) {
                if outcome_type == "no_batches" {
                    tracing::debug!(
                        batchable_step = batchable_step_name,
                        "Detected NoBatches scenario from batch_processing_outcome"
                    );
                    return Ok(BatchAggregationScenario::NoBatches { batchable_result });
                }
            }
        }

        // Not NoBatches - should have batch workers
        // Filter dependencies to find batch worker steps
        let batch_results: Vec<(&String, &StepExecutionResult)> = dependency_results
            .iter()
            .filter(|(name, _)| name.starts_with(batch_worker_prefix))
            .collect();

        let worker_count = batch_results.len();

        if worker_count == 0 {
            return Err(TaskerError::WorkerError(format!(
                "No batch workers found with prefix '{batch_worker_prefix}' \
                 and batchable step '{batchable_step_name}' did not return NoBatches outcome. \
                 This indicates a workflow configuration error."
            )));
        }

        tracing::debug!(
            batchable_step = batchable_step_name,
            worker_count = worker_count,
            "Detected WithBatches scenario with {worker_count} workers"
        );

        Ok(BatchAggregationScenario::WithBatches {
            batch_results,
            worker_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_step_result(result_data: serde_json::Value) -> StepExecutionResult {
        use tasker_shared::messaging::StepExecutionMetadata;
        use uuid::Uuid;

        StepExecutionResult {
            step_uuid: Uuid::new_v4(),
            success: true,
            result: result_data,
            metadata: StepExecutionMetadata {
                execution_time_ms: 100,
                handler_version: None,
                retryable: false,
                completed_at: chrono::Utc::now(),
                worker_id: None,
                worker_hostname: None,
                started_at: Some(chrono::Utc::now()),
                custom: HashMap::new(),
                error_code: None,
                error_type: None,
                context: HashMap::new(),
            },
            status: "completed".to_string(),
            error: None,
            orchestration_metadata: None,
        }
    }

    #[test]
    fn test_detect_no_batches_scenario() {
        let mut deps = HashMap::new();
        deps.insert(
            "analyze_dataset".to_string(),
            create_step_result(json!({
                "batch_processing_outcome": {
                    "type": "no_batches"
                },
                "dataset_size": 50,
                "reason": "dataset_too_small"
            })),
        );

        let scenario = BatchAggregationScenario::detect(&deps, "analyze_dataset", "process_batch_")
            .expect("Detection should succeed");

        match scenario {
            BatchAggregationScenario::NoBatches { batchable_result } => {
                assert_eq!(
                    batchable_result
                        .result
                        .get("dataset_size")
                        .and_then(|v| v.as_u64()),
                    Some(50)
                );
            }
            _ => panic!("Expected NoBatches scenario"),
        }
    }

    #[test]
    fn test_detect_with_batches_scenario() {
        let mut deps = HashMap::new();
        deps.insert(
            "analyze_dataset".to_string(),
            create_step_result(json!({
                "batch_processing_outcome": {
                    "type": "with_batches",
                    "worker_count": 3
                },
                "dataset_size": 5000
            })),
        );
        deps.insert(
            "process_batch_001".to_string(),
            create_step_result(json!({ "processed_count": 1000 })),
        );
        deps.insert(
            "process_batch_002".to_string(),
            create_step_result(json!({ "processed_count": 1000 })),
        );
        deps.insert(
            "process_batch_003".to_string(),
            create_step_result(json!({ "processed_count": 1000 })),
        );

        let scenario = BatchAggregationScenario::detect(&deps, "analyze_dataset", "process_batch_")
            .expect("Detection should succeed");

        match scenario {
            BatchAggregationScenario::WithBatches {
                batch_results,
                worker_count,
            } => {
                assert_eq!(worker_count, 3);
                assert_eq!(batch_results.len(), 3);
            }
            _ => panic!("Expected WithBatches scenario"),
        }
    }

    #[test]
    fn test_missing_batchable_step_error() {
        let deps = HashMap::new();

        let result = BatchAggregationScenario::detect(&deps, "analyze_dataset", "process_batch_");

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Missing batchable step dependency"));
    }

    #[test]
    fn test_no_workers_without_no_batches_outcome_error() {
        let mut deps = HashMap::new();
        deps.insert(
            "analyze_dataset".to_string(),
            create_step_result(json!({
                "dataset_size": 5000
                // Missing batch_processing_outcome entirely
            })),
        );

        let result = BatchAggregationScenario::detect(&deps, "analyze_dataset", "process_batch_");

        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("No batch workers found"));
    }
}
