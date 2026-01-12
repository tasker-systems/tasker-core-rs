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
    They must match the values expected by the Rust orchestration layer.
    """

    SUCCESS = "completed"
    """Step completed successfully. Value must be 'completed' to match Rust expectations."""

    FAILURE = "error"
    """Step failed with a business logic error. Value must be 'error' to match Rust expectations."""

    RETRYABLE_ERROR = "retryable_error"
    """Step failed with a retryable error (temporary failure)."""

    PERMANENT_ERROR = "permanent_error"
    """Step failed with a permanent error (no retry possible)."""


class ErrorType(str, Enum):
    """Standard error types for cross-language consistency.

    Using str, Enum allows the value to serialize as a string while
    providing type safety and IDE support. These values align with
    the Rust and Ruby worker implementations.

    Example:
        >>> from tasker_core import ErrorType, StepHandlerResult
        >>> result = StepHandlerResult.failure(
        ...     message="Payment gateway timeout",
        ...     error_type=ErrorType.TIMEOUT,
        ...     retryable=True,
        ... )
    """

    PERMANENT_ERROR = "permanent_error"
    """Error indicating a permanent, non-recoverable failure.
    Examples: invalid input, resource not found, authentication failure."""

    RETRYABLE_ERROR = "retryable_error"
    """Error indicating a transient failure that may succeed on retry.
    Examples: network timeout, service unavailable, rate limiting."""

    VALIDATION_ERROR = "validation_error"
    """Error indicating input validation failure.
    Examples: missing required field, invalid format, constraint violation."""

    TIMEOUT = "timeout"
    """Error indicating an operation timed out.
    Examples: HTTP request timeout, database query timeout."""

    HANDLER_ERROR = "handler_error"
    """Error indicating a failure within the step handler itself.
    Examples: unhandled exception, handler misconfiguration."""

    @classmethod
    def is_standard(cls, error_type: str) -> bool:
        """Check if an error type is one of the standard values.

        Args:
            error_type: The error type string to check.

        Returns:
            True if the error type matches one of the standard values.

        Example:
            >>> ErrorType.is_standard("permanent_error")
            True
            >>> ErrorType.is_standard("custom_error")
            False
        """
        return error_type in [e.value for e in cls]

    @classmethod
    def is_typically_retryable(cls, error_type: str) -> bool:
        """Get the recommended retryable flag for a given error type.

        Args:
            error_type: The error type string.

        Returns:
            True if the error type is typically retryable.

        Example:
            >>> ErrorType.is_typically_retryable("timeout")
            True
            >>> ErrorType.is_typically_retryable("permanent_error")
            False
        """
        return error_type in [cls.RETRYABLE_ERROR.value, cls.TIMEOUT.value]


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
    step_inputs: dict[str, Any] = Field(
        default_factory=dict,
        description="Step-specific inputs (from workflow_step.inputs). Used for batch cursor configuration.",
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

        # Extract retry information and step inputs from workflow_step
        # Ruby: step_data.workflow_step.attempts, step_data.workflow_step.max_attempts
        # Ruby: step_data.workflow_step.inputs (for batch cursor configuration)
        # Note: Use `or` to handle None values (get returns None if key exists but value is None)
        workflow_step = tss.get("workflow_step", {})
        retry_count = workflow_step.get("attempts") or 0
        max_retries = workflow_step.get("max_attempts") or 3
        step_inputs = workflow_step.get("inputs") or {}

        return cls(
            event=event,
            task_uuid=UUID(event.task_uuid),
            step_uuid=UUID(event.step_uuid),
            correlation_id=UUID(event.correlation_id),
            handler_name=handler_name,
            input_data=input_data,
            dependency_results=dependency_results,
            step_config=step_config,
            step_inputs=step_inputs,
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

    def get_input(self, key: str) -> Any:
        """Get a value from the input data.

        Args:
            key: The key to look up in input_data.

        Returns:
            The value or None if not found.

        Example:
            >>> order_id = context.get_input("order_id")
        """
        return self.input_data.get(key)

    def get_config(self, key: str) -> Any:
        """Get a value from the step configuration.

        Args:
            key: The key to look up in step_config.

        Returns:
            The value or None if not found.

        Example:
            >>> batch_size = context.get_config("batch_size")
        """
        return self.step_config.get(key)

    def is_retry(self) -> bool:
        """Check if this is a retry attempt.

        Returns:
            True if retry_count > 0.
        """
        return self.retry_count > 0

    def is_last_retry(self) -> bool:
        """Check if this is the last allowed retry attempt.

        Returns:
            True if retry_count >= max_retries - 1.
        """
        return self.retry_count >= self.max_retries - 1

    def get_input_or(self, key: str, default: Any = None) -> Any:
        """Get a value from the input data with a default.

        Args:
            key: The key to look up in input_data.
            default: Value to return if key not found or value is None.

        Returns:
            The value or default if not found/None.

        Example:
            >>> batch_size = context.get_input_or("batch_size", 100)
        """
        value = self.input_data.get(key)
        return default if value is None else value

    def get_dependency_field(self, step_name: str, *path: str) -> Any:
        """Extract a nested field from a dependency result.

        Useful when dependency results are complex objects and you need
        to extract a specific nested value without manual dict traversal.

        Args:
            step_name: Name of the dependency step.
            path: Path elements to traverse into the result.

        Returns:
            The nested value, or None if not found.

        Example:
            >>> # Extract nested field from dependency result
            >>> csv_path = context.get_dependency_field("analyze_csv", "csv_file_path")
            >>> # Multiple levels deep
            >>> value = context.get_dependency_field("step_1", "data", "items")
        """
        result = self.get_dependency_result(step_name)
        if result is None:
            return None
        for key in path:
            if not isinstance(result, dict):
                return None
            result = result.get(key)
        return result

    # =========================================================================
    # CHECKPOINT ACCESSORS (TAS-125 Batch Processing Support)
    # =========================================================================

    @property
    def checkpoint(self) -> dict[str, Any] | None:
        """Get the raw checkpoint data from the workflow step.

        Returns:
            The checkpoint data dict or None if not set.
        """
        workflow_step = self.event.task_sequence_step.get("workflow_step", {})
        checkpoint_data = (
            workflow_step.get("checkpoint") if isinstance(workflow_step, dict) else None
        )
        return checkpoint_data if isinstance(checkpoint_data, dict) else None

    @property
    def checkpoint_cursor(self) -> Any | None:
        """Get the checkpoint cursor position.

        The cursor represents the current position in batch processing,
        allowing handlers to resume from where they left off.

        Returns:
            The cursor value (int, string, or object) or None if not set.

        Example:
            >>> cursor = context.checkpoint_cursor
            >>> start_from = cursor if cursor is not None else 0
        """
        cp = self.checkpoint
        return cp.get("cursor") if cp else None

    @property
    def checkpoint_items_processed(self) -> int:
        """Get the number of items processed in the current batch run.

        Returns:
            Number of items processed (0 if no checkpoint).
        """
        cp = self.checkpoint
        return cp.get("items_processed", 0) if cp else 0

    @property
    def accumulated_results(self) -> dict[str, Any] | None:
        """Get the accumulated results from batch processing.

        Accumulated results allow handlers to maintain running totals
        or aggregated state across checkpoint boundaries.

        Returns:
            The accumulated results dict or None if not set.

        Example:
            >>> totals = context.accumulated_results or {}
            >>> current_sum = totals.get("sum", 0)
        """
        cp = self.checkpoint
        return cp.get("accumulated_results") if cp else None

    def has_checkpoint(self) -> bool:
        """Check if a checkpoint exists for this step.

        Returns:
            True if a checkpoint cursor exists.

        Example:
            >>> if context.has_checkpoint():
            ...     print(f"Resuming from cursor: {context.checkpoint_cursor}")
        """
        return self.checkpoint_cursor is not None

    def get_dependency_result_keys(self) -> list[str]:
        """Get all dependency result keys.

        Returns:
            List of step names that have dependency results.
        """
        return list(self.dependency_results.keys())

    def get_all_dependency_results(self, prefix: str) -> list[Any]:
        """Get all dependency results matching a step name prefix.

        This is useful for batch processing where multiple worker steps
        share a common prefix (e.g., "process_batch_001", "process_batch_002").

        Returns the unwrapped result values (same as get_dependency_result).

        Args:
            prefix: Step name prefix to match.

        Returns:
            List of unwrapped result values from matching steps.

        Example:
            >>> # For batch worker results named: process_batch_001, process_batch_002, etc.
            >>> batch_results = context.get_all_dependency_results("process_batch_")
            >>> total = sum(r.get("count", 0) for r in batch_results)
        """
        results: list[Any] = []
        for key in self.dependency_results:
            if key.startswith(prefix):
                result = self.get_dependency_result(key)
                if result is not None:
                    results.append(result)
        return results

    @property
    def task_sequence_step(self) -> Any:
        """Get a TaskSequenceStepWrapper for Ruby-like attribute access.

        This provides a wrapper around the raw task_sequence_step dict
        that allows attribute-style access instead of dictionary lookups.
        Matches the Ruby worker's TaskSequenceStepWrapper pattern.

        Returns:
            TaskSequenceStepWrapper instance wrapping the event data.

        Example:
            >>> # Ruby-style access
            >>> task_uuid = context.task_sequence_step.task.task_uuid
            >>> handler_name = context.task_sequence_step.step_definition.handler.callable
            >>>
            >>> # Convenience methods
            >>> value = context.task_sequence_step.get_task_field("even_number")
            >>> result = context.task_sequence_step.get_dependency_result("step_1")
        """
        # Lazy import to avoid circular dependency
        from tasker_core.models import TaskSequenceStepWrapper

        return TaskSequenceStepWrapper.from_dict(self.event.task_sequence_step)


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
        ...     error_type=ErrorType.VALIDATION_ERROR,
        ...     retryable=False,
        ... )
        >>>
        >>> # Failure with error code
        >>> result = StepHandlerResult.failure(
        ...     message="Payment gateway timeout",
        ...     error_type=ErrorType.TIMEOUT,
        ...     retryable=True,
        ...     error_code="GATEWAY_TIMEOUT",
        ... )
    """

    # Note: Field is named `is_success` in Python to avoid conflict with the `success()`
    # classmethod. The alias="success" ensures JSON serialization uses "success" for compatibility.
    # populate_by_name=True allows using either "is_success" or "success" when creating instances.
    is_success: bool = Field(
        alias="success",
        description="Whether the handler executed successfully.",
    )
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
        description="Error type/category for classification. Use ErrorType enum values.",
    )
    error_code: str | None = Field(
        default=None,
        description="Optional application-specific error code.",
    )
    retryable: bool = Field(
        default=True,
        description="Whether the error is retryable.",
    )
    metadata: dict[str, Any] = Field(
        default_factory=dict,
        description="Additional execution metadata.",
    )

    model_config = {"populate_by_name": True}

    @classmethod
    def success(
        cls,
        result: dict[str, Any],
        metadata: dict[str, Any] | None = None,
    ) -> StepHandlerResult:
        """Create a successful handler result.

        This is the primary factory method for creating success results.
        Aligned with Ruby and Rust worker APIs.

        Args:
            result: The handler output data.
            metadata: Optional additional metadata.

        Returns:
            A StepHandlerResult indicating success.

        Example:
            >>> result = StepHandlerResult.success({"processed": 100, "skipped": 5})
        """
        return cls(
            is_success=True,  # type: ignore[call-arg]  # populate_by_name=True allows this
            result=result,
            metadata=metadata or {},
        )

    @classmethod
    def failure(
        cls,
        message: str,
        error_type: str | ErrorType = ErrorType.HANDLER_ERROR,
        retryable: bool = True,
        metadata: dict[str, Any] | None = None,
        error_code: str | None = None,
    ) -> StepHandlerResult:
        """Create a failure handler result.

        Args:
            message: Human-readable error message.
            error_type: Error type/category for classification. Use ErrorType enum
                for cross-language consistency.
            retryable: Whether the error is retryable.
            metadata: Optional additional metadata.
            error_code: Optional application-specific error code (e.g., "PAYMENT_GATEWAY_TIMEOUT").

        Returns:
            A StepHandlerResult indicating failure.

        Example:
            >>> result = StepHandlerResult.failure(
            ...     message="Invalid input format",
            ...     error_type=ErrorType.VALIDATION_ERROR,
            ...     retryable=False,
            ... )
            >>>
            >>> # With error code
            >>> result = StepHandlerResult.failure(
            ...     message="Gateway timeout",
            ...     error_type=ErrorType.TIMEOUT,
            ...     retryable=True,
            ...     error_code="GATEWAY_TIMEOUT",
            ... )
        """
        # Convert ErrorType enum to string value if needed
        error_type_str = error_type.value if isinstance(error_type, ErrorType) else error_type
        return cls(
            is_success=False,  # type: ignore[call-arg]  # populate_by_name=True allows this
            error_message=message,
            error_type=error_type_str,
            error_code=error_code,
            retryable=retryable,
            metadata=metadata or {},
        )

    def is_success_result(self) -> bool:
        """Check if this result indicates success.

        Returns:
            True if the handler executed successfully.

        Example:
            >>> if result.is_success_result():
            ...     print(f"Output: {result.result}")
        """
        return self.is_success

    def is_failure_result(self) -> bool:
        """Check if this result indicates failure.

        Returns:
            True if the handler failed.

        Example:
            >>> if result.is_failure_result():
            ...     print(f"Error: {result.error_message}")
        """
        return not self.is_success


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


