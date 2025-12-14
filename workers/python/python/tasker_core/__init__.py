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

    >>> # Stop the worker
    >>> tasker_core.stop_worker()
"""

from __future__ import annotations

# Import from the internal FFI module (Phase 1)
from tasker_core._tasker_core import (
    __version__,
    get_rust_version,
    get_version,
    health_check,
)

# Import bootstrap functions (Phase 2)
from tasker_core.bootstrap import (
    bootstrap_worker,
    get_worker_status,
    is_worker_running,
    stop_worker,
    transition_to_graceful_shutdown,
)

# Import exceptions (Phase 2)
from tasker_core.exceptions import (
    ConversionError,
    FFIError,
    TaskerError,
    WorkerAlreadyRunningError,
    WorkerBootstrapError,
    WorkerNotInitializedError,
)

# Import logging functions (Phase 2)
from tasker_core.logging import (
    log_debug,
    log_error,
    log_info,
    log_trace,
    log_warn,
)

# Import types (Phase 2)
from tasker_core.types import (
    BootstrapConfig,
    BootstrapResult,
    LogContext,
    StepHandlerCallResult,
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
