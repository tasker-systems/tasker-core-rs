"""Step handlers for batch processing workflow.

This module implements step handlers for CSV batch processing using the
Batchable mixin to create cursor-based batch workers.

Workflow:
1. CsvAnalyzerHandler: Analyzes CSV and creates batch configurations
2. CsvBatchProcessorHandler: Processes rows within a cursor range
3. CsvResultsAggregatorHandler: Aggregates results from all batch workers
"""

from __future__ import annotations

import csv
import os
from typing import TYPE_CHECKING, Any

from tasker_core import StepHandler, StepHandlerResult
from tasker_core.batch_processing import Batchable

if TYPE_CHECKING:
    from tasker_core import StepContext


class CsvAnalyzerHandler(StepHandler, Batchable):
    """Analyze CSV file and create batch worker configurations.

    This handler reads the CSV file to count rows and creates cursor
    configurations for parallel batch workers.
    """

    handler_name = "batch_processing.step_handlers.CsvAnalyzerHandler"
    handler_version = "1.0.0"

    # Default batch configuration
    DEFAULT_BATCH_SIZE = 200
    DEFAULT_MAX_WORKERS = 5

    def call(self, context: StepContext) -> StepHandlerResult:
        """Analyze the CSV and create batch configurations."""
        input_data = context.input_data

        csv_file_path = input_data.get("csv_file_path")
        if not csv_file_path:
            return StepHandlerResult.failure(
                message="csv_file_path is required",
                error_type="validation_error",
                retryable=False,
            )

        # Get configuration from step_config (handler initialization)
        step_config = context.step_config or {}
        batch_size = step_config.get("batch_size", self.DEFAULT_BATCH_SIZE)
        max_workers = step_config.get("max_workers", self.DEFAULT_MAX_WORKERS)

        # Check if file exists and count rows
        if not os.path.exists(csv_file_path):
            # Handle empty CSV scenario
            return self.batch_analyzer_success(
                cursor_configs=[],
                total_items=0,
                batch_metadata={
                    "csv_file_path": csv_file_path,
                    "file_exists": False,
                    "reason": "File not found",
                },
            )

        # Count CSV rows (excluding header)
        try:
            with open(csv_file_path, encoding="utf-8") as f:
                reader = csv.reader(f)
                next(reader)  # Skip header
                row_count = sum(1 for _ in reader)
        except Exception as e:
            return StepHandlerResult.failure(
                message=f"Failed to read CSV file: {e}",
                error_type="file_error",
                retryable=True,
            )

        # Handle empty CSV
        if row_count == 0:
            return self.batch_analyzer_success(
                cursor_configs=[],
                total_items=0,
                batch_metadata={
                    "csv_file_path": csv_file_path,
                    "file_exists": True,
                    "reason": "No data rows",
                },
            )

        # Create cursor configurations for batch workers
        cursor_configs = self.create_cursor_ranges(
            total_items=row_count,
            batch_size=batch_size,
            max_batches=max_workers,
        )

        return self.batch_analyzer_success(
            cursor_configs=cursor_configs,
            total_items=row_count,
            worker_template_name="process_csv_batch_py",
            batch_metadata={
                "csv_file_path": csv_file_path,
                "batch_size": batch_size,
                "analysis_mode": input_data.get("analysis_mode", "default"),
            },
        )


