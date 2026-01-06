"""TAS-125: Checkpoint Yield E2E Test Handlers.

This module provides step handlers for testing the TAS-125 checkpoint yielding
functionality. These handlers demonstrate and test the checkpoint_yield() mechanism.
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from tasker_core.batch_processing import Batchable
from tasker_core.step_handler import StepHandler
from tasker_core.types import ErrorType, StepHandlerResult

if TYPE_CHECKING:
    from tasker_core.types import StepContext


class CheckpointYieldAnalyzerHandler(StepHandler, Batchable):
    """Analyzer handler that creates a single batch for checkpoint testing.

    Creates one batch worker that will process all items, using checkpoint
    yielding to persist progress periodically.

    Configuration from task context:
        - total_items: Number of items to process (default: 100)

    Returns batch_processing_outcome with a single batch covering all items.
    """

    handler_name = "tasker_core.examples.checkpoint_yield.CheckpointYieldAnalyzerHandler"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Analyze and create batch configuration.

        Args:
            context: Step execution context with task data.

        Returns:
            Success result with batch configuration.
        """
        # Get configuration from task context
        total_items = context.input_data.get("total_items", 100)

        # Get worker template name from step definition initialization
        worker_template_name = context.step_config.get(
            "worker_template_name", "checkpoint_yield_batch"
        )

        if total_items <= 0:
            # No items to process - return no_batches outcome
            return self.no_batches_outcome(reason="no_items_to_process")

        # Create a single batch for all items (checkpoint yielding handles chunking)
        cursor_configs = self.create_cursor_ranges(
            total_items=total_items,
            batch_size=total_items,  # Single batch
            step_size=1,
        )

        # Return batch analyzer success with batch configuration
        return self.batch_analyzer_success(
            cursor_configs=cursor_configs,
            total_items=total_items,
            worker_template_name=worker_template_name,
            batch_metadata={
                "test_type": "checkpoint_yield",
                "items_per_batch": total_items,
            },
        )


