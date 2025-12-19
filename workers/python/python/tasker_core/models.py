"""Wrapper classes for FFI data structures.

This module provides Python-friendly wrappers for the data structures
received from the Rust FFI layer. These classes mirror the Ruby
implementation in workers/ruby/lib/tasker_core/models.rb.

The wrappers provide:
- Type-safe attribute access instead of dictionary key lookups
- Convenience methods for common operations
- Consistent API between Python and Ruby handlers

Example:
    >>> from tasker_core.models import TaskSequenceStepWrapper
    >>>
    >>> # In a handler
    >>> def call(self, context: StepContext) -> StepHandlerResult:
    ...     # Access task context
    ...     even_number = context.task_sequence_step.get_task_field("even_number")
    ...
    ...     # Access dependency results
    ...     previous = context.task_sequence_step.get_dependency_result("step_1")
    ...
    ...     return StepHandlerResult.success_handler_result({"value": even_number * 2})
"""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime
from typing import Any


@dataclass
class HandlerWrapper:
    """Wrapper for handler configuration from task template.

    Provides access to handler class name and initialization parameters.

    Attributes:
        callable: Fully-qualified handler class name (e.g., "linear_workflow.step_handlers.LinearStep1Handler")
        initialization: Handler initialization parameters from template

    Example:
        >>> handler.callable
        'linear_workflow.step_handlers.LinearStep1Handler'
        >>> handler.initialization
        {'operation': 'square', 'step_number': 1}
    """

    callable: str
    initialization: dict[str, Any] = field(default_factory=dict)

    @classmethod
    def from_dict(cls, data: dict[str, Any] | None) -> HandlerWrapper:
        """Create a HandlerWrapper from FFI dictionary data.

        Args:
            data: Handler configuration dictionary from FFI

        Returns:
            HandlerWrapper instance
        """
        if data is None:
            data = {}
        return cls(
            callable=data.get("callable", ""),
            initialization=data.get("initialization") or {},
        )


@dataclass
class StepDefinitionWrapper:
    """Wrapper for step definition from task template.

    Provides access to step configuration including handler specification,
    dependencies, retry policy, and timeout settings.

    Attributes:
        name: Step name from template
        description: Human-readable description
        handler: Handler configuration wrapper
        system_dependency: System dependency name (optional)
        dependencies: Names of parent steps this depends on
        retry: Retry configuration dictionary
        timeout_seconds: Timeout in seconds
        publishes_events: Events this step publishes

    Example:
        >>> step_def.name
        'linear_step_1'
        >>> step_def.handler.callable
        'linear_workflow.step_handlers.LinearStep1Handler'
        >>> step_def.dependencies
        ['previous_step']
    """

    name: str
    description: str
    handler: HandlerWrapper
    system_dependency: str | None = None
    dependencies: list[str] = field(default_factory=list)
    retry: dict[str, Any] = field(default_factory=dict)
    timeout_seconds: int = 30
    publishes_events: list[str] = field(default_factory=list)

    @classmethod
    def from_dict(cls, data: dict[str, Any] | None) -> StepDefinitionWrapper:
        """Create a StepDefinitionWrapper from FFI dictionary data.

        Args:
            data: Step definition dictionary from FFI

        Returns:
            StepDefinitionWrapper instance
        """
        if data is None:
            data = {}
        return cls(
            name=data.get("name", ""),
            description=data.get("description", ""),
            handler=HandlerWrapper.from_dict(data.get("handler")),
            system_dependency=data.get("system_dependency"),
            dependencies=data.get("dependencies") or [],
            retry=data.get("retry") or {},
            timeout_seconds=data.get("timeout_seconds", 30),
            publishes_events=data.get("publishes_events") or [],
        )