class CsvBatchProcessorHandler(StepHandler, Batchable):
    """Process a batch of CSV rows based on cursor range.

    Each instance processes rows from start_cursor to end_cursor.
    """

    handler_name = "batch_processing.step_handlers.CsvBatchProcessorHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Process a batch of CSV rows."""
        # Get cursor config from step_inputs (from workflow_step.inputs)
        # This matches Ruby's BatchWorkerContext.from_step_data pattern
        step_inputs = context.step_inputs or {}

        # Check if this is a no-op placeholder worker
        if step_inputs.get("is_no_op"):
            return self.batch_worker_success(
                items_processed=0,
                items_succeeded=0,
                items_failed=0,
                results=[],
                errors=[],
                last_cursor=0,
                metadata={"no_op": True},
            )

        cursor = step_inputs.get("cursor", {})
        start_cursor = cursor.get("start_cursor", 0)
        end_cursor = cursor.get("end_cursor", 0)
        batch_id = cursor.get("batch_id", "unknown")

        # Get the CSV file path from analyzer result
        analyzer_result = context.get_dependency_result("analyze_csv_py")
        if analyzer_result is None:
            return StepHandlerResult.failure(
                message="Missing analyze_csv_py result",
                error_type="dependency_error",
                retryable=True,
            )

        batch_metadata = analyzer_result.get("batch_metadata", {})
        csv_file_path = batch_metadata.get("csv_file_path")

        if not csv_file_path:
            return StepHandlerResult.failure(
                message="csv_file_path not found in batch metadata",
                error_type="dependency_error",
                retryable=False,
            )

        # Process the batch
        try:
            results = self._process_csv_batch(csv_file_path, start_cursor, end_cursor)
        except Exception as e:
            return StepHandlerResult.failure(
                message=f"Batch processing failed: {e}",
                error_type="processing_error",
                retryable=True,
            )

        return self.batch_worker_success(
            items_processed=results["items_processed"],
            items_succeeded=results["items_succeeded"],
            items_failed=results["items_failed"],
            results=results["results"],
            errors=results.get("errors", []),
            last_cursor=end_cursor,
            metadata={
                "batch_id": batch_id,
                "batch_index": step_inputs.get("batch_metadata", {}).get("batch_index", 0),
            },
        )

    def _process_csv_batch(
        self, csv_file_path: str, start_cursor: int, end_cursor: int
    ) -> dict[str, Any]:
        """Process rows from start_cursor to end_cursor.

        Args:
            csv_file_path: Path to the CSV file.
            start_cursor: Starting row index (1-based, excluding header).
            end_cursor: Ending row index (inclusive).

        Returns:
            Dictionary with processing results.
        """
        results: list[dict[str, Any]] = []
        errors: list[dict[str, Any]] = []
        items_processed = 0
        items_succeeded = 0
        items_failed = 0

        with open(csv_file_path, encoding="utf-8") as f:
            reader = csv.DictReader(f)

            for row_num, row in enumerate(reader, start=1):
                # Only process rows in our cursor range
                # Cursors are 0-based end-exclusive: [start, end) maps to rows (start, end]
                # So row 1 is processed when start_cursor=0 (1 > 0), row 200 when end_cursor=200 (200 <= 200)
                if row_num <= start_cursor:
                    continue
                if row_num > end_cursor:
                    break

                items_processed += 1

                try:
                    # Calculate inventory value (price * stock) - matches Ruby handler
                    price = float(row.get("price", 0))
                    stock = int(row.get("stock", 0))
                    inventory_value = price * stock

                    results.append(
                        {
                            "row_number": row_num,
                            "product_id": row.get("id", f"PROD-{row_num}"),
                            "inventory_value": inventory_value,
                            "price": price,
                            "stock": stock,
                        }
                    )
                    items_succeeded += 1

                except (ValueError, KeyError) as e:
                    items_failed += 1
                    errors.append(
                        {
                            "row_number": row_num,
                            "error": str(e),
                        }
                    )

        return {
            "items_processed": items_processed,
            "items_succeeded": items_succeeded,
            "items_failed": items_failed,
            "results": results,
            "errors": errors,
        }


class CsvResultsAggregatorHandler(StepHandler, Batchable):
    """Aggregate results from all batch workers.

    This is a deferred convergence step that waits for all batch workers
    to complete, then aggregates their results.
    """

    handler_name = "batch_processing.step_handlers.CsvResultsAggregatorHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Aggregate results from all batch workers."""
        # Collect results from all batch workers
        worker_results: list[dict[str, Any]] = []

        # Look for batch worker results (process_csv_batch_001, _002, etc.)
        # Use get_dependency_result() to unwrap the {"result": {...}} structure
        for dep_name in context.dependency_results:
            if dep_name.startswith("process_csv_batch_"):
                unwrapped = context.get_dependency_result(dep_name)
                if unwrapped is not None and isinstance(unwrapped, dict):
                    worker_results.append(unwrapped)

        # Use the aggregate helper for counts
        aggregated = self.aggregate_worker_results(worker_results)

        # Calculate inventory metrics from individual worker results
        # Each worker result contains a "results" array with item-level data
        total_inventory_value = 0.0
        for worker_result in worker_results:
            for item_result in worker_result.get("results", []):
                total_inventory_value += item_result.get("inventory_value", 0.0)

        # aggregate_worker_results returns "batch_count", not "worker_count"
        worker_count = aggregated.get("batch_count", len(worker_results))

        return StepHandlerResult.success(
            {
                "total_processed": aggregated.get("total_processed", 0),
                "total_succeeded": aggregated.get("total_succeeded", 0),
                "total_failed": aggregated.get("total_failed", 0),
                "worker_count": worker_count,
                "total_inventory_value": total_inventory_value,
                "result": {
                    "total_processed": aggregated.get("total_processed", 0),
                    "total_inventory_value": total_inventory_value,
                    "worker_count": worker_count,
                },
            }
        )


__all__ = [
    "CsvAnalyzerHandler",
    "CsvBatchProcessorHandler",
    "CsvResultsAggregatorHandler",
]
