"""Custom exceptions for tasker-core Python worker.

This module provides a hierarchy of exceptions for error handling
in the tasker-core Python bindings.
"""

from __future__ import annotations


class TaskerError(Exception):
    """Base exception for all tasker-core errors.

    All exceptions raised by tasker-core inherit from this class,
    making it easy to catch all tasker-related errors.

    Example:
        >>> try:
        ...     result = bootstrap_worker()
        ... except TaskerError as e:
        ...     print(f"Tasker error: {e}")
    """

    pass


class WorkerNotInitializedError(TaskerError):
    """Raised when an FFI function is called before bootstrap_worker().

    This error indicates that the worker system has not been initialized
    and a function requiring the worker was called.

    Example:
        >>> try:
        ...     status = get_worker_status()
        ... except WorkerNotInitializedError:
        ...     print("Worker not started, call bootstrap_worker() first")
    """

    pass


class WorkerBootstrapError(TaskerError):
    """Raised when worker bootstrap fails.

    This error indicates that the worker system failed to initialize.
    Check the error message for details about the failure.

    Common causes:
    - Database connection failed
    - Invalid configuration
    - Resource exhaustion (memory, file descriptors)

    Example:
        >>> try:
        ...     result = bootstrap_worker()
        ... except WorkerBootstrapError as e:
        ...     print(f"Bootstrap failed: {e}")
    """

    pass


class WorkerAlreadyRunningError(TaskerError):
    """Raised when attempting to bootstrap an already-running worker.

    Only one worker instance can run at a time. If you need to restart
    the worker, call stop_worker() first.

    Example:
        >>> try:
        ...     result = bootstrap_worker()  # Already running
        ... except WorkerAlreadyRunningError:
        ...     print("Worker already running, stop it first")
    """

    pass


class FFIError(TaskerError):
    """Raised for low-level FFI errors.

    This error indicates a problem at the FFI boundary between
    Python and Rust. These are typically internal errors.

    Example:
        >>> try:
        ...     # Some FFI operation
        ...     pass
        ... except FFIError as e:
        ...     print(f"FFI error: {e}")
    """

    pass


class ConversionError(TaskerError):
    """Raised when type conversion fails.

    This error indicates that data could not be converted between
    Python and Rust types, typically due to invalid data format.

    Example:
        >>> try:
        ...     complete_step_event(event_id, invalid_data)
        ... except ConversionError as e:
        ...     print(f"Invalid data format: {e}")
    """

    pass


__all__ = [
    "TaskerError",
    "WorkerNotInitializedError",
    "WorkerBootstrapError",
    "WorkerAlreadyRunningError",
    "FFIError",
    "ConversionError",
]
