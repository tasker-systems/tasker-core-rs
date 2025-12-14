"""
Tasker Core Python Worker

This package provides Python bindings for the tasker-core workflow
orchestration system, enabling Python step handlers to integrate
with Rust-based orchestration.

Example:
    >>> import tasker_core
    >>> tasker_core.version()
    '0.1.0'
    >>> tasker_core.health_check()
    True

    >>> # Bootstrap the worker system (Phase 2+)
    >>> result = tasker_core.bootstrap_worker()
    >>> print(f"Worker started: {result.worker_id}")

    >>> # Use structured logging
    >>> tasker_core.log_info("Processing started", {"correlation_id": "abc-123"})

    >>> # Poll for step events (Phase 3+)
    >>> from tasker_core import EventPoller
    >>> poller = EventPoller()
    >>> poller.on_step_event(handle_step)
    >>> poller.start()

    >>> # Create custom step handlers (Phase 4+)
    >>> from tasker_core import StepHandler, StepContext, StepHandlerResult
    >>> class MyHandler(StepHandler):
    ...     handler_name = "my_handler"
    ...     def call(self, context: StepContext) -> StepHandlerResult:
    ...         return StepHandlerResult.success_handler_result({"done": True})

    >>> # Stop the worker
    >>> tasker_core.stop_worker()
"""

from __future__ import annotations

# Import from the internal FFI module (Phase 1)
# Import event dispatch (Phase 3)
from tasker_core._tasker_core import (
    __version__,
    check_starvation_warnings,
    cleanup_timeouts,
    complete_step_event,
    get_ffi_dispatch_metrics,
    get_rust_version,
    get_version,
    health_check,
    poll_step_events,
)

# Import bootstrap functions (Phase 2)
from tasker_core.bootstrap import (
    bootstrap_worker,
    get_worker_status,
    is_worker_running,
    stop_worker,
    transition_to_graceful_shutdown,
)

# Import Phase 4: Handler system
from tasker_core.event_bridge import EventBridge, EventNames
from tasker_core.event_poller import EventPoller

# Import exceptions (Phase 2)
from tasker_core.exceptions import (
    ConversionError,
    FFIError,
    TaskerError,
    WorkerAlreadyRunningError,
    WorkerBootstrapError,
    WorkerNotInitializedError,
)
from tasker_core.handler import HandlerRegistry, StepHandler

# Import logging functions (Phase 2)
from tasker_core.logging import (
    log_debug,
    log_error,
    log_info,
    log_trace,
    log_warn,
)
from tasker_core.step_execution_subscriber import (
    StepExecutionError,
    StepExecutionSubscriber,
)

# Import types (Phase 2 + Phase 3 + Phase 4)
from tasker_core.types import (
    BootstrapConfig,
    BootstrapResult,
    FfiDispatchMetrics,
    FfiStepEvent,
    LogContext,
    ResultStatus,
    StarvationWarning,
    StepContext,
    StepError,
    StepExecutionResult,
    StepHandlerCallResult,
    StepHandlerResult,
    WorkerState,
    WorkerStatus,
)

__all__ = [
    # Version info (Phase 1)
    "__version__",
    "get_version",
    "get_rust_version",
    "health_check",
    "version",
    # Bootstrap functions (Phase 2)
    "bootstrap_worker",
    "stop_worker",
    "get_worker_status",
    "transition_to_graceful_shutdown",
    "is_worker_running",
    # Logging functions (Phase 2)
    "log_error",
    "log_warn",
    "log_info",
    "log_debug",
    "log_trace",
    # Types (Phase 2)
    "BootstrapConfig",
    "BootstrapResult",
    "WorkerStatus",
    "WorkerState",
    "StepHandlerCallResult",
    "LogContext",
    # Exceptions (Phase 2)
    "TaskerError",
    "WorkerNotInitializedError",
    "WorkerBootstrapError",
    "WorkerAlreadyRunningError",
    "FFIError",
    "ConversionError",
    # Event dispatch FFI functions (Phase 3)
    "poll_step_events",
    "complete_step_event",
    "get_ffi_dispatch_metrics",
    "check_starvation_warnings",
    "cleanup_timeouts",
    # Event dispatch types (Phase 3)
    "FfiStepEvent",
    "StepExecutionResult",
    "StepError",
    "ResultStatus",
    "FfiDispatchMetrics",
    "StarvationWarning",
    # Event poller (Phase 3)
    "EventPoller",
    # Handler system (Phase 4)
    "EventBridge",
    "EventNames",
    "HandlerRegistry",
    "StepHandler",
    "StepContext",
    "StepHandlerResult",
    "StepExecutionSubscriber",
    "StepExecutionError",
]


def version() -> str:
    """Return the package version.

    Returns:
        The version string (e.g., "0.1.0")

    Example:
        >>> import tasker_core
        >>> tasker_core.version()
        '0.1.0'
    """
    return get_version()