@dataclass
class DependencyResultsWrapper:
    """Wrapper for dependency results from parent steps.

    Provides access to results from steps that this step depends on.
    Results are keyed by step name.

    Two methods for accessing results:
    - `get_result(name)` returns the full result hash with metadata
    - `get_results(name)` returns just the computed value (recommended)

    Example:
        >>> deps.get_results("previous_step")  # Just the value
        36
        >>> deps.get_result("previous_step")   # Full result with metadata
        {'result': 36, 'metadata': {...}}
        >>> deps["previous_step"]              # Same as get_result
        {'result': 36, 'metadata': {...}}
    """

    _results: dict[str, Any] = field(default_factory=dict)

    @classmethod
    def from_dict(cls, data: dict[str, Any] | None) -> DependencyResultsWrapper:
        """Create a DependencyResultsWrapper from FFI dictionary data.

        Args:
            data: Dependency results dictionary from FFI

        Returns:
            DependencyResultsWrapper instance
        """
        return cls(_results=data or {})

    def get_result(self, step_name: str) -> Any:
        """Get result from a parent step (returns full result hash).

        Args:
            step_name: Name of the parent step

        Returns:
            The full result hash or None if not found
        """
        return self._results.get(step_name)

    def get_results(self, step_name: str) -> Any:
        """Get the actual result value from a parent step.

        This is the typical method handlers use to get the actual computed
        value from a parent step, rather than the full result metadata hash.

        Args:
            step_name: Name of the parent step

        Returns:
            The result value or None if not found

        Example:
            >>> deps.get_results("linear_step_1")
            36  # The actual value
            >>> deps.get_result("linear_step_1")
            {'result': 36, 'metadata': {...}}  # Full metadata
        """
        result_hash = self._results.get(step_name)
        if result_hash is None:
            return None

        # If it's a dict with a 'result' key, extract that value
        # Otherwise return the whole thing (might be a primitive value)
        if isinstance(result_hash, dict) and "result" in result_hash:
            return result_hash["result"]
        return result_hash

    def __getitem__(self, step_name: str) -> Any:
        """Array-style access to dependency results.

        Args:
            step_name: Name of the parent step

        Returns:
            The full result hash or None if not found
        """
        return self.get_result(step_name)

    def keys(self) -> list[str]:
        """Get all dependency result step names.

        Returns:
            List of step names that have results
        """
        return list(self._results.keys())

    def __contains__(self, step_name: str) -> bool:
        """Check if a dependency result exists.

        Args:
            step_name: Name of the parent step

        Returns:
            True if result exists
        """
        return step_name in self._results


@dataclass
class WorkflowStepWrapper:
    """Wrapper for workflow step execution state and metadata.

    Provides access to step execution tracking, retry configuration, and results.

    Attributes:
        workflow_step_uuid: UUID of the workflow step instance
        task_uuid: UUID of the task this step belongs to
        named_step_uuid: UUID of the named step definition
        name: Step name from template
        retryable: Whether step can be retried
        max_attempts: Maximum retry attempts
        in_process: Whether step is currently being processed
        processed: Whether step has been processed
        processed_at: When step was last processed
        attempts: Number of execution attempts
        last_attempted_at: When step was last attempted
        backoff_request_seconds: Backoff delay in seconds for retry
        inputs: Step inputs from template
        results: Step execution results
        skippable: Whether step can be skipped
        created_at: When step was created
        updated_at: When step was last updated

    Example:
        >>> step.name
        'linear_step_1'
        >>> step.attempts
        1
        >>> step.max_attempts
        3
    """

    workflow_step_uuid: str
    task_uuid: str
    name: str
    named_step_uuid: str | None = None
    retryable: bool = True
    max_attempts: int = 3
    in_process: bool = False
    processed: bool = False
    processed_at: datetime | None = None
    attempts: int = 0
    last_attempted_at: datetime | None = None
    backoff_request_seconds: int = 0
    inputs: dict[str, Any] = field(default_factory=dict)
    results: dict[str, Any] = field(default_factory=dict)
    skippable: bool = False
    created_at: datetime | None = None
    updated_at: datetime | None = None

    @classmethod
    def from_dict(cls, data: dict[str, Any] | None) -> WorkflowStepWrapper:
        """Create a WorkflowStepWrapper from FFI dictionary data.

        Args:
            data: Workflow step dictionary from FFI

        Returns:
            WorkflowStepWrapper instance
        """
        if data is None:
            data = {}
        return cls(
            workflow_step_uuid=data.get("workflow_step_uuid", ""),
            task_uuid=data.get("task_uuid", ""),
            named_step_uuid=data.get("named_step_uuid"),
            name=data.get("name", ""),
            retryable=data.get("retryable", True),
            max_attempts=data.get("max_attempts", 3),
            in_process=data.get("in_process", False),
            processed=data.get("processed", False),
            processed_at=data.get("processed_at"),
            attempts=data.get("attempts", 0),
            last_attempted_at=data.get("last_attempted_at"),
            backoff_request_seconds=data.get("backoff_request_seconds", 0),
            inputs=data.get("inputs") or {},
            results=data.get("results") or {},
            skippable=data.get("skippable", False),
            created_at=data.get("created_at"),
            updated_at=data.get("updated_at"),
        )


