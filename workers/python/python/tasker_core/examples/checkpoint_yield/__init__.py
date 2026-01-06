"""TAS-125: Checkpoint Yield E2E Test Handlers.

This module provides step handlers for testing the TAS-125 checkpoint yielding
functionality. These handlers exercise the checkpoint_yield() mechanism that
allows batch processing handlers to persist progress and be re-dispatched.

Test Pattern:
1. CheckpointYieldAnalyzerHandler creates a single batch (configurable total_items)
2. CheckpointYieldWorkerHandler processes items, yielding checkpoints every N items
3. On transient failure, step resumes from last checkpoint
4. CheckpointYieldAggregatorHandler collects final output

Configuration (via task context):
  - total_items: Number of items to process (default: 100)
  - items_per_checkpoint: Items to process before yielding checkpoint (default: 25)
  - fail_after_items: Fail after processing this many items (optional)
  - fail_on_attempt: Only fail on this attempt number (optional, default: 1)
  - permanent_failure: If true, fail with non-retryable error (default: false)
"""

from tasker_core.examples.checkpoint_yield.handlers import (
    CheckpointYieldAggregatorHandler,
    CheckpointYieldAnalyzerHandler,
    CheckpointYieldWorkerHandler,
)

__all__ = [
    "CheckpointYieldAnalyzerHandler",
    "CheckpointYieldWorkerHandler",
    "CheckpointYieldAggregatorHandler",
]
