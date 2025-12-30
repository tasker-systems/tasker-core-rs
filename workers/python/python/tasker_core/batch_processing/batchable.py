"""Batchable mixin for batch processing handlers.

This module provides the Batchable mixin class that adds batch processing
capabilities to step handlers. It supports both analyzer (split) and worker
(process) patterns for parallel batch processing.

Batch Processing Pattern:
    1. Analyzer step: Determines total items and creates cursor ranges
    2. Worker steps: Process items within their assigned cursor range
    3. Aggregator step (optional): Combines results from all workers

Example:
    >>> from tasker_core.step_handler import StepHandler
    >>> from tasker_core.batch_processing import Batchable
    >>>
    >>> class ProductAnalyzer(StepHandler, Batchable):
    ...     handler_name = "analyze_products"
    ...
    ...     def call(self, context: StepContext) -> StepHandlerResult:
    ...         # Count total products to process
    ...         total_products = len(context.input_data.get("product_ids", []))
    ...
    ...         # Create batch outcome with cursor ranges
    ...         outcome = self.create_batch_outcome(
    ...             total_items=total_products,
    ...             batch_size=100,
    ...         )
    ...         return self.batch_analyzer_success(outcome)
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any
from uuid import uuid4

from tasker_core.types import (
    BatchAnalyzerOutcome,
    BatchWorkerContext,
    BatchWorkerOutcome,
    CursorConfig,
    # FFI Boundary Types (TAS-112/TAS-123)
    RustBatchWorkerInputs,
    RustCursorConfig,
    StepHandlerResult,
)
from tasker_core.types import (
    create_batches as create_batches_outcome,
)
from tasker_core.types import (
    no_batches as create_no_batches_outcome,
)

if TYPE_CHECKING:
    from tasker_core.types import StepContext


# =============================================================================
# BatchWorkerConfig - Cross-Language Standard Type (TAS-112)
# =============================================================================


class BatchWorkerConfig:
    """Configuration for a single batch worker.

    Cross-language standard: matches TypeScript's BatchWorkerConfig
    and Ruby's batch_worker_config hash structure.

    This type is returned by `create_cursor_configs()` and used
    to configure individual batch workers with their cursor ranges.

    Attributes:
        batch_id: Unique identifier for this batch (e.g., "001", "002").
        cursor_start: Starting position (inclusive).
        cursor_end: Ending position (exclusive).
        row_count: Number of items in this batch.
        worker_index: Zero-based index of this worker.
        total_workers: Total number of workers.

    Example:
        >>> configs = create_cursor_configs(1000, 3)
        >>> for config in configs:
        ...     print(f"Worker {config.worker_index}: {config.cursor_start}-{config.cursor_end}")
        Worker 0: 0-334
        Worker 1: 334-668
        Worker 2: 668-1000
    """

    def __init__(
        self,
        batch_id: str,
        cursor_start: int,
        cursor_end: int,
        row_count: int,
        worker_index: int,
        total_workers: int,
    ) -> None:
        """Initialize a BatchWorkerConfig.

        Args:
            batch_id: Unique identifier for this batch.
            cursor_start: Starting position (inclusive).
            cursor_end: Ending position (exclusive).
            row_count: Number of items in this batch.
            worker_index: Zero-based index of this worker.
            total_workers: Total number of workers.
        """
        self.batch_id = batch_id
        self.cursor_start = cursor_start
        self.cursor_end = cursor_end
        self.row_count = row_count
        self.worker_index = worker_index
        self.total_workers = total_workers

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for serialization.

        Returns:
            Dictionary representation matching cross-language standard.
        """
        return {
            "batch_id": self.batch_id,
            "cursor_start": self.cursor_start,
            "cursor_end": self.cursor_end,
            "row_count": self.row_count,
            "worker_index": self.worker_index,
            "total_workers": self.total_workers,
        }


