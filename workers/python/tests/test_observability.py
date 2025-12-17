"""Observability tests.

These tests verify:
- Observability FFI functions behavior when worker is not running
- Health check, metrics, and config endpoints
- poll_in_process_events behavior
"""

from __future__ import annotations

import pytest

from tasker_core import (
    WorkerNotInitializedError,
    get_health_check,
    get_metrics,
    get_worker_config,
    poll_in_process_events,
)


class TestObservabilityFunctionsWithoutWorker:
    """Test observability functions when worker is not running."""

    def test_poll_in_process_events_without_worker(self):
        """poll_in_process_events returns None or raises when worker not running."""
        # This should either return None or raise
        try:
            result = poll_in_process_events()
            # If it doesn't raise, result should be None
            assert result is None
        except RuntimeError:
            # Expected when worker not initialized
            pass

    def test_get_health_check_without_worker(self):
        """get_health_check raises when worker not running."""
        with pytest.raises(WorkerNotInitializedError):
            get_health_check()

    def test_get_metrics_without_worker(self):
        """get_metrics raises when worker not running."""
        with pytest.raises(WorkerNotInitializedError):
            get_metrics()

    def test_get_worker_config_without_worker(self):
        """get_worker_config raises when worker not running."""
        with pytest.raises(WorkerNotInitializedError):
            get_worker_config()