@dataclass
class TaskWrapper:
    """Wrapper for task metadata and context.

    Handles the nested task structure from Rust FFI where the actual task
    data is nested under a 'task' key within the parent task hash.

    Attributes:
        task_uuid: UUID of the task instance
        context: Task context with input data and accumulated state
        namespace_name: Namespace name from task template
        task_name: Task template name
        task_version: Task template version
        correlation_id: Correlation ID for tracing
        parent_correlation_id: Parent correlation ID (for nested tasks)

    Example:
        >>> task.task_uuid
        '0199a46a-11a8-7d53-83da-0b13513dab49'
        >>> task.context
        {'even_number': 2}
        >>> task.namespace_name
        'linear_workflow'
    """

    task_uuid: str
    context: dict[str, Any] = field(default_factory=dict)
    namespace_name: str = ""
    task_name: str = ""
    task_version: str = ""
    correlation_id: str | None = None
    parent_correlation_id: str | None = None

    @classmethod
    def from_dict(cls, data: dict[str, Any] | None) -> TaskWrapper:
        """Create a TaskWrapper from FFI dictionary data.

        Handles the nested task structure where actual task fields
        are under a 'task' key.

        Args:
            data: Task data dictionary from FFI

        Returns:
            TaskWrapper instance
        """
        if data is None:
            data = {}

        # Handle nested task structure from Rust FFI
        # Data comes as: { task: { task_uuid: ..., context: ... }, task_name: "...", ... }
        inner_task = data.get("task") or data

        return cls(
            task_uuid=inner_task.get("task_uuid", ""),
            context=inner_task.get("context") or {},
            namespace_name=data.get("namespace_name") or inner_task.get("namespace_name", ""),
            task_name=data.get("task_name") or inner_task.get("task_name", ""),
            task_version=data.get("task_version") or inner_task.get("task_version", ""),
            correlation_id=inner_task.get("correlation_id"),
            parent_correlation_id=inner_task.get("parent_correlation_id"),
        )


@dataclass
class TaskSequenceStepWrapper:
    """Wrapper for TaskSequenceStep data received from Rust FFI.

    This class provides a Python-friendly interface to the complete step
    execution context, including task metadata, workflow step state,
    dependency results, and step definition.

    Attributes:
        task: Wrapped task with context and metadata
        workflow_step: Wrapped workflow step with execution state
        dependency_results: Results from parent steps
        step_definition: Step definition from task template

    Example:
        >>> # Accessing task context
        >>> even_number = wrapper.get_task_field("even_number")
        >>>
        >>> # Accessing dependency results
        >>> previous_result = wrapper.get_dependency_result("previous_step")
    """

    task: TaskWrapper
    workflow_step: WorkflowStepWrapper
    dependency_results: DependencyResultsWrapper
    step_definition: StepDefinitionWrapper

    @classmethod
    def from_dict(cls, data: dict[str, Any] | None) -> TaskSequenceStepWrapper:
        """Create a TaskSequenceStepWrapper from FFI dictionary data.

        Args:
            data: The complete step execution data from Rust

        Returns:
            TaskSequenceStepWrapper instance
        """
        if data is None:
            data = {}
        return cls(
            task=TaskWrapper.from_dict(data.get("task")),
            workflow_step=WorkflowStepWrapper.from_dict(data.get("workflow_step")),
            dependency_results=DependencyResultsWrapper.from_dict(data.get("dependency_results")),
            step_definition=StepDefinitionWrapper.from_dict(data.get("step_definition")),
        )

    def get_task_field(self, field_name: str) -> Any:
        """Convenience method to access task context fields.

        Args:
            field_name: The field name in task context

        Returns:
            The field value or None if not found

        Example:
            >>> wrapper.get_task_field("even_number")
            2
        """
        return self.task.context.get(field_name)

    def get_dependency_result(self, step_name: str) -> Any:
        """Convenience method to access dependency results.

        Returns just the computed value, not the full metadata hash.

        Args:
            step_name: The name of the parent step

        Returns:
            The result from the parent step or None

        Example:
            >>> wrapper.get_dependency_result("linear_step_1")
            36
        """
        return self.dependency_results.get_results(step_name)

    def get_results(self, step_name: str) -> Any:
        """Alias for get_dependency_result for Ruby compatibility.

        Args:
            step_name: The name of the parent step

        Returns:
            The result from the parent step or None
        """
        return self.dependency_results.get_results(step_name)
