"""Pydantic models for tasker-core Python worker.

This module provides type-safe data models for the tasker-core FFI,
using Pydantic v2 for validation and serialization.

Phases:
- Phase 2: Bootstrap, lifecycle, logging types
- Phase 3: Event dispatch types (FfiStepEvent, StepExecutionResult, etc.)
"""

from __future__ import annotations

from enum import Enum
from typing import Any
from uuid import UUID

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


# =============================================================================
# Phase 3: Event Dispatch Types
# =============================================================================


class ResultStatus(str, Enum):
    """Step execution result status.

    These status values indicate the outcome of a step execution.
    """

    SUCCESS = "success"
    """Step completed successfully."""

    FAILURE = "failure"
    """Step failed with a business logic error."""

    RETRYABLE_ERROR = "retryable_error"
    """Step failed with a retryable error (temporary failure)."""

    PERMANENT_ERROR = "permanent_error"
    """Step failed with a permanent error (no retry possible)."""


class StepError(BaseModel):
    """Error details for failed step execution.

    This model captures detailed information about why a step failed,
    including whether the error is retryable.

    Example:
        >>> error = StepError(
        ...     error_type="ValidationError",
        ...     message="Invalid input data",
        ...     retryable=False
        ... )
    """

    error_type: str = Field(description="Error type/category for classification.")
    message: str = Field(description="Human-readable error message.")
    stack_trace: str | None = Field(
        default=None,
        description="Stack trace for debugging.",
    )
    retryable: bool = Field(
        default=True,
        description="Whether this error is retryable.",
    )
    metadata: dict[str, Any] = Field(
        default_factory=dict,
        description="Additional error context.",
    )


