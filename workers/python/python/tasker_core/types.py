"""Pydantic models for tasker-core Python worker.

This module provides type-safe data models for the tasker-core FFI,
using Pydantic v2 for validation and serialization.
"""

from __future__ import annotations

from enum import Enum
from typing import Any

from pydantic import BaseModel, Field


class WorkerState(str, Enum):
    """Worker lifecycle states.

    These states represent the different phases of the worker lifecycle.
    """

    STARTING = "starting"
    """Worker is initializing and connecting to resources."""

    RUNNING = "running"
    """Worker is fully operational and processing events."""

    SHUTTING_DOWN = "shutting_down"
    """Worker is gracefully shutting down, completing in-flight work."""

    STOPPED = "stopped"
    """Worker has fully stopped and released all resources."""

    ERROR = "error"
    """Worker encountered an unrecoverable error."""


class BootstrapConfig(BaseModel):
    """Configuration for worker bootstrap.

    This model validates and holds configuration options for
    initializing the worker system.

    Example:
        >>> config = BootstrapConfig(namespace="payments", log_level="debug")
        >>> result = bootstrap_worker(config)
    """

    worker_id: str | None = Field(
        default=None,
        description="Optional worker ID. Auto-generated if not provided.",
    )
    namespace: str = Field(
        default="default",
        description="Task namespace this worker handles.",
    )
    config_path: str | None = Field(
        default=None,
        description="Path to custom configuration file.",
    )
    log_level: str = Field(
        default="info",
        pattern="^(trace|debug|info|warn|error)$",
        description="Log level for the worker (trace, debug, info, warn, error).",
    )

    model_config = {"extra": "forbid"}


class BootstrapResult(BaseModel):
    """Result from worker bootstrap.

    Contains information about the bootstrapped worker instance.

    Example:
        >>> result = bootstrap_worker()
        >>> if result.success:
        ...     print(f"Worker {result.worker_id} started")
    """

    success: bool = Field(description="Whether bootstrap was successful.")
    handle_id: str = Field(description="Unique identifier for this worker instance.")
    worker_id: str = Field(description="Full worker identifier string.")
    status: str = Field(description="Current status (started, already_running).")
    message: str | None = Field(
        default=None,
        description="Human-readable status message.",
    )
    error_message: str | None = Field(
        default=None,
        description="Error message if bootstrap failed.",
    )


class WorkerStatus(BaseModel):
    """Current worker status.

    Contains detailed information about the worker's current state
    and resource usage.

    Example:
        >>> status = get_worker_status()
        >>> if status.running:
        ...     print(f"Processed {status.steps_processed} steps")
    """

    running: bool = Field(description="Whether the worker is currently running.")
    environment: str | None = Field(
        default=None,
        description="Current environment (test, development, production).",
    )
    worker_core_status: str | None = Field(
        default=None,
        description="Internal worker core status.",
    )
    web_api_enabled: bool | None = Field(
        default=None,
        description="Whether the web API is enabled.",
    )
    supported_namespaces: list[str] | None = Field(
        default=None,
        description="List of task namespaces this worker handles.",
    )
    database_pool_size: int | None = Field(
        default=None,
        description="Total database connection pool size.",
    )
    database_pool_idle: int | None = Field(
        default=None,
        description="Number of idle database connections.",
    )
    error: str | None = Field(
        default=None,
        description="Error message if worker is not running.",
    )


class StepHandlerCallResult(BaseModel):
    """Result from a step handler execution.

    This model represents the result of executing a step handler,
    including success/failure status and any output data.

    Example:
        >>> result = StepHandlerCallResult(
        ...     success=True,
        ...     result={"processed": 100},
        ...     metadata={"duration_ms": 150}
        ... )
    """

    success: bool = Field(description="Whether the handler executed successfully.")
    result: dict[str, Any] = Field(
        default_factory=dict,
        description="Handler output data.",
    )
    error_message: str | None = Field(
        default=None,
        description="Error message if handler failed.",
    )
    metadata: dict[str, Any] = Field(
        default_factory=dict,
        description="Additional metadata about the execution.",
    )


class LogContext(BaseModel):
    """Context fields for structured logging.

    This model provides structured context for log messages,
    enabling correlation and filtering.

    Example:
        >>> context = LogContext(
        ...     correlation_id="abc-123",
        ...     task_uuid="task-456",
        ...     operation="process_payment"
        ... )
        >>> log_info("Payment processed", context.model_dump())
    """

    correlation_id: str | None = Field(
        default=None,
        description="Correlation ID for request tracing.",
    )
    task_uuid: str | None = Field(
        default=None,
        description="Task UUID for task-level tracing.",
    )
    step_uuid: str | None = Field(
        default=None,
        description="Step UUID for step-level tracing.",
    )
    namespace: str | None = Field(
        default=None,
        description="Task namespace.",
    )
    operation: str | None = Field(
        default=None,
        description="Current operation name.",
    )


__all__ = [
    "WorkerState",
    "BootstrapConfig",
    "BootstrapResult",
    "WorkerStatus",
    "StepHandlerCallResult",
    "LogContext",
]
