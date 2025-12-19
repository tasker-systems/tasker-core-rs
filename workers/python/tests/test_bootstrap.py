"""Bootstrap and FFI function tests.

These tests verify:
- Bootstrap function behavior without actual bootstrap
- Worker status when not running
- FFI dispatch functions behavior when worker is not running
"""

from __future__ import annotations

from uuid import uuid4

import pytest


class TestBootstrapFunctions:
    """Test bootstrap function behavior without actual bootstrap."""

    def test_is_worker_running_false_initially(self):
        """Test is_worker_running returns False when not bootstrapped."""
        from tasker_core import is_worker_running

        assert is_worker_running() is False

    def test_stop_worker_when_not_running(self):
        """Test stop_worker handles not-running state."""
        from tasker_core import stop_worker

        result = stop_worker()
        assert result == "Worker system not running"

    def test_get_worker_status_when_not_running(self):
        """Test get_worker_status when worker is not running."""
        from tasker_core import get_worker_status

        status = get_worker_status()
        assert status.running is False
        assert status.error == "Worker system not initialized"


class TestFFIFunctionsWithoutWorker:
    """Test FFI dispatch functions when worker is not running."""

    def test_poll_step_events_raises_when_not_running(self):
        """Test poll_step_events raises when worker not running."""
        from tasker_core import poll_step_events

        # Should raise RuntimeError when worker not initialized
        with pytest.raises(RuntimeError):
            poll_step_events()

    def test_complete_step_event_raises_when_not_running(self):
        """Test complete_step_event raises when worker not running."""
        from tasker_core import complete_step_event

        with pytest.raises(RuntimeError):
            complete_step_event(str(uuid4()), {})

    def test_get_ffi_dispatch_metrics_raises_when_not_running(self):
        """Test get_ffi_dispatch_metrics raises when worker not running."""
        from tasker_core import get_ffi_dispatch_metrics

        with pytest.raises(RuntimeError):
            get_ffi_dispatch_metrics()

    def test_check_starvation_warnings_raises_when_not_running(self):
        """Test check_starvation_warnings raises when worker not running."""
        from tasker_core import check_starvation_warnings

        with pytest.raises(RuntimeError):
            check_starvation_warnings()

    def test_cleanup_timeouts_raises_when_not_running(self):
        """Test cleanup_timeouts raises when worker not running."""
        from tasker_core import cleanup_timeouts

        with pytest.raises(RuntimeError):
            cleanup_timeouts()
