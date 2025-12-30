"""Batch processing module.

This module provides utilities for implementing batch processing handlers,
including the Batchable mixin and batch context classes.

Classes:
    Batchable: Mixin class for handlers that support batch processing.

Example:
    >>> from tasker_core.step_handler import StepHandler
    >>> from tasker_core.batch_processing import Batchable
    >>>
    >>> class ProductAnalyzer(StepHandler, Batchable):
    ...     handler_name = "analyze_products"
    ...
    ...     def call(self, context):
    ...         # Analyze total items and create batch ranges
    ...         total_items = self.count_items(context)
    ...         outcome = self.create_batch_outcome(
    ...             total_items=total_items,
    ...             batch_size=100
    ...         )
    ...         return self.batch_analyzer_success(outcome)
"""

from __future__ import annotations

from tasker_core.batch_processing.batchable import Batchable, BatchWorkerConfig

__all__ = [
    "Batchable",
    "BatchWorkerConfig",
]