class StepExecutionResult(BaseModel):
    """Result submitted via complete_step_event().

    This model represents the result of executing a step handler,
    submitted back to the Rust orchestration layer.

    Example:
        >>> result = StepExecutionResult(
        ...     step_uuid=UUID("..."),
        ...     task_uuid=UUID("..."),
        ...     success=True,
        ...     status=ResultStatus.SUCCESS,
        ...     execution_time_ms=150,
        ...     output={"processed": 100}
        ... )
    """

    step_uuid: UUID = Field(description="The step UUID.")
    task_uuid: UUID = Field(description="The task UUID.")
    success: bool = Field(description="Whether execution succeeded.")
    status: str = Field(description="Result status string.")
    execution_time_ms: int = Field(description="Execution time in milliseconds.")
    result: dict[str, Any] | None = Field(
        default=None,
        description="Handler output data (success case). Named 'result' to match Rust FFI.",
    )
    error: StepError | None = Field(
        default=None,
        description="Error details (failure case).",
    )
    worker_id: str | None = Field(
        default=None,
        description="Worker that executed this step.",
    )
    correlation_id: UUID | None = Field(
        default=None,
        description="Correlation ID for tracing.",
    )
    metadata: dict[str, Any] = Field(
        default_factory=dict,
        description="Additional execution metadata.",
    )

    @classmethod
    def success_result(
        cls,
        step_uuid: UUID,
        task_uuid: UUID,
        result: dict[str, Any],
        execution_time_ms: int,
        worker_id: str,
        correlation_id: UUID | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> StepExecutionResult:
        """Create a successful result.

        Args:
            step_uuid: The step UUID.
            task_uuid: The task UUID.
            result: Handler output data.
            execution_time_ms: Execution time in milliseconds.
            worker_id: Worker that executed this step.
            correlation_id: Optional correlation ID.
            metadata: Optional additional metadata.

        Returns:
            A StepExecutionResult indicating success.
        """
        return cls(
            step_uuid=step_uuid,
            task_uuid=task_uuid,
            success=True,
            status=ResultStatus.SUCCESS.value,
            execution_time_ms=execution_time_ms,
            result=result,
            worker_id=worker_id,
            correlation_id=correlation_id,
            metadata=metadata or {},
        )

    @classmethod
    def failure_result(
        cls,
        step_uuid: UUID,
        task_uuid: UUID,
        error: StepError,
        execution_time_ms: int,
        worker_id: str,
        correlation_id: UUID | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> StepExecutionResult:
        """Create a failure result.

        Args:
            step_uuid: The step UUID.
            task_uuid: The task UUID.
            error: Error details.
            execution_time_ms: Execution time in milliseconds.
            worker_id: Worker that executed this step.
            correlation_id: Optional correlation ID.
            metadata: Optional additional metadata.

        Returns:
            A StepExecutionResult indicating failure.
        """
        status = (
            ResultStatus.RETRYABLE_ERROR.value
            if error.retryable
            else ResultStatus.PERMANENT_ERROR.value
        )
        return cls(
            step_uuid=step_uuid,
            task_uuid=task_uuid,
            success=False,
            status=status,
            execution_time_ms=execution_time_ms,
            error=error,
            worker_id=worker_id,
            correlation_id=correlation_id,
            metadata=metadata or {},
        )


class FfiStepEvent(BaseModel):
    """Event received from FFI poll_step_events().

    This model represents a step execution event received from the
    Rust orchestration layer, containing all information needed to
    execute the step handler.

    The event_id is critical for completion correlation - it must be
    passed back to complete_step_event() when submitting results.

    Example:
        >>> event = FfiStepEvent.model_validate(poll_step_events())
        >>> print(f"Processing step {event.step_uuid}")
        >>> # ... execute handler ...
        >>> complete_step_event(str(event.event_id), result_dict)
    """

    event_id: str = Field(description="Unique event ID for completion correlation.")
    task_uuid: str = Field(description="Task UUID.")
    step_uuid: str = Field(description="Step UUID.")
    correlation_id: str = Field(description="Correlation ID for tracing.")
    task_correlation_id: str | None = Field(
        default=None,
        description="Task-level correlation ID.",
    )
    parent_correlation_id: str | None = Field(
        default=None,
        description="Parent correlation ID (for nested tasks).",
    )
    task_sequence_step: dict[str, Any] = Field(
        description="Full TaskSequenceStep payload.",
    )
    trace_id: str | None = Field(
        default=None,
        description="OpenTelemetry trace ID.",
    )
    span_id: str | None = Field(
        default=None,
        description="OpenTelemetry span ID.",
    )

    model_config = {"frozen": True}


class FfiDispatchMetrics(BaseModel):
    """Metrics from the FFI dispatch channel.

    Provides observability into FFI dispatch channel health,
    including pending event counts and starvation detection.

    Example:
        >>> metrics = FfiDispatchMetrics.model_validate(get_ffi_dispatch_metrics())
        >>> if metrics.starvation_detected:
        ...     log_warn(f"{metrics.starving_event_count} events starving")
    """

    pending_count: int = Field(description="Number of pending events.")
    oldest_pending_age_ms: int | None = Field(
        default=None,
        description="Age of oldest pending event in milliseconds.",
    )
    newest_pending_age_ms: int | None = Field(
        default=None,
        description="Age of newest pending event in milliseconds.",
    )
    oldest_event_id: str | None = Field(
        default=None,
        description="UUID of oldest pending event.",
    )
    starvation_detected: bool = Field(
        default=False,
        description="Whether any events exceed starvation threshold.",
    )
    starving_event_count: int = Field(
        default=0,
        description="Number of events exceeding starvation threshold.",
    )


class StarvationWarning(BaseModel):
    """Warning for events that have been pending too long.

    Emitted when events exceed the starvation threshold,
    indicating slow polling or handler issues.
    """

    event_id: UUID = Field(description="Event ID that is starving.")
    step_uuid: UUID = Field(description="Step UUID.")
    pending_duration_ms: int = Field(description="How long the event has been pending.")
    threshold_ms: int = Field(description="The starvation threshold.")


# =============================================================================
# Phase 4: Handler System Types
# =============================================================================


class StepContext(BaseModel):
    """Context provided to step handlers during execution.

    Contains all information needed for a step handler to execute,
    including input data, dependency results, and configuration.

    Example:
        >>> context = StepContext(
        ...     event=ffi_event,
        ...     task_uuid=UUID("..."),
        ...     step_uuid=UUID("..."),
        ...     correlation_id=UUID("..."),
        ...     handler_name="my_handler",
        ...     input_data={"key": "value"},
        ... )
        >>> result = handler.call(context)
    """

    event: FfiStepEvent = Field(description="The original FFI step event.")
    task_uuid: UUID = Field(description="Task UUID.")
    step_uuid: UUID = Field(description="Step UUID.")
    correlation_id: UUID = Field(description="Correlation ID for tracing.")
    handler_name: str = Field(description="Name of the handler being executed.")
    input_data: dict[str, Any] = Field(
        default_factory=dict,
        description="Input data for the handler.",
    )
    dependency_results: dict[str, Any] = Field(
        default_factory=dict,
        description="Results from dependent steps.",
    )
    step_config: dict[str, Any] = Field(
        default_factory=dict,
        description="Handler-specific configuration.",
    )
    retry_count: int = Field(
        default=0,
        description="Current retry attempt number.",
    )
    max_retries: int = Field(
        default=3,
        description="Maximum retry attempts allowed.",
    )

    model_config = {"arbitrary_types_allowed": True}

    @classmethod
    def from_ffi_event(
        cls,
        event: FfiStepEvent,
        handler_name: str,
    ) -> StepContext:
        """Create a StepContext from an FFI event.

        Extracts input data, dependency results, and configuration from
        the task_sequence_step payload.

        The FFI data structure mirrors the Ruby TaskSequenceStepWrapper:
        - task.task.context -> input_data (task context with user inputs)
        - dependency_results -> results from parent steps
        - step_definition.handler.initialization -> step_config
        - workflow_step.attempts -> retry_count
        - workflow_step.max_attempts -> max_retries

        Args:
            event: The FFI step event.
            handler_name: Name of the handler to execute.

        Returns:
            A StepContext populated from the event.
        """
        tss = event.task_sequence_step

        # Extract task context (input_data) from nested task structure
        # Ruby: step_data.task.context
        task_data = tss.get("task", {})
        inner_task = task_data.get("task", task_data)  # Handle nested structure
        input_data = inner_task.get("context", {})

        # Extract dependency results
        # Ruby: step_data.dependency_results
        dependency_results = tss.get("dependency_results", {})

        # Extract step config from handler initialization
        # Ruby: step_data.step_definition.handler.initialization
        step_definition = tss.get("step_definition", {})
        handler_config = step_definition.get("handler", {})
        step_config = handler_config.get("initialization", {})

        # Extract retry information from workflow_step
        # Ruby: step_data.workflow_step.attempts, step_data.workflow_step.max_attempts
        # Note: Use `or` to handle None values (get returns None if key exists but value is None)
        workflow_step = tss.get("workflow_step", {})
        retry_count = workflow_step.get("attempts") or 0
        max_retries = workflow_step.get("max_attempts") or 3

        return cls(
            event=event,
            task_uuid=UUID(event.task_uuid),
            step_uuid=UUID(event.step_uuid),
            correlation_id=UUID(event.correlation_id),
            handler_name=handler_name,
            input_data=input_data,
            dependency_results=dependency_results,
            step_config=step_config,
            retry_count=retry_count,
            max_retries=max_retries,
        )

    def get_dependency_result(self, step_name: str) -> Any:
        """Get the computed result value from a dependency step.

        This method extracts the actual computed value from a dependency result,
        unwrapping any nested structure. It mirrors Ruby's get_results() behavior.

        The dependency result structure can be:
        - {"result": actual_value} - unwraps to actual_value
        - primitive value - returns as-is

        Args:
            step_name: Name of the dependency step.

        Returns:
            The computed result value, or None if not found.

        Example:
            >>> # Instead of:
            >>> step1_result = context.dependency_results.get("step_1", {})
            >>> value = step1_result.get("result")  # Might be nested!
            >>>
            >>> # Use:
            >>> value = context.get_dependency_result("step_1")  # Unwrapped
        """
        result_hash = self.dependency_results.get(step_name)
        if result_hash is None:
            return None

        # If it's a dict with a 'result' key, extract that value
        # This handles the nested structure: {"result": {...actual handler output...}}
        if isinstance(result_hash, dict) and "result" in result_hash:
            return result_hash["result"]

        # Otherwise return the whole thing (might be a primitive value)
        return result_hash


class StepHandlerResult(BaseModel):
    """Result from a step handler execution.

    Step handlers return this to indicate success or failure,
    along with any output data or error details.

    Example:
        >>> # Success case
        >>> result = StepHandlerResult.success({"processed": 100})
        >>>
        >>> # Failure case
        >>> result = StepHandlerResult.failure(
        ...     message="Validation failed",
        ...     error_type="ValidationError",
        ...     retryable=False,
        ... )
    """

    success: bool = Field(description="Whether the handler executed successfully.")
    result: dict[str, Any] | None = Field(
        default=None,
        description="Handler output data (success case).",
    )
    error_message: str | None = Field(
        default=None,
        description="Error message (failure case).",
    )
    error_type: str | None = Field(
        default=None,
        description="Error type/category for classification.",
    )
    retryable: bool = Field(
        default=True,
        description="Whether the error is retryable.",
    )
    metadata: dict[str, Any] = Field(
        default_factory=dict,
        description="Additional execution metadata.",
    )

    @classmethod
    def success_handler_result(
        cls,
        result: dict[str, Any],
        metadata: dict[str, Any] | None = None,
    ) -> StepHandlerResult:
        """Create a successful handler result.

        Args:
            result: The handler output data.
            metadata: Optional additional metadata.

        Returns:
            A StepHandlerResult indicating success.

        Example:
            >>> result = StepHandlerResult.success_handler_result(
            ...     {"processed": 100, "skipped": 5}
            ... )
        """
        return cls(
            success=True,
            result=result,
            metadata=metadata or {},
        )

    @classmethod
    def failure_handler_result(
        cls,
        message: str,
        error_type: str = "handler_error",
        retryable: bool = True,
        metadata: dict[str, Any] | None = None,
    ) -> StepHandlerResult:
        """Create a failure handler result.

        Args:
            message: Human-readable error message.
            error_type: Error type/category for classification.
            retryable: Whether the error is retryable.
            metadata: Optional additional metadata.

        Returns:
            A StepHandlerResult indicating failure.

        Example:
            >>> result = StepHandlerResult.failure_handler_result(
            ...     message="Invalid input format",
            ...     error_type="ValidationError",
            ...     retryable=False,
            ... )
        """
        return cls(
            success=False,
            error_message=message,
            error_type=error_type,
            retryable=retryable,
            metadata=metadata or {},
        )


# =============================================================================
# Phase 5: Domain Events & Observability Types
# =============================================================================


class DomainEventMetadata(BaseModel):
    """Metadata for domain events.

    Contains correlation and tracing information for domain events.

    Example:
        >>> metadata = DomainEventMetadata(
        ...     task_uuid=UUID("..."),
        ...     step_uuid=UUID("..."),
        ...     namespace="payments",
        ...     correlation_id=UUID("..."),
        ...     fired_at="2025-01-01T00:00:00Z",
        ...     fired_by="python-worker",
        ... )
    """

    task_uuid: UUID = Field(description="Task UUID.")
    step_uuid: UUID | None = Field(
        default=None,
        description="Step UUID (if step-level event).",
    )
    step_name: str | None = Field(
        default=None,
        description="Step name.",
    )
    namespace: str | None = Field(
        default=None,
        description="Task namespace.",
    )
    correlation_id: UUID = Field(description="Correlation ID for tracing.")
    fired_at: str = Field(description="ISO-8601 timestamp when event was fired.")
    fired_by: str = Field(description="Source that fired the event.")


class InProcessDomainEvent(BaseModel):
    """Domain event received from in-process polling.

    In-process events use the fast path (tokio broadcast channel)
    for real-time notifications that don't require guaranteed delivery.

    Example:
        >>> event = InProcessDomainEvent.model_validate(poll_in_process_events())
        >>> if event:
        ...     print(f"Received {event.event_name}: {event.payload}")
    """

    event_id: UUID = Field(description="Unique event ID.")
    event_name: str = Field(description="Event name (e.g., 'step.completed').")
    event_version: str = Field(description="Event schema version.")
    metadata: DomainEventMetadata = Field(description="Event metadata.")
    payload: dict[str, Any] = Field(
        default_factory=dict,
        description="Event payload data.",
    )


class ComponentHealth(BaseModel):
    """Health status for a single component.

    Example:
        >>> component = ComponentHealth(
        ...     name="database",
        ...     status="healthy",
        ...     pool_size=10,
        ...     pool_idle=5,
        ... )
    """

    name: str = Field(description="Component name.")
    status: str = Field(description="Health status (healthy/unhealthy).")
    pool_size: int | None = Field(
        default=None,
        description="Connection pool size (for database).",
    )
    pool_idle: int | None = Field(
        default=None,
        description="Idle connections in pool.",
    )


class HealthCheck(BaseModel):
    """Health check response from the worker.

    Provides comprehensive health status of all worker components.

    Example:
        >>> health = HealthCheck.model_validate(get_health_check())
        >>> if health.status == "healthy":
        ...     print("Worker is healthy")
    """

    status: str = Field(description="Overall health status (healthy/unhealthy).")
    is_running: bool = Field(description="Whether the worker is running.")
    components: dict[str, ComponentHealth] = Field(
        default_factory=dict,
        description="Health status of individual components.",
    )
    rust: dict[str, Any] = Field(
        default_factory=dict,
        description="Rust layer information.",
    )
    python: dict[str, Any] = Field(
        default_factory=dict,
        description="Python layer information.",
    )


class WorkerMetrics(BaseModel):
    """Performance metrics from the worker.

    Provides metrics about step execution, timing, channels, and errors.

    Example:
        >>> metrics = WorkerMetrics.model_validate(get_metrics())
        >>> print(f"Pending events: {metrics.dispatch_channel_pending}")
    """

    # FFI dispatch channel metrics
    dispatch_channel_pending: int = Field(
        default=0,
        description="Number of pending events in dispatch channel.",
    )
    oldest_pending_age_ms: int | None = Field(
        default=None,
        description="Age of oldest pending event in milliseconds.",
    )
    newest_pending_age_ms: int | None = Field(
        default=None,
        description="Age of newest pending event in milliseconds.",
    )
    starvation_detected: bool = Field(
        default=False,
        description="Whether event starvation is detected.",
    )
    starving_event_count: int = Field(
        default=0,
        description="Number of starving events.",
    )

    # Database metrics
    database_pool_size: int = Field(
        default=0,
        description="Total database connection pool size.",
    )
    database_pool_idle: int = Field(
        default=0,
        description="Idle database connections.",
    )

    # Environment
    environment: str | None = Field(
        default=None,
        description="Current environment.",
    )

    # Step execution metrics (placeholders, populated as worker tracks stats)
    steps_processed: int = Field(
        default=0,
        description="Total steps processed.",
    )
    steps_succeeded: int = Field(
        default=0,
        description="Steps completed successfully.",
    )
    steps_failed: int = Field(
        default=0,
        description="Steps that failed.",
    )
    steps_in_progress: int = Field(
        default=0,
        description="Steps currently in progress.",
    )


class WorkerConfig(BaseModel):
    """Current worker configuration.

    Provides runtime configuration settings for the worker.

    Example:
        >>> config = WorkerConfig.model_validate(get_worker_config())
        >>> print(f"Environment: {config.environment}")
        >>> print(f"Namespaces: {config.supported_namespaces}")
    """

    environment: str | None = Field(
        default=None,
        description="Current environment (test, development, production).",
    )
    supported_namespaces: list[str] = Field(
        default_factory=list,
        description="List of task namespaces this worker handles.",
    )
    web_api_enabled: bool = Field(
        default=False,
        description="Whether the web API is enabled.",
    )
    database_pool_size: int = Field(
        default=0,
        description="Database connection pool size.",
    )

    # Configuration defaults
    polling_interval_ms: int = Field(
        default=10,
        description="Event polling interval in milliseconds.",
    )
    starvation_threshold_ms: int = Field(
        default=1000,
        description="Event starvation detection threshold.",
    )
    max_concurrent_handlers: int = Field(
        default=10,
        description="Maximum concurrent handler executions.",
    )
    handler_timeout_ms: int = Field(
        default=30000,
        description="Handler execution timeout in milliseconds.",
    )


__all__ = [
    # Phase 2: Bootstrap and lifecycle
    "WorkerState",
    "BootstrapConfig",
    "BootstrapResult",
    "WorkerStatus",
    "StepHandlerCallResult",
    "LogContext",
    # Phase 3: Event dispatch
    "ResultStatus",
    "StepError",
    "StepExecutionResult",
    "FfiStepEvent",
    "FfiDispatchMetrics",
    "StarvationWarning",
    # Phase 4: Handler system
    "StepContext",
    "StepHandlerResult",
    # Phase 5: Domain events & observability
    "DomainEventMetadata",
    "InProcessDomainEvent",
    "ComponentHealth",
    "HealthCheck",
    "WorkerMetrics",
    "WorkerConfig",
]
