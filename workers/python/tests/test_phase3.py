"""Phase 3 (TAS-82) Event Dispatch System tests.

These tests verify:
- Pydantic models for step events and results
- FFI dispatch functions behavior (without worker running)
- EventPoller class functionality
"""

from __future__ import annotations

from uuid import uuid4

import pytest


class TestEventDispatchTypes:
    """Test Pydantic models for event dispatch."""

    def test_result_status_enum(self):
        """Test ResultStatus enum values."""
        from tasker_core import ResultStatus

        assert ResultStatus.SUCCESS == "completed"
        assert ResultStatus.FAILURE == "error"
        assert ResultStatus.RETRYABLE_ERROR == "retryable_error"
        assert ResultStatus.PERMANENT_ERROR == "permanent_error"

    def test_step_error_model(self):
        """Test StepError model."""
        from tasker_core import StepError

        error = StepError(
            error_type="ValidationError",
            message="Invalid input data",
            retryable=False,
            metadata={"field": "email"},
        )
        assert error.error_type == "ValidationError"
        assert error.message == "Invalid input data"
        assert error.retryable is False
        assert error.metadata["field"] == "email"
        assert error.stack_trace is None

    def test_step_error_with_stack_trace(self):
        """Test StepError with stack trace."""
        from tasker_core import StepError

        error = StepError(
            error_type="RuntimeError",
            message="Something went wrong",
            stack_trace="File 'foo.py', line 10\n  raise RuntimeError()",
            retryable=True,
        )
        assert error.stack_trace is not None
        assert "line 10" in error.stack_trace

    def test_step_execution_result_model(self):
        """Test StepExecutionResult model."""
        from tasker_core import ResultStatus, StepExecutionResult

        step_uuid = uuid4()
        task_uuid = uuid4()

        result = StepExecutionResult(
            step_uuid=step_uuid,
            task_uuid=task_uuid,
            success=True,
            status=ResultStatus.SUCCESS.value,
            execution_time_ms=150,
            result={"processed": 100},
            worker_id="python-worker-1",
        )
        assert result.success is True
        assert result.status == "completed"
        assert result.execution_time_ms == 150
        assert result.result["processed"] == 100
        assert result.error is None

    def test_step_execution_result_success_factory(self):
        """Test StepExecutionResult.success_result factory method."""
        from tasker_core import StepExecutionResult

        step_uuid = uuid4()
        task_uuid = uuid4()

        result = StepExecutionResult.success_result(
            step_uuid=step_uuid,
            task_uuid=task_uuid,
            result={"data": "test"},
            execution_time_ms=100,
            worker_id="test-worker",
        )
        assert result.success is True
        assert result.status == "completed"
        assert result.result["data"] == "test"
        assert result.worker_id == "test-worker"

    def test_step_execution_result_failure_factory(self):
        """Test StepExecutionResult.failure_result factory method."""
        from tasker_core import StepError, StepExecutionResult

        step_uuid = uuid4()
        task_uuid = uuid4()

        error = StepError(
            error_type="TimeoutError",
            message="Operation timed out",
            retryable=True,
        )
        result = StepExecutionResult.failure_result(
            step_uuid=step_uuid,
            task_uuid=task_uuid,
            error=error,
            execution_time_ms=30000,
            worker_id="test-worker",
        )
        assert result.success is False
        assert result.status == "retryable_error"
        assert result.error is not None
        assert result.error.retryable is True

    def test_step_execution_result_permanent_failure(self):
        """Test StepExecutionResult with permanent error."""
        from tasker_core import StepError, StepExecutionResult

        step_uuid = uuid4()
        task_uuid = uuid4()

        error = StepError(
            error_type="ValidationError",
            message="Invalid data",
            retryable=False,
        )
        result = StepExecutionResult.failure_result(
            step_uuid=step_uuid,
            task_uuid=task_uuid,
            error=error,
            execution_time_ms=50,
            worker_id="test-worker",
        )
        assert result.success is False
        assert result.status == "permanent_error"

    def test_ffi_step_event_model(self):
        """Test FfiStepEvent model."""
        from tasker_core import FfiStepEvent

        event = FfiStepEvent(
            event_id=str(uuid4()),
            task_uuid=str(uuid4()),
            step_uuid=str(uuid4()),
            correlation_id=str(uuid4()),
            task_sequence_step={"step": {"name": "test_step"}},
        )
        assert event.event_id is not None
        assert event.task_sequence_step["step"]["name"] == "test_step"
        assert event.trace_id is None

    def test_ffi_step_event_immutable(self):
        """Test that FfiStepEvent is frozen (immutable)."""
        from tasker_core import FfiStepEvent

        event = FfiStepEvent(
            event_id=str(uuid4()),
            task_uuid=str(uuid4()),
            step_uuid=str(uuid4()),
            correlation_id=str(uuid4()),
            task_sequence_step={},
        )
        # Pydantic frozen models raise ValidationError on modification
        from pydantic import ValidationError

        with pytest.raises(ValidationError):
            event.event_id = "new-id"

    def test_ffi_dispatch_metrics_model(self):
        """Test FfiDispatchMetrics model."""
        from tasker_core import FfiDispatchMetrics

        metrics = FfiDispatchMetrics(
            pending_count=5,
            oldest_pending_age_ms=1000,
            newest_pending_age_ms=100,
            oldest_event_id=str(uuid4()),
            starvation_detected=False,
            starving_event_count=0,
        )
        assert metrics.pending_count == 5
        assert metrics.oldest_pending_age_ms == 1000
        assert metrics.starvation_detected is False

    def test_ffi_dispatch_metrics_defaults(self):
        """Test FfiDispatchMetrics with defaults."""
        from tasker_core import FfiDispatchMetrics

        metrics = FfiDispatchMetrics(pending_count=0)
        assert metrics.pending_count == 0
        assert metrics.oldest_pending_age_ms is None
        assert metrics.starvation_detected is False
        assert metrics.starving_event_count == 0

    def test_starvation_warning_model(self):
        """Test StarvationWarning model."""
        from tasker_core import StarvationWarning

        warning = StarvationWarning(
            event_id=uuid4(),
            step_uuid=uuid4(),
            pending_duration_ms=15000,
            threshold_ms=10000,
        )
        assert warning.pending_duration_ms == 15000
        assert warning.threshold_ms == 10000


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


