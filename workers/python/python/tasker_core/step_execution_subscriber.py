"""Step execution subscriber that routes events to handlers.

This module provides the StepExecutionSubscriber class that subscribes
to step execution events and routes them to the appropriate handlers.

Example:
    >>> from tasker_core import EventBridge, HandlerRegistry, StepExecutionSubscriber
    >>>
    >>> bridge = EventBridge.instance()
    >>> registry = HandlerRegistry.instance()
    >>> subscriber = StepExecutionSubscriber(
    ...     event_bridge=bridge,
    ...     handler_registry=registry,
    ...     worker_id="worker-001",
    ... )
    >>>
    >>> bridge.start()
    >>> subscriber.start()
    >>>
    >>> # Events flow to handlers automatically
    >>>
    >>> subscriber.stop()
    >>> bridge.stop()
"""

from __future__ import annotations

import time
import traceback
from typing import TYPE_CHECKING
from uuid import UUID

from tasker_core._tasker_core import complete_step_event as _complete_step_event

from .event_bridge import EventBridge, EventNames
from .handler import HandlerRegistry, StepHandler
from .logging import log_debug, log_error, log_info, log_warn
from .types import (
    FfiStepEvent,
    StepContext,
    StepError,
    StepExecutionResult,
    StepHandlerResult,
)

if TYPE_CHECKING:
    pass


