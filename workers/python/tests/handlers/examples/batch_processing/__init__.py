"""Batch processing workflow handlers.

This package contains step handlers for the batch processing workflow
demonstrating the Batchable mixin with cursor-based CSV row processing.

Workflow Pattern:
1. analyze_csv (batchable): Analyzes CSV file and creates batch workers
2. process_csv_batch_001...N (batch_worker): Process N rows of CSV data
3. aggregate_csv_results (deferred_convergence): Aggregate results from all workers
"""

from __future__ import annotations

from .step_handlers import (
    CsvAnalyzerHandler,
    CsvBatchProcessorHandler,
    CsvResultsAggregatorHandler,
)

__all__ = [
    "CsvAnalyzerHandler",
    "CsvBatchProcessorHandler",
    "CsvResultsAggregatorHandler",
]
