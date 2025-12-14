"""Type stubs for the Rust FFI module.

This file provides type hints for the _tasker_core extension module
which is compiled from Rust using PyO3.
"""

from typing import Any

# Version of the package from Cargo.toml
__version__: str

# Phase 1: Version and health check functions

def get_version() -> str:
    """Return the package version string.

    Returns:
        The version string (e.g., "0.1.0")
    """
    ...

def get_rust_version() -> str:
    """Return the Rust library version for debugging.

    Returns:
        Version string including rustc version (e.g., "tasker-worker-py 0.1.0 (rustc 1.70.0)")
    """
    ...

def health_check() -> bool:
    """Check if the FFI module is working correctly.

    Returns:
        True if the FFI layer is functional
    """
    ...

# Phase 2: Bootstrap and lifecycle functions

def bootstrap_worker(config: dict[str, Any] | None = None) -> dict[str, Any]:
    """Initialize the worker system.

    Args:
        config: Optional configuration dict with keys like 'namespace', 'log_level'.

    Returns:
        Dict with keys: handle_id, status, message, worker_id

    Raises:
        RuntimeError: If bootstrap fails or lock cannot be acquired.
    """
    ...

def stop_worker() -> str:
    """Stop the worker system gracefully.

    Returns:
        Status message ("Worker system stopped" or "Worker system not running")

    Raises:
        RuntimeError: If lock cannot be acquired or stop fails.
    """
    ...

def get_worker_status() -> dict[str, Any]:
    """Get the current worker system status.

    Returns:
        Dict with keys: running, environment, worker_core_status, web_api_enabled,
        supported_namespaces, database_pool_size, database_pool_idle, error

    Raises:
        RuntimeError: If lock cannot be acquired or status fetch fails.
    """
    ...

def transition_to_graceful_shutdown() -> str:
    """Initiate graceful shutdown of the worker system.

    Returns:
        Status message indicating the transition.

    Raises:
        RuntimeError: If worker not running or shutdown fails.
    """
    ...

def is_worker_running() -> bool:
    """Check if the worker system is currently running.

    Returns:
        True if the worker is running, False otherwise.

    Raises:
        RuntimeError: If lock cannot be acquired.
    """
    ...

# Phase 2: Logging functions

def log_error(message: str, fields: dict[str, str] | None = None) -> None:
    """Log an ERROR level message with structured fields.

    Args:
        message: The log message.
        fields: Optional dict of string key-value pairs for context.
    """
    ...

def log_warn(message: str, fields: dict[str, str] | None = None) -> None:
    """Log a WARN level message with structured fields.

    Args:
        message: The log message.
        fields: Optional dict of string key-value pairs for context.
    """
    ...

def log_info(message: str, fields: dict[str, str] | None = None) -> None:
    """Log an INFO level message with structured fields.

    Args:
        message: The log message.
        fields: Optional dict of string key-value pairs for context.
    """
    ...

def log_debug(message: str, fields: dict[str, str] | None = None) -> None:
    """Log a DEBUG level message with structured fields.

    Args:
        message: The log message.
        fields: Optional dict of string key-value pairs for context.
    """
    ...

def log_trace(message: str, fields: dict[str, str] | None = None) -> None:
    """Log a TRACE level message with structured fields.

    Args:
        message: The log message.
        fields: Optional dict of string key-value pairs for context.
    """
    ...

# Phase 3: Event dispatch functions

def poll_step_events() -> dict[str, Any] | None:
    """Poll for pending step execution events.

    This is a non-blocking call that returns the next available
    step event or None if no events are pending.

    Returns:
        Dict representing FfiStepEvent, or None if no events available.
        Keys include: event_id, task_uuid, step_uuid, correlation_id,
        task_sequence_step, trace_id, span_id

    Raises:
        RuntimeError: If worker system is not running.
    """
    ...

def complete_step_event(event_id: str, completion_data: dict[str, Any]) -> bool:
    """Submit completion result for a step event.

    Args:
        event_id: The event ID string from the FfiStepEvent.
        completion_data: Dict with completion result data matching
            StepExecutionResult structure.

    Returns:
        True if completion was successfully submitted, False otherwise.

    Raises:
        ValueError: If event_id format is invalid.
        RuntimeError: If worker system is not running.
    """
    ...

def get_ffi_dispatch_metrics() -> dict[str, Any]:
    """Get metrics from the FFI dispatch channel.

    Returns:
        Dict with keys: pending_count, oldest_pending_age_ms,
        newest_pending_age_ms, oldest_event_id, starvation_detected,
        starving_event_count

    Raises:
        RuntimeError: If worker system is not running.
    """
    ...

def check_starvation_warnings() -> bool:
    """Check for starvation warnings and emit logs.

    Emits warning logs for any pending events that exceed the
    starvation threshold.

    Returns:
        True after checking.

    Raises:
        RuntimeError: If worker system is not running.
    """
    ...

def cleanup_timeouts() -> int:
    """Clean up timed-out pending events.

    Returns:
        Number of events that were cleaned up.

    Raises:
        RuntimeError: If worker system is not running.
    """
    ...