class ExecutionResult(BaseModel):
    """Step execution result included in domain events.

    Provides information about the step execution that triggered this event.
    Matches TypeScript's executionResult field for cross-language consistency.

    Example:
        >>> result = ExecutionResult(
        ...     success=True,
        ...     result={"processed": 100},
        ...     execution_time_ms=150,
        ... )
    """

    success: bool = Field(description="Whether the step execution succeeded.")
    result: dict[str, Any] | None = Field(
        default=None,
        description="Step handler's return value (on success).",
    )
    error_message: str | None = Field(
        default=None,
        description="Error message (on failure).",
    )
    error_type: str | None = Field(
        default=None,
        description="Error type/category (on failure).",
    )
    execution_time_ms: int | None = Field(
        default=None,
        description="Execution time in milliseconds.",
    )


class InProcessDomainEvent(BaseModel):
    """Domain event received from in-process polling.

    In-process events use the fast path (tokio broadcast channel)
    for real-time notifications that don't require guaranteed delivery.

    Example:
        >>> event = InProcessDomainEvent.model_validate(poll_in_process_events())
        >>> if event:
        ...     print(f"Received {event.event_name}: {event.payload}")
        ...     if event.execution_result:
        ...         print(f"Step success: {event.execution_result.success}")
    """

    event_id: UUID = Field(description="Unique event ID.")
    event_name: str = Field(description="Event name (e.g., 'step.completed').")
    event_version: str = Field(description="Event schema version.")
    metadata: DomainEventMetadata = Field(description="Event metadata.")
    payload: dict[str, Any] = Field(
        default_factory=dict,
        description="Event payload data.",
    )
    execution_result: ExecutionResult | None = Field(
        default=None,
        description="Step execution result that triggered this event (TAS-112).",
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


# =============================================================================
# Phase 6b: Decision Point and Batch Processing Types
# =============================================================================


class DecisionType(str, Enum):
    """Type of decision point outcome.

    Used by decision handlers to indicate whether to create steps
    or skip branching.
    """

    CREATE_STEPS = "create_steps"
    """Create the specified steps as next steps in the workflow."""

    NO_BRANCHES = "no_branches"
    """Skip branching, no additional steps needed."""


class DecisionPointOutcome(BaseModel):
    """Outcome from a decision point handler.

    Decision handlers return this to indicate which branch(es) of a workflow
    to execute. Supports both static step selection and dynamic step creation.

    Example (Create Steps):
        >>> outcome = DecisionPointOutcome.create_steps(
        ...     ["process_premium", "send_notification"]
        ... )
        >>> result = handler.decision_success(outcome)

    Example (No Branches):
        >>> outcome = DecisionPointOutcome.no_branches(
        ...     reason="No items match criteria"
        ... )
        >>> result = handler.decision_no_branches(outcome)
    """

    decision_type: DecisionType = Field(description="Type of decision outcome.")
    next_step_names: list[str] = Field(
        default_factory=list,
        description="Names of steps to execute (for CREATE_STEPS).",
    )
    dynamic_steps: list[dict[str, Any]] | None = Field(
        default=None,
        description="Dynamically generated step definitions.",
    )
    reason: str | None = Field(
        default=None,
        description="Reason for no branches (for NO_BRANCHES).",
    )
    routing_context: dict[str, Any] = Field(
        default_factory=dict,
        description="Additional context for routing decisions.",
    )

    @classmethod
    def create_steps(
        cls,
        step_names: list[str],
        dynamic_steps: list[dict[str, Any]] | None = None,
        routing_context: dict[str, Any] | None = None,
    ) -> DecisionPointOutcome:
        """Create an outcome that executes specified steps.

        Args:
            step_names: Names of the steps to execute.
            dynamic_steps: Optional dynamically generated step definitions.
            routing_context: Optional context data for routing.

        Returns:
            A DecisionPointOutcome for step creation.

        Example:
            >>> outcome = DecisionPointOutcome.create_steps(
            ...     ["validate_premium", "process_premium"],
            ...     routing_context={"customer_tier": "premium"}
            ... )
        """
        return cls(
            decision_type=DecisionType.CREATE_STEPS,
            next_step_names=step_names,
            dynamic_steps=dynamic_steps,
            routing_context=routing_context or {},
        )

    @classmethod
    def no_branches(
        cls,
        reason: str,
        routing_context: dict[str, Any] | None = None,
    ) -> DecisionPointOutcome:
        """Create an outcome that skips branching.

        Args:
            reason: Human-readable reason for skipping branches.
            routing_context: Optional context data for routing.

        Returns:
            A DecisionPointOutcome for no branching.

        Example:
            >>> outcome = DecisionPointOutcome.no_branches(
            ...     reason="No items require processing"
            ... )
        """
        return cls(
            decision_type=DecisionType.NO_BRANCHES,
            reason=reason,
            routing_context=routing_context or {},
        )


class CursorConfig(BaseModel):
    """Configuration for cursor-based batch processing.

    Defines a range of items to process in a batch worker step.

    Example:
        >>> config = CursorConfig(
        ...     start_cursor=0,
        ...     end_cursor=100,
        ...     step_size=10,
        ...     metadata={"partition": "A"}
        ... )
    """

    start_cursor: int = Field(description="Starting cursor position (inclusive).")
    end_cursor: int = Field(description="Ending cursor position (exclusive).")
    step_size: int = Field(
        default=1,
        description="Size of each processing step.",
    )
    metadata: dict[str, Any] = Field(
        default_factory=dict,
        description="Additional cursor metadata.",
    )


class BatchAnalyzerOutcome(BaseModel):
    """Outcome from a batch analyzer handler.

    Batch analyzers return this to define the cursor ranges that will
    spawn parallel batch worker steps.

    Example:
        >>> outcome = BatchAnalyzerOutcome(
        ...     cursor_configs=[
        ...         CursorConfig(start_cursor=0, end_cursor=100),
        ...         CursorConfig(start_cursor=100, end_cursor=200),
        ...     ],
        ...     total_items=200,
        ... )
    """

    cursor_configs: list[CursorConfig] = Field(
        default_factory=list,
        description="List of cursor configurations for batch workers.",
    )
    total_items: int | None = Field(
        default=None,
        description="Total number of items to process.",
    )
    batch_metadata: dict[str, Any] = Field(
        default_factory=dict,
        description="Metadata to pass to all batch workers.",
    )

    @classmethod
    def from_ranges(
        cls,
        ranges: list[tuple[int, int]],
        step_size: int = 1,
        total_items: int | None = None,
        batch_metadata: dict[str, Any] | None = None,
    ) -> BatchAnalyzerOutcome:
        """Create an outcome from a list of (start, end) ranges.

        Args:
            ranges: List of (start_cursor, end_cursor) tuples.
            step_size: Size of each processing step.
            total_items: Total number of items to process.
            batch_metadata: Metadata to pass to all batch workers.

        Returns:
            A BatchAnalyzerOutcome with cursor configs.

        Example:
            >>> outcome = BatchAnalyzerOutcome.from_ranges(
            ...     [(0, 100), (100, 200), (200, 300)],
            ...     total_items=300,
            ... )
        """
        cursor_configs = [
            CursorConfig(start_cursor=start, end_cursor=end, step_size=step_size)
            for start, end in ranges
        ]
        return cls(
            cursor_configs=cursor_configs,
            total_items=total_items,
            batch_metadata=batch_metadata or {},
        )


class BatchWorkerContext(BaseModel):
    """Context for a batch worker step.

    Provides information about the specific batch this worker should process,
    including cursor position and batch metadata.

    Example:
        >>> context = BatchWorkerContext(
        ...     batch_id="batch_001",
        ...     cursor_config=CursorConfig(start_cursor=0, end_cursor=100),
        ...     batch_index=0,
        ...     total_batches=10,
        ... )
    """

    batch_id: str = Field(description="Unique identifier for this batch.")
    cursor_config: CursorConfig = Field(description="Cursor configuration for this batch.")
    batch_index: int = Field(description="Index of this batch (0-based).")
    total_batches: int = Field(description="Total number of batches.")
    batch_metadata: dict[str, Any] = Field(
        default_factory=dict,
        description="Metadata from the analyzer.",
    )
    # TAS-125: Checkpoint data from previous yields
    checkpoint: dict[str, Any] = Field(
        default_factory=dict,
        description="Checkpoint data from previous yields (cursor, items_processed, accumulated_results).",
    )

    @property
    def start_cursor(self) -> int:
        """Get the starting cursor position."""
        return self.cursor_config.start_cursor

    @property
    def end_cursor(self) -> int:
        """Get the ending cursor position."""
        return self.cursor_config.end_cursor

    @property
    def step_size(self) -> int:
        """Get the processing step size."""
        return self.cursor_config.step_size

    # TAS-125: Checkpoint accessor properties
    @property
    def checkpoint_cursor(self) -> int | str | dict[str, Any] | None:
        """Get checkpoint cursor from previous yield.

        When a handler yields a checkpoint, the cursor position is persisted.
        On re-dispatch, this returns that cursor position to resume from.

        Returns:
            Last persisted cursor position, or None if no checkpoint.

        Example:
            >>> start = batch_ctx.checkpoint_cursor or batch_ctx.start_cursor
        """
        return self.checkpoint.get("cursor")

    @property
    def accumulated_results(self) -> dict[str, Any] | None:
        """Get accumulated results from previous checkpoint yield.

        When a handler yields a checkpoint with accumulated_results, those
        partial aggregations are persisted. On re-dispatch, this returns
        them so the handler can continue accumulating.

        Returns:
            Partial aggregations from previous yields, or None if no checkpoint.

        Example:
            >>> accumulated = batch_ctx.accumulated_results or {"total": 0}
            >>> accumulated["total"] += item.value
        """
        return self.checkpoint.get("accumulated_results")

    def has_checkpoint(self) -> bool:
        """Check if checkpoint exists.

        Use this to determine if this is a fresh execution or a resumption
        from a previous checkpoint yield.

        Returns:
            True if checkpoint data exists with a cursor.

        Example:
            >>> if batch_ctx.has_checkpoint():
            ...     start = batch_ctx.checkpoint_cursor
            ... else:
            ...     start = batch_ctx.start_cursor
        """
        return bool(self.checkpoint and self.checkpoint.get("cursor") is not None)

    @property
    def checkpoint_items_processed(self) -> int:
        """Get items processed count from checkpoint.

        Returns the cumulative count of items processed across all yields.

        Returns:
            Items processed so far, or 0 if no checkpoint.
        """
        return int(self.checkpoint.get("items_processed", 0))

    @classmethod
    def from_step_context(cls, step_context: StepContext) -> BatchWorkerContext | None:
        """Extract batch context from a step context.

        Looks for batch processing information in the step_config
        and input_data to construct a BatchWorkerContext.

        Args:
            step_context: The step execution context.

        Returns:
            BatchWorkerContext if batch info exists, None otherwise.

        Example:
            >>> batch_context = BatchWorkerContext.from_step_context(context)
            >>> if batch_context:
            ...     for i in range(batch_context.start_cursor, batch_context.end_cursor):
            ...         process_item(i)
        """
        # Look for batch context in step_config or input_data
        batch_data = step_context.step_config.get("batch_context")
        if batch_data is None:
            batch_data = step_context.input_data.get("batch_context")

        if batch_data is None:
            return None

        # Extract cursor config
        cursor_data = batch_data.get("cursor_config", {})
        cursor_config = CursorConfig(
            start_cursor=cursor_data.get("start_cursor", 0),
            end_cursor=cursor_data.get("end_cursor", 0),
            step_size=cursor_data.get("step_size", 1),
            metadata=cursor_data.get("metadata", {}),
        )

        # TAS-125: Extract checkpoint from workflow_step if present
        checkpoint: dict[str, Any] = {}
        if hasattr(step_context, "event") and step_context.event:
            task_sequence_step = getattr(step_context.event, "task_sequence_step", None)
            if isinstance(task_sequence_step, dict):
                workflow_step = task_sequence_step.get("workflow_step", {})
                if isinstance(workflow_step, dict):
                    checkpoint = workflow_step.get("checkpoint") or {}

        return cls(
            batch_id=batch_data.get("batch_id", ""),
            cursor_config=cursor_config,
            batch_index=batch_data.get("batch_index", 0),
            total_batches=batch_data.get("total_batches", 1),
            batch_metadata=batch_data.get("batch_metadata", {}),
            checkpoint=checkpoint,
        )


class BatchWorkerOutcome(BaseModel):
    """Outcome from a batch worker step.

    Batch workers return this to report progress and results
    for their assigned cursor range.

    Example:
        >>> outcome = BatchWorkerOutcome(
        ...     items_processed=95,
        ...     items_succeeded=90,
        ...     items_failed=5,
        ...     results=[{"item_id": i, "status": "ok"} for i in range(90)],
        ... )
    """

    items_processed: int = Field(
        default=0,
        description="Total items processed in this batch.",
    )
    items_succeeded: int = Field(
        default=0,
        description="Items successfully processed.",
    )
    items_failed: int = Field(
        default=0,
        description="Items that failed processing.",
    )
    items_skipped: int = Field(
        default=0,
        description="Items skipped (e.g., already processed).",
    )
    results: list[dict[str, Any]] = Field(
        default_factory=list,
        description="Individual item results (optional).",
    )
    errors: list[dict[str, Any]] = Field(
        default_factory=list,
        description="Error details for failed items.",
    )
    last_cursor: int | None = Field(
        default=None,
        description="Last successfully processed cursor position.",
    )
    batch_metadata: dict[str, Any] = Field(
        default_factory=dict,
        description="Additional batch result metadata.",
    )


# =============================================================================
# FFI Boundary Types (TAS-112/TAS-123)
#
# These types match Rust structures that cross the FFI boundary.
# They are serialized by Rust and deserialized by Python workers.
# =============================================================================


class FailureStrategy(str, Enum):
    """Failure strategy for batch processing.

    Matches Rust's `FailureStrategy` enum in `task_template.rs`.
    """

    CONTINUE_ON_FAILURE = "continue_on_failure"
    """Log errors, continue processing remaining items."""

    FAIL_FAST = "fail_fast"
    """Stop immediately on first error."""

    ISOLATE = "isolate"
    """Mark batch for manual investigation."""


class RustCursorConfig(BaseModel):
    """Cursor configuration for a single batch's position and range.

    Matches Rust's `CursorConfig` in `tasker-shared/src/messaging/execution_types.rs`.

    ## Flexible Cursor Types

    Unlike the simpler `CursorConfig` class (which uses `int`),
    this type supports flexible cursor values that can be:
    - Integer for record IDs: `123`
    - String for timestamps: `"2025-11-01T00:00:00Z"`
    - Dict for composite keys: `{"page": 1, "offset": 0}`

    This enables cursor-based pagination across diverse data sources.

    Example:
        >>> # Integer cursors (most common)
        >>> config = RustCursorConfig(
        ...     batch_id="batch_001",
        ...     start_cursor=0,
        ...     end_cursor=1000,
        ...     batch_size=1000,
        ... )

        >>> # Timestamp cursors
        >>> config = RustCursorConfig(
        ...     batch_id="batch_001",
        ...     start_cursor="2025-01-01T00:00:00Z",
        ...     end_cursor="2025-01-02T00:00:00Z",
        ...     batch_size=86400,
        ... )
    """

    batch_id: str = Field(description="Batch identifier (e.g., 'batch_001', 'batch_002').")
    start_cursor: Any = Field(
        description=(
            "Starting position for this batch (inclusive). "
            "Type depends on cursor strategy: int for record IDs, "
            "str for timestamps or UUIDs, dict for composite keys."
        )
    )
    end_cursor: Any = Field(
        description=(
            "Ending position for this batch (exclusive). "
            "Workers process items from start_cursor (inclusive) "
            "up to but not including end_cursor."
        )
    )
    batch_size: int = Field(description="Number of items in this batch (for progress reporting).")


class BatchMetadata(BaseModel):
    """Batch processing metadata from template configuration.

    Matches Rust's `BatchMetadata` in `tasker-shared/src/models/core/batch_worker.rs`.

    This structure extracts relevant template configuration that workers
    need during execution. Workers don't need parallelism settings or
    batch size calculation logic - just execution parameters.
    """

    # TAS-125: checkpoint_interval removed - handlers decide when to checkpoint
    cursor_field: str = Field(
        description=(
            "Database field name used for cursor-based pagination. "
            "Workers use this to construct queries like: "
            "WHERE cursor_field > start_cursor AND cursor_field <= end_cursor"
        )
    )
    failure_strategy: FailureStrategy = Field(
        description="How this worker should handle failures during batch processing."
    )


class RustBatchWorkerInputs(BaseModel):
    """Initialization inputs for batch worker instances.

    Matches Rust's `BatchWorkerInputs` in `tasker-shared/src/models/core/batch_worker.rs`.

    This structure is serialized to JSONB by Rust orchestration and stored
    in `workflow_steps.inputs` for dynamically created batch workers.

    Example:
        >>> # In a batch worker handler
        >>> def call(self, context: StepContext) -> StepHandlerResult:
        ...     inputs = RustBatchWorkerInputs.model_validate(context.step_inputs)
        ...
        ...     # Check for no-op placeholder first
        ...     if inputs.is_no_op:
        ...         return self.success({
        ...             "batch_id": inputs.cursor.batch_id,
        ...             "no_op": True,
        ...             "message": "No batches to process",
        ...         })
        ...
        ...     # Process the batch using cursor bounds
        ...     start, end = inputs.cursor.start_cursor, inputs.cursor.end_cursor
        ...     # ... process items in range
    """

    cursor: RustCursorConfig = Field(
        description=(
            "Cursor configuration defining this worker's processing range. "
            "Created by the batchable handler after analyzing dataset size."
        )
    )
    batch_metadata: BatchMetadata = Field(
        description="Batch processing metadata from template configuration."
    )
    is_no_op: bool = Field(
        description=(
            "Explicit flag indicating if this is a no-op/placeholder worker. "
            "Workers should check this flag FIRST before any processing logic. "
            "If True, immediately return success without processing."
        )
    )


# =============================================================================
# BatchProcessingOutcome - Discriminated Union (TAS-112/TAS-123)
#
# Matches Rust's `BatchProcessingOutcome` enum with tagged serialization.
# Uses Pydantic discriminated unions for type-safe pattern matching.
# =============================================================================


class NoBatchesOutcome(BaseModel):
    """No batches needed - process as single step or skip.

    Returned when:
    - Dataset is too small to warrant batching
    - Data doesn't meet batching criteria
    - Batch processing not applicable for this execution

    Serialization format: `{ "type": "no_batches" }`
    """

    type: str = Field(default="no_batches", description="Discriminator for outcome type.")


class CreateBatchesOutcome(BaseModel):
    """Create batch worker steps from template.

    The orchestration system will:
    1. Instantiate N workers from the template step
    2. Assign each worker a unique cursor config
    3. Create DAG edges from batchable step to workers
    4. Enqueue workers for parallel execution

    Serialization format:
        {
            "type": "create_batches",
            "worker_template_name": "batch_worker_template",
            "worker_count": 5,
            "cursor_configs": [...],
            "total_items": 5000
        }
    """

    type: str = Field(default="create_batches", description="Discriminator for outcome type.")
    worker_template_name: str = Field(
        description=(
            "Template step name to use for creating workers. "
            "Must match a step definition in the template with type: batch_worker."
        )
    )
    worker_count: int = Field(description="Number of worker instances to create.")
    cursor_configs: list[RustCursorConfig] = Field(
        description=(
            "Initial cursor positions for each batch. "
            "Each worker receives one cursor config. Length must equal worker_count."
        )
    )
    total_items: int = Field(description="Total items to process across all batches.")


# Type alias for discriminated union
BatchProcessingOutcome = NoBatchesOutcome | CreateBatchesOutcome


def no_batches() -> NoBatchesOutcome:
    """Create a NoBatches outcome.

    Use when batching is not needed or applicable.

    Returns:
        A NoBatchesOutcome object.
    """
    return NoBatchesOutcome()


def create_batches(
    worker_template_name: str,
    worker_count: int,
    cursor_configs: list[RustCursorConfig],
    total_items: int,
) -> CreateBatchesOutcome:
    """Create a CreateBatches outcome with specified configuration.

    Args:
        worker_template_name: Name of the template step to instantiate.
        worker_count: Number of workers to create.
        cursor_configs: Cursor configuration for each worker.
        total_items: Total number of items to process.

    Returns:
        A CreateBatchesOutcome object.

    Raises:
        ValueError: If cursor_configs length doesn't equal worker_count.

    Example:
        >>> outcome = create_batches(
        ...     worker_template_name="process_csv_batch",
        ...     worker_count=3,
        ...     cursor_configs=[
        ...         RustCursorConfig(batch_id="001", start_cursor=0, end_cursor=1000, batch_size=1000),
        ...         RustCursorConfig(batch_id="002", start_cursor=1000, end_cursor=2000, batch_size=1000),
        ...         RustCursorConfig(batch_id="003", start_cursor=2000, end_cursor=3000, batch_size=1000),
        ...     ],
        ...     total_items=3000,
        ... )
    """
    if len(cursor_configs) != worker_count:
        msg = f"cursor_configs length ({len(cursor_configs)}) must equal worker_count ({worker_count})"
        raise ValueError(msg)

    return CreateBatchesOutcome(
        worker_template_name=worker_template_name,
        worker_count=worker_count,
        cursor_configs=cursor_configs,
        total_items=total_items,
    )


class BatchAggregationResult(BaseModel):
    """Result from aggregating multiple batch worker results.

    Cross-language standard: matches TypeScript's aggregateBatchResults output
    and Ruby's aggregate_batch_worker_results.

    TAS-112: Standardized aggregation result structure.
    """

    total_processed: int = Field(description="Total items processed across all batches.")
    total_succeeded: int = Field(description="Total items that succeeded.")
    total_failed: int = Field(description="Total items that failed.")
    total_skipped: int = Field(description="Total items that were skipped.")
    batch_count: int = Field(description="Number of batch workers that ran.")
    success_rate: float = Field(description="Success rate (0.0 to 1.0).")
    errors: list[dict[str, Any]] = Field(
        default_factory=list,
        description="Collected errors from all batches (limited).",
    )
    error_count: int = Field(description="Total error count (may exceed errors array length).")


def aggregate_batch_results(
    worker_results: list[dict[str, Any] | None],
    max_errors: int = 100,
) -> BatchAggregationResult:
    """Aggregate results from multiple batch workers.

    Cross-language standard: matches TypeScript's `aggregateBatchResults`
    and Ruby's `aggregate_batch_worker_results`.

    Args:
        worker_results: List of results from batch worker steps.
        max_errors: Maximum number of errors to collect (default: 100).

    Returns:
        Aggregated summary of all batch processing.

    Example:
        >>> # In an aggregator handler
        >>> worker_results = [
        ...     context.get_dependency_result(f"worker_{i}")
        ...     for i in range(batch_count)
        ... ]
        >>> summary = aggregate_batch_results(worker_results)
        >>> return self.success(summary.model_dump())
    """
    total_processed = 0
    total_succeeded = 0
    total_failed = 0
    total_skipped = 0
    all_errors: list[dict[str, Any]] = []
    batch_count = 0

    for result in worker_results:
        if result is None:
            continue

        batch_count += 1
        total_processed += result.get("items_processed", 0)
        total_succeeded += result.get("items_succeeded", 0)
        total_failed += result.get("items_failed", 0)
        total_skipped += result.get("items_skipped", 0)

        errors = result.get("errors")
        if errors and isinstance(errors, list):
            all_errors.extend(errors)

    return BatchAggregationResult(
        total_processed=total_processed,
        total_succeeded=total_succeeded,
        total_failed=total_failed,
        total_skipped=total_skipped,
        batch_count=batch_count,
        success_rate=total_succeeded / total_processed if total_processed > 0 else 0.0,
        errors=all_errors[:max_errors],
        error_count=len(all_errors),
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
    "ErrorType",
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
    "ExecutionResult",
    "InProcessDomainEvent",
    "ComponentHealth",
    "HealthCheck",
    "WorkerMetrics",
    "WorkerConfig",
    # Phase 6b: Decision point and batch processing
    "DecisionType",
    "DecisionPointOutcome",
    "CursorConfig",
    "BatchAnalyzerOutcome",
    "BatchWorkerContext",
    "BatchWorkerOutcome",
    # Phase 7: FFI Boundary Types (TAS-112/TAS-123)
    "FailureStrategy",
    "RustCursorConfig",
    "BatchMetadata",
    "RustBatchWorkerInputs",
    "NoBatchesOutcome",
    "CreateBatchesOutcome",
    "BatchProcessingOutcome",
    "no_batches",
    "create_batches",
    "BatchAggregationResult",
    "aggregate_batch_results",
]
