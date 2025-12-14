"""Phase 2 (TAS-72-P2) functionality tests.

These tests verify:
- Pydantic models work correctly
- Exception hierarchy is correct
- Logging functions are callable
- Bootstrap functions handle uninitialized state correctly
"""

from __future__ import annotations

import pytest


class TestPydanticModels:
    """Test Pydantic model validation and serialization."""

    def test_bootstrap_config_defaults(self):
        """Test BootstrapConfig with default values."""
        from tasker_core import BootstrapConfig

        config = BootstrapConfig()
        assert config.worker_id is None
        assert config.namespace == "default"
        assert config.config_path is None
        assert config.log_level == "info"

    def test_bootstrap_config_custom_values(self):
        """Test BootstrapConfig with custom values."""
        from tasker_core import BootstrapConfig

        config = BootstrapConfig(
            worker_id="my-worker",
            namespace="payments",
            log_level="debug",
        )
        assert config.worker_id == "my-worker"
        assert config.namespace == "payments"
        assert config.log_level == "debug"

    def test_bootstrap_config_invalid_log_level(self):
        """Test BootstrapConfig rejects invalid log level."""
        from pydantic import ValidationError

        from tasker_core import BootstrapConfig

        with pytest.raises(ValidationError):
            BootstrapConfig(log_level="invalid")

    def test_bootstrap_config_extra_fields_forbidden(self):
        """Test BootstrapConfig rejects extra fields."""
        from pydantic import ValidationError

        from tasker_core import BootstrapConfig

        with pytest.raises(ValidationError):
            BootstrapConfig(unknown_field="value")

    def test_bootstrap_result_model(self):
        """Test BootstrapResult model."""
        from tasker_core import BootstrapResult

        result = BootstrapResult(
            success=True,
            handle_id="abc-123",
            worker_id="python-worker-abc",
            status="started",
            message="Worker started",
        )
        assert result.success is True
        assert result.handle_id == "abc-123"
        assert result.error_message is None

    def test_worker_status_model(self):
        """Test WorkerStatus model."""
        from tasker_core import WorkerStatus

        # Test with minimal data (not running)
        status = WorkerStatus(running=False, error="Not initialized")
        assert status.running is False
        assert status.error == "Not initialized"
        assert status.environment is None

        # Test with full data (running)
        status = WorkerStatus(
            running=True,
            environment="test",
            worker_core_status="Running",
            web_api_enabled=True,
            supported_namespaces=["default", "payments"],
            database_pool_size=10,
            database_pool_idle=5,
        )
        assert status.running is True
        assert status.database_pool_size == 10

    def test_worker_state_enum(self):
        """Test WorkerState enum values."""
        from tasker_core import WorkerState

        assert WorkerState.STARTING == "starting"
        assert WorkerState.RUNNING == "running"
        assert WorkerState.SHUTTING_DOWN == "shutting_down"
        assert WorkerState.STOPPED == "stopped"
        assert WorkerState.ERROR == "error"

    def test_step_handler_call_result(self):
        """Test StepHandlerCallResult model."""
        from tasker_core import StepHandlerCallResult

        result = StepHandlerCallResult(
            success=True,
            result={"processed": 100},
            metadata={"duration_ms": 150},
        )
        assert result.success is True
        assert result.result["processed"] == 100
        assert result.error_message is None

    def test_log_context_model(self):
        """Test LogContext model."""
        from tasker_core import LogContext

        context = LogContext(
            correlation_id="abc-123",
            task_uuid="task-456",
            operation="process",
        )
        assert context.correlation_id == "abc-123"
        assert context.step_uuid is None

        # Test model_dump excludes None values
        dump = context.model_dump()
        assert "correlation_id" in dump
        assert dump["step_uuid"] is None


class TestExceptionHierarchy:
    """Test exception class hierarchy."""

    def test_tasker_error_is_base(self):
        """Test TaskerError is the base class."""
        from tasker_core import (
            ConversionError,
            FFIError,
            TaskerError,
            WorkerAlreadyRunningError,
            WorkerBootstrapError,
            WorkerNotInitializedError,
        )

        assert issubclass(WorkerNotInitializedError, TaskerError)
        assert issubclass(WorkerBootstrapError, TaskerError)
        assert issubclass(WorkerAlreadyRunningError, TaskerError)
        assert issubclass(FFIError, TaskerError)
        assert issubclass(ConversionError, TaskerError)

    def test_exceptions_inherit_from_exception(self):
        """Test all exceptions inherit from Exception."""
        from tasker_core import (
            ConversionError,
            FFIError,
            TaskerError,
            WorkerAlreadyRunningError,
            WorkerBootstrapError,
            WorkerNotInitializedError,
        )

        for exc_class in [
            TaskerError,
            WorkerNotInitializedError,
            WorkerBootstrapError,
            WorkerAlreadyRunningError,
            FFIError,
            ConversionError,
        ]:
            assert issubclass(exc_class, Exception)

    def test_can_catch_by_base_class(self):
        """Test exceptions can be caught by base class."""
        from tasker_core import TaskerError, WorkerBootstrapError

        with pytest.raises(TaskerError):
            raise WorkerBootstrapError("Test error")


class TestLogging:
    """Test logging functions."""

    def test_log_info_callable(self):
        """Test log_info is callable without raising."""
        from tasker_core import log_info

        # Should not raise
        log_info("Test message")
        log_info("Test with fields", {"key": "value"})

    def test_log_error_callable(self):
        """Test log_error is callable without raising."""
        from tasker_core import log_error

        log_error("Error message")
        log_error("Error with context", {"correlation_id": "abc-123"})

    def test_log_warn_callable(self):
        """Test log_warn is callable without raising."""
        from tasker_core import log_warn

        log_warn("Warning message")

    def test_log_debug_callable(self):
        """Test log_debug is callable without raising."""
        from tasker_core import log_debug

        log_debug("Debug message")

    def test_log_trace_callable(self):
        """Test log_trace is callable without raising."""
        from tasker_core import log_trace

        log_trace("Trace message")

    def test_log_with_log_context(self):
        """Test logging with LogContext model."""
        from tasker_core import LogContext, log_info

        context = LogContext(
            correlation_id="abc-123",
            task_uuid="task-456",
            operation="test",
        )
        # Should not raise
        log_info("Test with context", context)


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