class StepExecutionSubscriber:
    """Subscriber that routes step events to handlers.

    Subscribes to step.execution.received events from the EventBridge
    and routes them to the appropriate handler based on the handler_name
    in the event payload.

    The execution flow is:
    1. Receive step.execution.received event
    2. Resolve handler from HandlerRegistry
    3. Create StepContext from event
    4. Execute handler.call(context)
    5. Convert result to StepExecutionResult
    6. Submit result via FFI complete_step_event()
    7. Publish step.completion.sent event

    Errors at any stage are caught and converted to failure results.

    Example:
        >>> subscriber = StepExecutionSubscriber(
        ...     event_bridge=EventBridge.instance(),
        ...     handler_registry=HandlerRegistry.instance(),
        ...     worker_id="worker-001",
        ... )
        >>>
        >>> subscriber.start()
        >>> # Events are now being routed to handlers
        >>>
        >>> subscriber.stop()
    """

    def __init__(
        self,
        event_bridge: EventBridge,
        handler_registry: HandlerRegistry,
        worker_id: str,
    ) -> None:
        """Initialize the StepExecutionSubscriber.

        Args:
            event_bridge: The EventBridge to subscribe to.
            handler_registry: The HandlerRegistry to resolve handlers from.
            worker_id: Worker ID to include in results.
        """
        self._event_bridge = event_bridge
        self._registry = handler_registry
        self._worker_id = worker_id
        self._active = False

    def start(self) -> None:
        """Start subscribing to execution events.

        Subscribes to step.execution.received events on the EventBridge.
        Safe to call multiple times (no-op if already active).

        Example:
            >>> subscriber.start()
            >>> assert subscriber.is_active
        """
        if self._active:
            return

        self._event_bridge.subscribe(
            EventNames.STEP_EXECUTION_RECEIVED,
            self._handle_execution_event,
        )
        self._active = True
        log_info("StepExecutionSubscriber started")

    def stop(self) -> None:
        """Stop subscribing to execution events.

        Unsubscribes from step.execution.received events.
        Safe to call multiple times (no-op if not active).

        Example:
            >>> subscriber.stop()
            >>> assert not subscriber.is_active
        """
        if not self._active:
            return

        self._event_bridge.unsubscribe(
            EventNames.STEP_EXECUTION_RECEIVED,
            self._handle_execution_event,
        )
        self._active = False
        log_info("StepExecutionSubscriber stopped")

    @property
    def is_active(self) -> bool:
        """Check if the subscriber is active.

        Returns:
            True if the subscriber is listening for events.
        """
        return self._active

    @property
    def worker_id(self) -> str:
        """Get the worker ID.

        Returns:
            The worker ID string.
        """
        return self._worker_id

    def _handle_execution_event(self, event: FfiStepEvent) -> None:
        """Handle a step execution event.

        This is the main entry point called by the EventBridge when
        a step.execution.received event is published.

        Args:
            event: The FfiStepEvent to process.
        """
        start_time = time.time()

        # Extract handler name from task_sequence_step
        handler_name = self._get_handler_name(event)

        log_info(
            f"Executing step {event.step_uuid} with handler {handler_name}",
            {
                "step_uuid": event.step_uuid,
                "handler": handler_name,
                "correlation_id": event.correlation_id,
            },
        )

        try:
            # Resolve handler
            handler = self._registry.resolve(handler_name)
            if handler is None:
                handler_result = self._create_handler_not_found_result(handler_name)
            else:
                # Execute handler
                handler_result = self._execute_handler(event, handler)

        except Exception as e:
            log_error(f"Error executing handler {handler_name}: {e}")
            handler_result = self._create_error_result(e)
            self._event_bridge.publish(EventNames.HANDLER_ERROR, event, e)

        # Calculate execution time
        execution_time_ms = int((time.time() - start_time) * 1000)

        # Submit result via FFI
        self._submit_result(event, handler_result, execution_time_ms)

    def _get_handler_name(self, event: FfiStepEvent) -> str:
        """Extract handler name from the event.

        The handler name is the callable string from step_definition.handler.callable.
        This matches the Ruby worker's implementation which uses:
            step_data.step_definition.handler.callable

        For batch processing with dynamically created steps, we also check
        workflow_step.template_step_name as a fallback.

        Args:
            event: The FfiStepEvent to extract from.

        Returns:
            The handler name string (fully qualified handler class name).
        """
        tss = event.task_sequence_step

        # Primary: step_definition.handler.callable (matches Ruby implementation)
        step_definition = tss.get("step_definition", {})
        handler = step_definition.get("handler", {})
        callable_name = handler.get("callable")
        if callable_name:
            return str(callable_name)

        # Fallback for batch processing: workflow_step.template_step_name
        workflow_step = tss.get("workflow_step", {})
        template_step_name = workflow_step.get("template_step_name")
        if template_step_name:
            return str(template_step_name)

        # Legacy: step_template.handler_class
        step_template = tss.get("step_template", {})
        handler_class = step_template.get("handler_class")
        if handler_class:
            return str(handler_class)

        # Fall back to unknown
        log_warn(
            "Could not resolve handler name from step_definition.handler.callable",
            {"step_uuid": event.step_uuid, "task_uuid": event.task_uuid},
        )
        return "unknown_handler"

    def _execute_handler(
        self,
        event: FfiStepEvent,
        handler: StepHandler,
    ) -> StepHandlerResult:
        """Execute a handler and return the result.

        Args:
            event: The FfiStepEvent to process.
            handler: The resolved handler instance.

        Returns:
            StepHandlerResult from handler execution.
        """
        # Create context from event
        context = StepContext.from_ffi_event(event, handler.name)

        log_debug(
            f"Calling handler {handler.name}",
            {
                "step_uuid": event.step_uuid,
                "retry_count": str(context.retry_count),
            },
        )

        # Execute handler
        return handler.call(context)

    def _create_handler_not_found_result(
        self,
        handler_name: str,
    ) -> StepHandlerResult:
        """Create result for handler not found.

        Args:
            handler_name: The name of the handler that was not found.

        Returns:
            StepHandlerResult indicating handler not found error.
        """
        return StepHandlerResult.failure_handler_result(
            message=f"Handler not found: {handler_name}",
            error_type="handler_not_found",
            retryable=False,
        )

    def _create_error_result(
        self,
        error: Exception,
    ) -> StepHandlerResult:
        """Create result for handler execution error.

        Args:
            error: The exception that occurred.

        Returns:
            StepHandlerResult indicating handler error.
        """
        return StepHandlerResult.failure_handler_result(
            message=str(error),
            error_type=error.__class__.__name__,
            retryable=True,
            metadata={"traceback": traceback.format_exc()},
        )

    def _submit_result(
        self,
        event: FfiStepEvent,
        handler_result: StepHandlerResult,
        execution_time_ms: int,
    ) -> None:
        """Submit result via FFI and publish completion event.

        Converts the StepHandlerResult to StepExecutionResult and
        submits it via the FFI complete_step_event function.

        Args:
            event: The original FfiStepEvent.
            handler_result: The result from handler execution.
            execution_time_ms: Execution time in milliseconds.
        """
        # Convert handler result to step execution result
        step_uuid = UUID(event.step_uuid)
        task_uuid = UUID(event.task_uuid)
        correlation_id = UUID(event.correlation_id)

        if handler_result.success:
            result = StepExecutionResult.success_result(
                step_uuid=step_uuid,
                task_uuid=task_uuid,
                result=handler_result.result or {},
                execution_time_ms=execution_time_ms,
                worker_id=self._worker_id,
                correlation_id=correlation_id,
                metadata=handler_result.metadata,
            )
        else:
            error = StepError(
                error_type=handler_result.error_type or "unknown",
                message=handler_result.error_message or "Unknown error",
                retryable=handler_result.retryable,
                metadata=handler_result.metadata,
            )
            result = StepExecutionResult.failure_result(
                step_uuid=step_uuid,
                task_uuid=task_uuid,
                error=error,
                execution_time_ms=execution_time_ms,
                worker_id=self._worker_id,
                correlation_id=correlation_id,
            )

        # Submit via FFI
        try:
            result_dict = result.model_dump(mode="json")
            success = _complete_step_event(str(event.event_id), result_dict)

            if success:
                log_debug(
                    "Step completion submitted",
                    {"event_id": event.event_id, "success": str(handler_result.success)},
                )
                self._event_bridge.publish(EventNames.STEP_COMPLETION_SENT, result)
            else:
                log_error(
                    "Failed to submit step completion",
                    {"event_id": event.event_id},
                )

        except Exception as e:
            log_error(f"Failed to submit result: {e}")
            # Result submission failure is serious - log but don't retry
            raise


class StepExecutionError(Exception):
    """Error during step execution."""

    def __init__(
        self,
        message: str,
        error_type: str = "step_execution_error",
        retryable: bool = True,
    ) -> None:
        """Initialize the error.

        Args:
            message: Error message.
            error_type: Error type for classification.
            retryable: Whether the error is retryable.
        """
        super().__init__(message)
        self.error_type = error_type
        self.retryable = retryable


__all__ = ["StepExecutionSubscriber", "StepExecutionError"]
