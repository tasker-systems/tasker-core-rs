"""Structured logging for tasker-core Python worker.

This module provides structured logging functions that integrate with
the Rust tracing infrastructure, enabling unified logging across
Python and Rust components.

Example:
    >>> from tasker_core import log_info, log_error
    >>>
    >>> log_info("Processing started", {
    ...     "correlation_id": "abc-123",
    ...     "task_uuid": "task-456"
    ... })
    >>>
    >>> try:
    ...     process()
    ... except Exception as e:
    ...     log_error(f"Processing failed: {e}", {
    ...         "correlation_id": "abc-123",
    ...         "error_type": type(e).__name__
    ...     })
"""

from __future__ import annotations

from typing import Any

from tasker_core._tasker_core import (
    log_debug as _log_debug,
)
from tasker_core._tasker_core import (
    log_error as _log_error,
)
from tasker_core._tasker_core import (
    log_info as _log_info,
)
from tasker_core._tasker_core import (
    log_trace as _log_trace,
)
from tasker_core._tasker_core import (
    log_warn as _log_warn,
)

from .types import LogContext


def log_error(message: str, fields: dict[str, Any] | LogContext | None = None) -> None:
    """Log an ERROR level message with structured fields.

    Use this for unrecoverable failures that require intervention.

    Args:
        message: The log message.
        fields: Optional structured fields for context. Can be a dict
                or a LogContext instance.

    Example:
        >>> log_error("Database connection failed", {
        ...     "correlation_id": "abc-123",
        ...     "error_message": "Connection timeout"
        ... })
    """
    fields_dict = _normalize_fields(fields)
    _log_error(message, fields_dict)


def log_warn(message: str, fields: dict[str, Any] | LogContext | None = None) -> None:
    """Log a WARN level message with structured fields.

    Use this for degraded operation or retryable failures.

    Args:
        message: The log message.
        fields: Optional structured fields for context.

    Example:
        >>> log_warn("Retry attempt 3 of 5", {
        ...     "correlation_id": "abc-123",
        ...     "attempt": "3"
        ... })
    """
    fields_dict = _normalize_fields(fields)
    _log_warn(message, fields_dict)


def log_info(message: str, fields: dict[str, Any] | LogContext | None = None) -> None:
    """Log an INFO level message with structured fields.

    Use this for lifecycle events and state transitions.

    Args:
        message: The log message.
        fields: Optional structured fields for context.

    Example:
        >>> log_info("Task processing started", {
        ...     "correlation_id": "abc-123",
        ...     "task_uuid": "task-456",
        ...     "operation": "process_payment"
        ... })
    """
    fields_dict = _normalize_fields(fields)
    _log_info(message, fields_dict)


def log_debug(message: str, fields: dict[str, Any] | LogContext | None = None) -> None:
    """Log a DEBUG level message with structured fields.

    Use this for detailed diagnostic information during development.

    Args:
        message: The log message.
        fields: Optional structured fields for context.

    Example:
        >>> log_debug("Parsed request payload", {
        ...     "payload_size": "1024",
        ...     "content_type": "application/json"
        ... })
    """
    fields_dict = _normalize_fields(fields)
    _log_debug(message, fields_dict)


def log_trace(message: str, fields: dict[str, Any] | LogContext | None = None) -> None:
    """Log a TRACE level message with structured fields.

    Use this for very verbose logging, like function entry/exit.
    This level is typically disabled in production.

    Args:
        message: The log message.
        fields: Optional structured fields for context.

    Example:
        >>> log_trace("Entering process_step", {
        ...     "step_uuid": "step-789"
        ... })
    """
    fields_dict = _normalize_fields(fields)
    _log_trace(message, fields_dict)


def _normalize_fields(
    fields: dict[str, Any] | LogContext | None,
) -> dict[str, str] | None:
    """Normalize fields to a dict of strings for FFI.

    Args:
        fields: Input fields as dict, LogContext, or None.

    Returns:
        Dict with string values, or None if no fields.
    """
    if fields is None:
        return None

    if isinstance(fields, LogContext):
        # Convert LogContext to dict, excluding None values
        return {k: str(v) for k, v in fields.model_dump().items() if v is not None}

    # Convert all values to strings
    return {k: str(v) for k, v in fields.items()}


__all__ = [
    "log_error",
    "log_warn",
    "log_info",
    "log_debug",
    "log_trace",
]