class TestEventPoller:
    """Test EventPoller class functionality."""

    def test_event_poller_init(self):
        """Test EventPoller initialization."""
        from tasker_core import EventPoller

        poller = EventPoller(
            polling_interval_ms=20,
            starvation_check_interval=50,
            cleanup_interval=100,
        )
        assert poller._polling_interval == 0.020
        assert poller._starvation_check_interval == 50
        assert poller._cleanup_interval == 100
        assert not poller.is_running

    def test_event_poller_default_values(self):
        """Test EventPoller default initialization."""
        from tasker_core import EventPoller

        poller = EventPoller()
        assert poller._polling_interval == 0.010  # 10ms
        assert poller._starvation_check_interval == 100
        assert poller._cleanup_interval == 1000

    def test_event_poller_callback_registration(self):
        """Test registering callbacks with EventPoller."""
        from tasker_core import EventPoller

        poller = EventPoller()
        events_received = []
        errors_received = []

        poller.on_step_event(lambda e: events_received.append(e))
        poller.on_error(lambda e: errors_received.append(e))

        assert len(poller._step_event_callbacks) == 1
        assert len(poller._error_callbacks) == 1

    def test_event_poller_multiple_callbacks(self):
        """Test registering multiple callbacks."""
        from tasker_core import EventPoller

        poller = EventPoller()
        callback1_called = []
        callback2_called = []

        poller.on_step_event(lambda e: callback1_called.append(e))
        poller.on_step_event(lambda e: callback2_called.append(e))

        assert len(poller._step_event_callbacks) == 2

    def test_event_poller_start_raises_if_already_running(self):
        """Test that starting twice raises RuntimeError."""
        from tasker_core import EventPoller

        poller = EventPoller()
        # We can't actually start without a running worker,
        # but we can test the flag logic
        poller._running = True

        with pytest.raises(RuntimeError, match="already running"):
            poller.start()

        # Reset for cleanup
        poller._running = False

    def test_event_poller_stop_when_not_running(self):
        """Test stop() is safe when poller not running."""
        from tasker_core import EventPoller

        poller = EventPoller()
        # Should not raise
        poller.stop()
        assert not poller.is_running

    def test_event_poller_is_running_property(self):
        """Test is_running property."""
        from tasker_core import EventPoller

        poller = EventPoller()
        assert poller.is_running is False

        # Simulate running state
        poller._running = True
        # Still False because no thread
        assert poller.is_running is False

        poller._running = False

    def test_event_poller_get_metrics_without_worker(self):
        """Test get_metrics returns None without worker running."""
        from tasker_core import EventPoller

        poller = EventPoller()
        metrics = poller.get_metrics()
        assert metrics is None


class TestModuleExportsPhase3:
    """Test that Phase 3 exports are available."""

    def test_phase3_ffi_functions_exported(self):
        """Test Phase 3 FFI functions are exported."""
        from tasker_core import (
            check_starvation_warnings,
            cleanup_timeouts,
            complete_step_event,
            get_ffi_dispatch_metrics,
            poll_step_events,
        )

        assert callable(poll_step_events)
        assert callable(complete_step_event)
        assert callable(get_ffi_dispatch_metrics)
        assert callable(check_starvation_warnings)
        assert callable(cleanup_timeouts)

    def test_phase3_types_exported(self):
        """Test Phase 3 types are exported."""
        from tasker_core import (
            FfiDispatchMetrics,
            FfiStepEvent,
            ResultStatus,
            StarvationWarning,
            StepError,
            StepExecutionResult,
        )

        assert FfiStepEvent is not None
        assert StepExecutionResult is not None
        assert StepError is not None
        assert ResultStatus is not None
        assert FfiDispatchMetrics is not None
        assert StarvationWarning is not None

    def test_event_poller_exported(self):
        """Test EventPoller is exported."""
        from tasker_core import EventPoller

        assert EventPoller is not None
        assert callable(EventPoller)

    def test_all_exports_includes_phase3(self):
        """Test __all__ includes Phase 3 exports."""
        import tasker_core

        phase3_exports = {
            "poll_step_events",
            "complete_step_event",
            "get_ffi_dispatch_metrics",
            "check_starvation_warnings",
            "cleanup_timeouts",
            "FfiStepEvent",
            "StepExecutionResult",
            "StepError",
            "ResultStatus",
            "FfiDispatchMetrics",
            "StarvationWarning",
            "EventPoller",
        }

        assert phase3_exports.issubset(set(tasker_core.__all__))
