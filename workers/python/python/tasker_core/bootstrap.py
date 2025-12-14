"""Worker bootstrap and lifecycle management.

This module provides the high-level Python API for bootstrapping and
managing the tasker-core worker system.

Example:
    >>> from tasker_core import bootstrap_worker, stop_worker, get_worker_status
    >>>
    >>> # Start the worker
    >>> result = bootstrap_worker()
    >>> print(f"Worker started: {result.worker_id}")
    >>>
    >>> # Check status
    >>> status = get_worker_status()
    >>> print(f"Running: {status.running}")
    >>>
    >>> # Stop the worker
    >>> stop_worker()
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from tasker_core._tasker_core import (
    bootstrap_worker as _bootstrap_worker,
)
from tasker_core._tasker_core import (
    get_worker_status as _get_worker_status,
)
from tasker_core._tasker_core import (
    is_worker_running as _is_worker_running,
)
from tasker_core._tasker_core import (
    stop_worker as _stop_worker,
)
from tasker_core._tasker_core import (
    transition_to_graceful_shutdown as _transition_to_graceful_shutdown,
)

from .types import BootstrapConfig, BootstrapResult, WorkerStatus

if TYPE_CHECKING:
    pass


def bootstrap_worker(config: BootstrapConfig | None = None) -> BootstrapResult:
    """Initialize the worker system.

    This function bootstraps the full tasker-worker system, including:
    - Creating a Tokio runtime for async operations
    - Connecting to the database
    - Setting up the FFI dispatch channel for step events
    - Subscribing to domain events

    Args:
        config: Optional bootstrap configuration. If not provided,
                defaults will be used.

    Returns:
        BootstrapResult with worker details and status.

    Raises:
        WorkerBootstrapError: If bootstrap fails.
        WorkerAlreadyRunningError: If worker is already running.

    Example:
        >>> # Bootstrap with defaults
        >>> result = bootstrap_worker()
        >>> print(f"Worker {result.worker_id} started")
        >>>
        >>> # Bootstrap with custom config
        >>> config = BootstrapConfig(namespace="payments", log_level="debug")
        >>> result = bootstrap_worker(config)
    """
    config_dict = config.model_dump() if config else None
    result_dict = _bootstrap_worker(config_dict)

    # Map the result to our Pydantic model
    return BootstrapResult(
        success=result_dict.get("status") == "started",
        handle_id=result_dict.get("handle_id", ""),
        worker_id=result_dict.get("worker_id", ""),
        status=result_dict.get("status", ""),
        message=result_dict.get("message"),
    )


def stop_worker() -> str:
    """Stop the worker system gracefully.

    This function stops the worker system and releases all resources.
    It is safe to call this function even if the worker is not running.

    Returns:
        Status message indicating the result.

    Raises:
        WorkerNotInitializedError: If worker not running.

    Example:
        >>> stop_worker()
        'Worker system stopped'
    """
    return _stop_worker()


def get_worker_status() -> WorkerStatus:
    """Get the current worker system status.

    Returns detailed information about the worker's current state,
    including resource usage and operational status.

    Returns:
        WorkerStatus with current state and metrics.

    Example:
        >>> status = get_worker_status()
        >>> if status.running:
        ...     print(f"Pool size: {status.database_pool_size}")
        ... else:
        ...     print(f"Error: {status.error}")
    """
    result_dict = _get_worker_status()
    return WorkerStatus.model_validate(result_dict)


def transition_to_graceful_shutdown() -> str:
    """Initiate graceful shutdown of the worker system.

    This function begins the graceful shutdown process, allowing
    in-flight operations to complete before fully stopping.

    Returns:
        Status message indicating the transition.

    Raises:
        WorkerNotInitializedError: If worker not running.

    Example:
        >>> transition_to_graceful_shutdown()
        'Worker system transitioned to graceful shutdown'
    """
    return _transition_to_graceful_shutdown()


def is_worker_running() -> bool:
    """Check if the worker system is currently running.

    This is a lightweight check that doesn't query the full status.

    Returns:
        True if the worker is running, False otherwise.

    Example:
        >>> if not is_worker_running():
        ...     bootstrap_worker()
    """
    return _is_worker_running()


__all__ = [
    "bootstrap_worker",
    "stop_worker",
    "get_worker_status",
    "transition_to_graceful_shutdown",
    "is_worker_running",
]