class CheckpointYieldWorkerHandler(StepHandler, Batchable):
    """Worker handler that processes items with checkpoint yielding.

    Processes items in chunks, yielding checkpoints every N items to test
    the TAS-125 checkpoint persistence and re-dispatch mechanism.

    Configuration from task context:
        - items_per_checkpoint: Items before checkpoint yield (default: 25)
        - fail_after_items: Fail after processing this many items (optional)
        - fail_on_attempt: Only fail on this attempt number (default: 1)
        - permanent_failure: If true, fail with non-retryable error (default: false)

    Checkpoint behavior:
        - After processing items_per_checkpoint items, calls checkpoint_yield()
        - Checkpoint persists cursor position and accumulated results
        - On resume, continues from checkpoint cursor with accumulated results

    Failure injection:
        - Set fail_after_items to simulate a failure after processing N items
        - Set fail_on_attempt to control which attempt fails (default: 1)
        - Set permanent_failure to test non-retryable errors
    """

    handler_name = "tasker_core.examples.checkpoint_yield.CheckpointYieldWorkerHandler"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Process batch items with checkpoint yielding.

        Args:
            context: Step execution context.

        Returns:
            StepHandlerResult - either checkpoint_yield, success, or failure.
        """
        # Check for no-op placeholder
        no_op_result = self.handle_no_op_worker(context)
        if no_op_result:
            return no_op_result

        # Get batch worker inputs (cursor range)
        batch_inputs = self.get_batch_worker_inputs(context)
        if batch_inputs is None:
            return self.failure(
                message="No batch inputs found - is this a batch worker step?",
                error_type=ErrorType.VALIDATION_ERROR,
                retryable=False,
            )

        # Get configuration
        items_per_checkpoint = context.input_data.get(
            "items_per_checkpoint",
            context.step_config.get("items_per_checkpoint", 25),
        )
        fail_after_items = context.input_data.get("fail_after_items")
        fail_on_attempt = context.input_data.get("fail_on_attempt", 1)
        permanent_failure = context.input_data.get("permanent_failure", False)

        # Get checkpoint data if resuming
        # Checkpoint is in workflow_step.checkpoint from FFI event
        workflow_step = context.event.task_sequence_step.get("workflow_step", {})
        checkpoint = workflow_step.get("checkpoint")

        # Determine starting position
        if checkpoint:
            # Resume from checkpoint
            start_cursor = checkpoint.get("cursor", batch_inputs.cursor.start_cursor)
            accumulated = checkpoint.get("accumulated_results", {})
            total_processed = checkpoint.get("items_processed", 0)
        else:
            # Fresh start
            start_cursor = batch_inputs.cursor.start_cursor
            accumulated = {"running_total": 0, "item_ids": []}
            total_processed = 0

        end_cursor = batch_inputs.cursor.end_cursor
        current_attempt = context.retry_count + 1  # 1-indexed

        # Process items in chunks
        current_cursor = start_cursor
        items_in_chunk = 0

        while current_cursor < end_cursor:
            # Check for failure injection
            if fail_after_items is not None:
                if total_processed >= fail_after_items and current_attempt == fail_on_attempt:
                    return self._inject_failure(
                        total_processed,
                        current_cursor,
                        permanent_failure,
                    )

            # Process one item
            item_result = self._process_item(current_cursor)
            accumulated["running_total"] += item_result["value"]
            accumulated["item_ids"].append(item_result["id"])

            current_cursor += 1
            items_in_chunk += 1
            total_processed += 1

            # Check if we should yield a checkpoint
            if items_in_chunk >= items_per_checkpoint and current_cursor < end_cursor:
                return self.checkpoint_yield(
                    cursor=current_cursor,
                    items_processed=total_processed,
                    accumulated_results=accumulated,
                )

        # All items processed - return success
        return self.batch_worker_success(
            items_processed=total_processed,
            items_succeeded=total_processed,
            items_failed=0,
            batch_metadata={
                **accumulated,
                "final_cursor": current_cursor,
                "checkpoints_used": total_processed // items_per_checkpoint,
            },
        )

    def _process_item(self, cursor: int) -> dict[str, Any]:
        """Simulate processing a single item.

        Args:
            cursor: The cursor position (item index).

        Returns:
            Dict with item processing result.
        """
        # Simple processing - just compute a value based on cursor
        return {
            "id": f"item_{cursor:04d}",
            "value": cursor + 1,  # 1-indexed value
        }

    def _inject_failure(
        self,
        items_processed: int,
        cursor: int,
        permanent: bool,
    ) -> StepHandlerResult:
        """Inject a failure for testing retry behavior.

        Args:
            items_processed: Number of items processed so far.
            cursor: Current cursor position.
            permanent: If True, fail with non-retryable error.

        Returns:
            Failure result.
        """
        if permanent:
            return self.failure(
                message=f"Injected permanent failure after {items_processed} items",
                error_type=ErrorType.PERMANENT_ERROR,
                retryable=False,
                metadata={
                    "items_processed": items_processed,
                    "cursor_at_failure": cursor,
                    "failure_type": "permanent",
                },
            )
        else:
            return self.failure(
                message=f"Injected transient failure after {items_processed} items",
                error_type=ErrorType.RETRYABLE_ERROR,
                retryable=True,
                metadata={
                    "items_processed": items_processed,
                    "cursor_at_failure": cursor,
                    "failure_type": "transient",
                },
            )


class CheckpointYieldAggregatorHandler(StepHandler, Batchable):
    """Aggregator handler that collects results from checkpoint yield batch workers.

    Aggregates results from all batch workers to produce final test output.
    """

    handler_name = "tasker_core.examples.checkpoint_yield.CheckpointYieldAggregatorHandler"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Aggregate batch worker results.

        Args:
            context: Step execution context.

        Returns:
            Success result with aggregated data.
        """
        # Detect aggregation scenario
        scenario = self.detect_aggregation_scenario(
            context.dependency_results,
            "analyze_items",
            "checkpoint_yield_batch_",
        )

        if scenario.is_no_batches:
            return self.no_batches_aggregation_result(
                {
                    "total_processed": 0,
                    "running_total": 0,
                    "test_passed": True,
                    "scenario": "no_batches",
                }
            )

        # Aggregate from batch results
        total_processed = 0
        running_total = 0
        all_item_ids: list[str] = []
        checkpoints_used = 0

        for _worker_name, result in scenario.batch_results.items():
            if result:
                total_processed += result.get("items_processed", 0)
                batch_metadata = result.get("batch_metadata", {})
                running_total += batch_metadata.get("running_total", 0)
                all_item_ids.extend(batch_metadata.get("item_ids", []))
                checkpoints_used += batch_metadata.get("checkpoints_used", 0)

        return self.success(
            {
                "total_processed": total_processed,
                "running_total": running_total,
                "item_count": len(all_item_ids),
                "checkpoints_used": checkpoints_used,
                "worker_count": scenario.worker_count,
                "test_passed": True,
                "scenario": "with_batches",
            }
        )