class Batchable:
    """Mixin class for batch processing capabilities.

    Add this mixin to StepHandler subclasses to gain batch processing
    helper methods for both analyzer and worker patterns.

    The mixin provides:
    - Cursor range creation for splitting work
    - Batch outcome builders for analyzers
    - Batch context extraction for workers
    - Result aggregation helpers

    Example (Analyzer):
        >>> class ProductBatchAnalyzer(StepHandler, Batchable):
        ...     handler_name = "batch_analyze_products"
        ...
        ...     def call(self, context: StepContext) -> StepHandlerResult:
        ...         total = self.count_products(context)
        ...         outcome = self.create_batch_outcome(total, batch_size=50)
        ...         return self.batch_analyzer_success(outcome)

    Example (Worker):
        >>> class ProductBatchWorker(StepHandler, Batchable):
        ...     handler_name = "batch_process_products"
        ...
        ...     def call(self, context: StepContext) -> StepHandlerResult:
        ...         batch_ctx = self.get_batch_context(context)
        ...         if batch_ctx is None:
        ...             return self.failure("No batch context found")
        ...
        ...         results = []
        ...         for i in range(batch_ctx.start_cursor, batch_ctx.end_cursor):
        ...             results.append(self.process_item(i))
        ...
        ...         outcome = self.create_worker_outcome(
        ...             items_processed=len(results),
        ...             items_succeeded=len(results),
        ...             results=results,
        ...         )
        ...         return self.batch_worker_success(outcome)
    """

    # =========================================================================
    # Cursor Configuration Helpers
    # =========================================================================

    def create_cursor_config(
        self,
        start: int,
        end: int,
        step_size: int = 1,
        metadata: dict[str, Any] | None = None,
    ) -> CursorConfig:
        """Create a cursor configuration for a batch range.

        Args:
            start: Starting cursor position (inclusive).
            end: Ending cursor position (exclusive).
            step_size: Size of each processing step (default: 1).
            metadata: Optional metadata for this cursor range.

        Returns:
            A CursorConfig for the specified range.

        Example:
            >>> config = self.create_cursor_config(0, 100, step_size=10)
        """
        return CursorConfig(
            start_cursor=start,
            end_cursor=end,
            step_size=step_size,
            metadata=metadata or {},
        )

    def create_cursor_ranges(
        self,
        total_items: int,
        batch_size: int,
        step_size: int = 1,
        max_batches: int | None = None,
    ) -> list[CursorConfig]:
        """Create cursor configurations for splitting work into batches.

        Divides the total item count into batch-sized cursor ranges. If max_batches
        is specified, limits the number of batches and adjusts batch sizes accordingly.

        Args:
            total_items: Total number of items to process.
            batch_size: Number of items per batch (may be adjusted if max_batches is set).
            step_size: Size of each processing step within a batch.
            max_batches: Maximum number of batches to create (optional).

        Returns:
            List of CursorConfig objects defining the batch ranges.

        Example:
            >>> # Split 1000 items into batches of 100
            >>> configs = self.create_cursor_ranges(1000, 100)
            >>> len(configs)
            10
            >>> configs[0].start_cursor, configs[0].end_cursor
            (0, 100)

            >>> # Split 1000 items with max 5 batches
            >>> configs = self.create_cursor_ranges(1000, 100, max_batches=5)
            >>> len(configs)
            5
        """
        if total_items == 0:
            return []

        # If max_batches is specified and would create more than max_batches,
        # adjust the batch_size to create exactly max_batches
        if max_batches is not None and max_batches > 0:
            calculated_batches = (total_items + batch_size - 1) // batch_size
            if calculated_batches > max_batches:
                # Recalculate batch_size to create exactly max_batches
                batch_size = (total_items + max_batches - 1) // max_batches

        configs: list[CursorConfig] = []
        start = 0

        while start < total_items:
            end = min(start + batch_size, total_items)
            configs.append(
                CursorConfig(
                    start_cursor=start,
                    end_cursor=end,
                    step_size=step_size,
                )
            )
            start = end

        return configs

    def create_cursor_configs(
        self,
        total_items: int,
        worker_count: int,
    ) -> list[BatchWorkerConfig]:
        """Create cursor configurations for a specific number of workers.

        Ruby-style method that divides items into worker_count roughly equal ranges.
        Uses ceiling division to ensure all items are covered.

        ## Cursor Boundary Math

        1. items_per_worker = ceil(total_items / worker_count)
        2. For worker i (0-indexed):
           - start = i * items_per_worker
           - end = min((i + 1) * items_per_worker, total_items)
           - batch_size = end - start

        Example: 1000 items, 3 workers
          - items_per_worker = ceil(1000/3) = 334
          - Worker 0: start=0, end=334, size=334
          - Worker 1: start=334, end=668, size=334
          - Worker 2: start=668, end=1000, size=332

        Cross-language standard: matches Ruby's create_cursor_configs(total_items, worker_count)
        and TypeScript's createCursorConfigs(totalItems, workerCount).

        Args:
            total_items: Total number of items to process.
            worker_count: Number of workers to create configs for (must be > 0).

        Returns:
            List of BatchWorkerConfig for each worker.

        Raises:
            ValueError: If worker_count <= 0.

        Example:
            >>> configs = self.create_cursor_configs(1000, 3)
            >>> len(configs)
            3
            >>> configs[0].cursor_start, configs[0].cursor_end
            (0, 334)
        """
        if worker_count <= 0:
            msg = "worker_count must be > 0"
            raise ValueError(msg)

        if total_items == 0:
            return []

        # Ceiling division to get items per worker
        items_per_worker = (total_items + worker_count - 1) // worker_count
        configs: list[BatchWorkerConfig] = []

        for i in range(worker_count):
            start_position = i * items_per_worker
            end_position = min((i + 1) * items_per_worker, total_items)

            # Skip if this worker would have no items
            if start_position >= total_items:
                break

            configs.append(
                BatchWorkerConfig(
                    batch_id=f"{i + 1:03d}",
                    cursor_start=start_position,
                    cursor_end=end_position,
                    row_count=end_position - start_position,
                    worker_index=i,
                    total_workers=worker_count,
                )
            )

        return configs

    # =========================================================================
    # FFI Boundary Helpers (TAS-112)
    # =========================================================================

    def get_batch_worker_inputs(
        self,
        context: StepContext,
    ) -> RustBatchWorkerInputs | None:
        """Get Rust batch worker inputs from step context.

        Returns the BatchWorkerInputs structure from workflow_step.inputs,
        which contains cursor config, batch metadata, and no-op flag.

        Cross-language standard: matches Ruby's get_batch_context pattern
        and TypeScript's getBatchWorkerInputs.

        Args:
            context: The step execution context.

        Returns:
            RustBatchWorkerInputs if present, None otherwise.

        Example:
            >>> inputs = self.get_batch_worker_inputs(context)
            >>> if inputs and inputs.is_no_op:
            ...     return self.success({"no_op": True})
            >>> start = inputs.cursor.start_cursor
            >>> end = inputs.cursor.end_cursor
        """
        if not context.step_inputs or len(context.step_inputs) == 0:
            return None

        try:
            return RustBatchWorkerInputs.model_validate(context.step_inputs)
        except Exception:
            # Not valid batch worker inputs
            return None

    def handle_no_op_worker(
        self,
        context: StepContext,
    ) -> StepHandlerResult | None:
        """Handle no-op placeholder worker scenario.

        Returns a success result if the worker is a no-op placeholder
        (created when a batchable step returns NoBatches), otherwise
        returns None to allow normal processing to continue.

        Cross-language standard: matches Ruby's handle_no_op_worker
        and TypeScript's handleNoOpWorker.

        Args:
            context: The step execution context.

        Returns:
            Success result if no-op, None otherwise.

        Example:
            >>> def call(self, context: StepContext) -> StepHandlerResult:
            ...     no_op_result = self.handle_no_op_worker(context)
            ...     if no_op_result:
            ...         return no_op_result
            ...     # ... normal processing
        """
        batch_inputs = self.get_batch_worker_inputs(context)

        if batch_inputs is None or not batch_inputs.is_no_op:
            return None

        from datetime import datetime, timezone

        return StepHandlerResult.success(
            {
                "batch_id": batch_inputs.cursor.batch_id,
                "no_op": True,
                "processed_count": 0,
                "message": "No batches to process",
                "processed_at": datetime.now(timezone.utc).isoformat(),
            }
        )

    # =========================================================================
    # Batch Outcome Builders
    # =========================================================================

    def create_batch_outcome(
        self,
        total_items: int,
        batch_size: int,
        step_size: int = 1,
        batch_metadata: dict[str, Any] | None = None,
    ) -> BatchAnalyzerOutcome:
        """Create a batch analyzer outcome with auto-generated cursor ranges.

        This is a convenience method that combines cursor range creation
        with outcome building.

        Args:
            total_items: Total number of items to process.
            batch_size: Number of items per batch worker.
            step_size: Size of each processing step within a batch.
            batch_metadata: Metadata to pass to all batch workers.

        Returns:
            A BatchAnalyzerOutcome ready to return from an analyzer handler.

        Example:
            >>> outcome = self.create_batch_outcome(
            ...     total_items=5000,
            ...     batch_size=500,
            ...     batch_metadata={"source": "database"}
            ... )
            >>> return self.batch_analyzer_success(outcome)
        """
        cursor_configs = self.create_cursor_ranges(total_items, batch_size, step_size)

        return BatchAnalyzerOutcome(
            cursor_configs=cursor_configs,
            total_items=total_items,
            batch_metadata=batch_metadata or {},
        )

    def create_batch_outcome_from_ranges(
        self,
        ranges: list[tuple[int, int]],
        step_size: int = 1,
        total_items: int | None = None,
        batch_metadata: dict[str, Any] | None = None,
    ) -> BatchAnalyzerOutcome:
        """Create a batch analyzer outcome from explicit cursor ranges.

        Use this when you need custom batch boundaries rather than
        uniform batch sizes.

        Args:
            ranges: List of (start, end) tuples defining batch ranges.
            step_size: Size of each processing step within batches.
            total_items: Total item count (calculated from ranges if not provided).
            batch_metadata: Metadata to pass to all batch workers.

        Returns:
            A BatchAnalyzerOutcome ready to return from an analyzer handler.

        Example:
            >>> # Custom ranges for partitioned data
            >>> ranges = [(0, 500), (500, 800), (800, 1000)]
            >>> outcome = self.create_batch_outcome_from_ranges(ranges)
        """
        return BatchAnalyzerOutcome.from_ranges(
            ranges=ranges,
            step_size=step_size,
            total_items=total_items,
            batch_metadata=batch_metadata,
        )

    def create_worker_outcome(
        self,
        items_processed: int,
        items_succeeded: int = 0,
        items_failed: int = 0,
        items_skipped: int = 0,
        results: list[dict[str, Any]] | None = None,
        errors: list[dict[str, Any]] | None = None,
        last_cursor: int | None = None,
        batch_metadata: dict[str, Any] | None = None,
    ) -> BatchWorkerOutcome:
        """Create a batch worker outcome with processing results.

        Args:
            items_processed: Total items processed in this batch.
            items_succeeded: Items successfully processed.
            items_failed: Items that failed processing.
            items_skipped: Items skipped (e.g., already processed).
            results: Individual item results (optional).
            errors: Error details for failed items.
            last_cursor: Last successfully processed cursor position.
            batch_metadata: Additional result metadata.

        Returns:
            A BatchWorkerOutcome ready to return from a worker handler.

        Example:
            >>> outcome = self.create_worker_outcome(
            ...     items_processed=100,
            ...     items_succeeded=95,
            ...     items_failed=5,
            ...     errors=[{"item_id": i, "error": "..."} for i in failed_ids]
            ... )
        """
        return BatchWorkerOutcome(
            items_processed=items_processed,
            items_succeeded=items_succeeded or items_processed,
            items_failed=items_failed,
            items_skipped=items_skipped,
            results=results or [],
            errors=errors or [],
            last_cursor=last_cursor,
            batch_metadata=batch_metadata or {},
        )

    # =========================================================================
    # Batch Context Helpers
    # =========================================================================

    def get_batch_context(self, context: StepContext) -> BatchWorkerContext | None:
        """Extract batch context from a step context.

        Use this in batch worker handlers to get information about the
        specific batch being processed.

        Args:
            context: The step execution context.

        Returns:
            BatchWorkerContext if batch info exists, None otherwise.

        Example:
            >>> batch_ctx = self.get_batch_context(context)
            >>> if batch_ctx is None:
            ...     return self.failure("No batch context - is this a worker step?")
            >>> for i in range(batch_ctx.start_cursor, batch_ctx.end_cursor):
            ...     self.process_item(i)
        """
        return BatchWorkerContext.from_step_context(context)

    def create_batch_context(
        self,
        cursor_config: CursorConfig,
        batch_index: int,
        total_batches: int,
        batch_metadata: dict[str, Any] | None = None,
    ) -> BatchWorkerContext:
        """Create a batch worker context manually.

        This is useful for testing or when creating batch contexts
        programmatically.

        Args:
            cursor_config: The cursor configuration for this batch.
            batch_index: Index of this batch (0-based).
            total_batches: Total number of batches.
            batch_metadata: Metadata from the analyzer.

        Returns:
            A BatchWorkerContext for the specified batch.
        """
        return BatchWorkerContext(
            batch_id=str(uuid4()),
            cursor_config=cursor_config,
            batch_index=batch_index,
            total_batches=total_batches,
            batch_metadata=batch_metadata or {},
        )

    # =========================================================================
    # Result Helpers (require StepHandler methods)
    # =========================================================================

    def batch_analyzer_success(
        self,
        outcome: BatchAnalyzerOutcome | None = None,
        metadata: dict[str, Any] | None = None,
        worker_template_name: str = "batch_worker",
        # Keyword argument overload (matches Ruby's create_batches_outcome pattern)
        cursor_configs: list[CursorConfig] | None = None,
        total_items: int | None = None,
        batch_metadata: dict[str, Any] | None = None,
    ) -> StepHandlerResult:
        """Create a success result for a batch analyzer.

        Can be called with either a BatchAnalyzerOutcome object or keyword arguments.

        Args:
            outcome: The batch analyzer outcome (alternative to keyword args).
            metadata: Optional additional metadata.
            worker_template_name: Name of the worker template for batch processing.
            cursor_configs: List of cursor configurations (keyword arg alternative).
            total_items: Total items to process (keyword arg alternative).
            batch_metadata: Metadata to pass to workers (keyword arg alternative).

        Returns:
            A success StepHandlerResult with the batch configuration.

        Example using keyword arguments (matches Ruby pattern):
            >>> cursor_configs = self.create_cursor_ranges(1000, 100)
            >>> return self.batch_analyzer_success(
            ...     cursor_configs=cursor_configs,
            ...     total_items=1000,
            ...     worker_template_name="process_csv_batch",
            ...     batch_metadata={"source": "file.csv"},
            ... )

        Example using BatchAnalyzerOutcome:
            >>> outcome = self.create_batch_outcome(total_items=1000, batch_size=100)
            >>> return self.batch_analyzer_success(outcome)
        """
        # Support keyword argument style (matches Ruby pattern)
        if cursor_configs is not None:
            configs_list = cursor_configs
            total = total_items or 0
            batch_meta = batch_metadata or {}
        elif outcome is not None:
            configs_list = outcome.cursor_configs
            total = outcome.total_items or 0
            batch_meta = outcome.batch_metadata
        else:
            # Empty batch case - return no_batches
            return self.no_batches_outcome(reason="no_cursor_configs_provided")

        # Handle empty cursor configs (NoBatches scenario)
        if not configs_list:
            reason = batch_meta.get("reason", "empty_dataset")
            return self.no_batches_outcome(reason=reason, metadata=batch_meta)

        # Convert CursorConfig[] to RustCursorConfig[] (TAS-112/TAS-123)
        rust_cursor_configs = [
            RustCursorConfig(
                batch_id=f"{i + 1:03d}",
                start_cursor=cfg.start_cursor,
                end_cursor=cfg.end_cursor,
                batch_size=cfg.end_cursor - cfg.start_cursor,
            )
            for i, cfg in enumerate(configs_list)
        ]

        # Use typed BatchProcessingOutcome factory (TAS-112/TAS-123)
        batch_processing_outcome = create_batches_outcome(
            worker_template_name=worker_template_name,
            worker_count=len(configs_list),
            cursor_configs=rust_cursor_configs,
            total_items=total,
        )

        result: dict[str, Any] = {
            "batch_processing_outcome": batch_processing_outcome.model_dump(),
            "worker_count": len(configs_list),
            "total_items": total,
        }

        # Merge any batch metadata
        if batch_meta:
            result["batch_metadata"] = batch_meta

        combined_metadata = metadata or {}
        combined_metadata["batch_analyzer"] = True

        # Call the success method from StepHandler (assumes mixin is used with StepHandler)
        return self.success(result, metadata=combined_metadata)  # type: ignore[attr-defined, no-any-return]

    def no_batches_outcome(
        self,
        reason: str,
        metadata: dict[str, Any] | None = None,
    ) -> StepHandlerResult:
        """Create a success result indicating no batches are needed.

        Use this when the analyzer determines that batch processing is not
        required (e.g., empty dataset, data below threshold).

        Uses the typed BatchProcessingOutcome from types.py (TAS-112/TAS-123).

        Args:
            reason: Human-readable reason why no batches are needed.
            metadata: Optional additional metadata.

        Returns:
            A success StepHandlerResult with a no_batches outcome.

        Example:
            >>> if total_items == 0:
            ...     return self.no_batches_outcome(reason="empty_dataset")
        """
        # Use typed BatchProcessingOutcome factory (TAS-112/TAS-123)
        batch_processing_outcome = create_no_batches_outcome()

        result: dict[str, Any] = {
            "batch_processing_outcome": batch_processing_outcome.model_dump(),
            "reason": reason,
        }

        # Merge any additional metadata
        if metadata:
            result.update(metadata)

        combined_metadata = {"batch_analyzer": True, "no_batches": True}

        # Call the success method from StepHandler (assumes mixin is used with StepHandler)
        return self.success(result, metadata=combined_metadata)  # type: ignore[attr-defined, no-any-return]

    def batch_worker_success(
        self,
        outcome: BatchWorkerOutcome | None = None,
        metadata: dict[str, Any] | None = None,
        # Keyword argument overload (matches Ruby pattern)
        items_processed: int | None = None,
        items_succeeded: int | None = None,
        items_failed: int = 0,
        items_skipped: int = 0,
        results: list[dict[str, Any]] | None = None,
        errors: list[dict[str, Any]] | None = None,
        last_cursor: int | None = None,
        batch_metadata: dict[str, Any] | None = None,
    ) -> StepHandlerResult:
        """Create a success result for a batch worker.

        Can be called with either a BatchWorkerOutcome object or keyword arguments.

        Args:
            outcome: The batch worker outcome (alternative to keyword args).
            metadata: Optional additional metadata.
            items_processed: Total items processed (keyword arg alternative).
            items_succeeded: Items successfully processed (keyword arg alternative).
            items_failed: Items that failed processing (keyword arg alternative).
            items_skipped: Items skipped (keyword arg alternative).
            results: Individual item results (keyword arg alternative).
            errors: Error details for failed items (keyword arg alternative).
            last_cursor: Last successfully processed cursor position (keyword arg alternative).
            batch_metadata: Additional batch result metadata (keyword arg alternative).

        Returns:
            A success StepHandlerResult with the batch processing results.

        Example using keyword arguments (matches Ruby pattern):
            >>> return self.batch_worker_success(
            ...     items_processed=100,
            ...     items_succeeded=98,
            ...     items_failed=2,
            ...     results=[{"item_id": i, "status": "ok"} for i in range(98)],
            ... )

        Example using BatchWorkerOutcome:
            >>> outcome = self.create_worker_outcome(items_processed=100, items_succeeded=98)
            >>> return self.batch_worker_success(outcome)
        """
        # Support keyword argument style (matches Ruby pattern)
        if items_processed is not None:
            processed = items_processed
            succeeded = items_succeeded if items_succeeded is not None else items_processed
            failed = items_failed
            skipped = items_skipped
            result_list = results or []
            error_list = errors or []
            cursor = last_cursor
            batch_meta = batch_metadata or {}
        elif outcome is not None:
            processed = outcome.items_processed
            succeeded = outcome.items_succeeded
            failed = outcome.items_failed
            skipped = outcome.items_skipped
            result_list = outcome.results
            error_list = outcome.errors
            cursor = outcome.last_cursor
            batch_meta = outcome.batch_metadata
        else:
            return self.failure(  # type: ignore[attr-defined, no-any-return]
                message="batch_worker_success requires either outcome or items_processed",
                error_type="invalid_args",
                retryable=False,
            )

        result: dict[str, Any] = {
            "items_processed": processed,
            "items_succeeded": succeeded,
            "items_failed": failed,
            "items_skipped": skipped,
            "last_cursor": cursor,
            "batch_metadata": batch_meta,
        }

        # Only include results and errors if they contain data
        if result_list:
            result["results"] = result_list
        if error_list:
            result["errors"] = error_list

        combined_metadata = metadata or {}
        combined_metadata["batch_worker"] = True

        # Call the success method from StepHandler (assumes mixin is used with StepHandler)
        return self.success(result, metadata=combined_metadata)  # type: ignore[attr-defined, no-any-return]

    def batch_worker_partial_failure(
        self,
        outcome: BatchWorkerOutcome,
        message: str | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> StepHandlerResult:
        """Create a success result for a batch worker with partial failures.

        Use this when some items failed but the batch should still be
        considered successful overall (e.g., for soft failures).

        The result is marked as success but includes failure information
        for monitoring and aggregation.

        Args:
            outcome: The batch worker outcome with processing results.
            message: Optional message about the partial failures.
            metadata: Optional additional metadata.

        Returns:
            A success StepHandlerResult with partial failure information.
        """
        result: dict[str, Any] = {
            "items_processed": outcome.items_processed,
            "items_succeeded": outcome.items_succeeded,
            "items_failed": outcome.items_failed,
            "items_skipped": outcome.items_skipped,
            "last_cursor": outcome.last_cursor,
            "batch_metadata": outcome.batch_metadata,
            "partial_failure": True,
        }

        if message:
            result["partial_failure_message"] = message

        if outcome.results:
            result["results"] = outcome.results
        if outcome.errors:
            result["errors"] = outcome.errors

        combined_metadata = metadata or {}
        combined_metadata["batch_worker"] = True
        combined_metadata["had_failures"] = True

        return self.success(result, metadata=combined_metadata)  # type: ignore[attr-defined, no-any-return]

    # =========================================================================
    # Aggregation Helpers
    # =========================================================================

    @staticmethod
    def aggregate_worker_results(
        worker_results: list[dict[str, Any]],
    ) -> dict[str, Any]:
        """Aggregate results from multiple batch workers.

        Use this in an aggregator step to combine results from all
        batch workers into a single summary.

        Args:
            worker_results: List of results from batch worker steps.

        Returns:
            Aggregated summary of all batch processing.

        Example:
            >>> # In an aggregator handler
            >>> worker_results = [
            ...     context.get_dependency_result(f"worker_{i}")
            ...     for i in range(batch_count)
            ... ]
            >>> summary = Batchable.aggregate_worker_results(worker_results)
            >>> return self.success(summary)
        """
        total_processed = 0
        total_succeeded = 0
        total_failed = 0
        total_skipped = 0
        all_errors: list[dict[str, Any]] = []

        for result in worker_results:
            if result is None:
                continue

            total_processed += result.get("items_processed", 0)
            total_succeeded += result.get("items_succeeded", 0)
            total_failed += result.get("items_failed", 0)
            total_skipped += result.get("items_skipped", 0)

            if "errors" in result and result["errors"]:
                all_errors.extend(result["errors"])

        return {
            "total_processed": total_processed,
            "total_succeeded": total_succeeded,
            "total_failed": total_failed,
            "total_skipped": total_skipped,
            "batch_count": len(worker_results),
            "success_rate": (total_succeeded / total_processed if total_processed > 0 else 0.0),
            "errors": all_errors[:100] if all_errors else [],  # Limit errors
            "error_count": len(all_errors),
        }


__all__ = ["Batchable", "BatchWorkerConfig"]
