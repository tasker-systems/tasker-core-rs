"""Exception hierarchy and logging tests.

These tests verify:
- TaskerError is the base exception class
- All custom exceptions inherit correctly
- Exceptions can be caught by base class
- Logging functions are callable and accept context
"""

from __future__ import annotations

import pytest


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
