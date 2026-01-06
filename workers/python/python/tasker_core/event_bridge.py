"""In-process event bridge for step execution coordination.

This module provides the EventBridge class that wraps pyee's EventEmitter
to provide pub/sub functionality for coordinating step handlers.

Example:
    >>> from tasker_core import EventBridge
    >>>
    >>> bridge = EventBridge.instance()
    >>> bridge.start()
    >>>
    >>> def on_step_received(event):
    ...     print(f"Processing step {event.step_uuid}")
    ...
    >>> bridge.subscribe("step.execution.received", on_step_received)
    >>> bridge.publish("step.execution.received", event_data)
    >>>
    >>> bridge.stop()
"""

from __future__ import annotations

from collections.abc import Callable
from typing import Any

from pyee.base import EventEmitter

from .logging import log_debug, log_info, log_warn


# Event names used in the system
class EventNames:
    """Constants for event names used in the event bridge.

    Attributes:
        STEP_EXECUTION_RECEIVED: Emitted when a step is ready for execution.
        STEP_COMPLETION_SENT: Emitted after step result is submitted.
        STEP_CHECKPOINT_YIELD: Emitted after checkpoint yield is submitted (TAS-125).
        HANDLER_REGISTERED: Emitted when a new handler is registered.
        HANDLER_ERROR: Emitted when a handler execution fails.
        POLLER_METRICS: Emitted with dispatch metrics updates.
        POLLER_ERROR: Emitted when the poller encounters an error.
    """

    STEP_EXECUTION_RECEIVED = "step.execution.received"
    STEP_COMPLETION_SENT = "step.completion.sent"
    STEP_CHECKPOINT_YIELD = "step.checkpoint_yield.sent"  # TAS-125
    HANDLER_REGISTERED = "handler.registered"
    HANDLER_ERROR = "handler.error"
    POLLER_METRICS = "poller.metrics"
    POLLER_ERROR = "poller.error"


class EventBridge:
    """In-process event bus for step execution coordination.

    This is the Python equivalent of Ruby's dry-events integration,
    providing pub/sub functionality for coordinating step handlers.

    The EventBridge is implemented as a singleton to ensure all components
    share the same event bus.

    Events:
        step.execution.received: Step ready for execution (FfiStepEvent)
        step.completion.sent: Step execution completed (StepExecutionResult)
        handler.registered: New handler registered (name, handler_class)
        handler.error: Handler execution error (FfiStepEvent, Exception)
        poller.metrics: Dispatch metrics update (FfiDispatchMetrics)
        poller.error: Poller error (Exception)

    Example:
        >>> bridge = EventBridge.instance()
        >>> bridge.start()
        >>>
        >>> bridge.subscribe("step.execution.received", my_handler)
        >>> bridge.publish("step.execution.received", event_data)
        >>>
        >>> bridge.stop()
    """

    _instance: EventBridge | None = None

    def __init__(self) -> None:
        """Initialize the EventBridge.

        Creates a new pyee EventEmitter and sets up the event schema.
        Prefer using EventBridge.instance() to get the singleton.
        """
        self._emitter = EventEmitter()
        self._active = False
        self._setup_event_schema()

    @classmethod
    def instance(cls) -> EventBridge:
        """Get the singleton EventBridge instance.

        Returns:
            The singleton EventBridge instance.

        Example:
            >>> bridge = EventBridge.instance()
            >>> assert bridge is EventBridge.instance()  # Same instance
        """
        if cls._instance is None:
            cls._instance = cls()
        return cls._instance

    @classmethod
    def reset_instance(cls) -> None:
        """Reset the singleton instance.

        This is primarily for testing to ensure a clean state between tests.
        Stops the current instance if active before resetting.

        Example:
            >>> EventBridge.reset_instance()
            >>> # Fresh instance for next test
        """
        if cls._instance is not None:
            cls._instance.stop()
        cls._instance = None

    def _setup_event_schema(self) -> None:
        """Define the event schema for documentation and validation."""
        self._event_schema: dict[str, str] = {
            EventNames.STEP_EXECUTION_RECEIVED: "FfiStepEvent",
            EventNames.STEP_COMPLETION_SENT: "StepExecutionResult",
            EventNames.STEP_CHECKPOINT_YIELD: "dict[str, Any]",  # TAS-125
            EventNames.HANDLER_REGISTERED: "tuple[str, type]",
            EventNames.HANDLER_ERROR: "tuple[FfiStepEvent, Exception]",
            EventNames.POLLER_METRICS: "FfiDispatchMetrics",
            EventNames.POLLER_ERROR: "Exception",
        }

    def start(self) -> None:
        """Activate the event bridge.

        Events will only be published when the bridge is active.
        Calling start() multiple times is safe (no-op if already active).

        Example:
            >>> bridge = EventBridge.instance()
            >>> bridge.start()
            >>> assert bridge.is_active
        """
        if self._active:
            return
        self._active = True
        log_info("EventBridge started")

    def stop(self) -> None:
        """Deactivate the event bridge.

        Removes all listeners and deactivates the bridge.
        Calling stop() multiple times is safe (no-op if not active).

        Example:
            >>> bridge.stop()
            >>> assert not bridge.is_active
        """
        if not self._active:
            return
        self._active = False
        self._emitter.remove_all_listeners()
        log_info("EventBridge stopped")

    def subscribe(
        self,
        event: str,
        handler: Callable[..., Any],
    ) -> None:
        """Subscribe to an event.

        Args:
            event: Event name to subscribe to.
            handler: Callback function to invoke when event is published.

        Example:
            >>> def my_handler(data):
            ...     print(f"Received: {data}")
            >>> bridge.subscribe("my.event", my_handler)
        """
        self._emitter.on(event, handler)
        handler_name = getattr(handler, "__name__", str(handler))
        log_debug(f"Subscribed to {event}: {handler_name}")

    def subscribe_once(
        self,
        event: str,
        handler: Callable[..., Any],
    ) -> None:
        """Subscribe to an event for a single invocation.

        The handler will be automatically unsubscribed after the first
        event is received.

        Args:
            event: Event name to subscribe to.
            handler: Callback function to invoke once.

        Example:
            >>> bridge.subscribe_once("one.time.event", my_handler)
        """
        self._emitter.once(event, handler)
        handler_name = getattr(handler, "__name__", str(handler))
        log_debug(f"Subscribed once to {event}: {handler_name}")

    def unsubscribe(
        self,
        event: str,
        handler: Callable[..., Any],
    ) -> None:
        """Unsubscribe from an event.

        Args:
            event: Event name to unsubscribe from.
            handler: The handler callback to remove.

        Example:
            >>> bridge.unsubscribe("my.event", my_handler)
        """
        self._emitter.remove_listener(event, handler)
        handler_name = getattr(handler, "__name__", str(handler))
        log_debug(f"Unsubscribed from {event}: {handler_name}")

    def publish(self, event: str, *args: Any, **kwargs: Any) -> None:
        """Publish an event to all subscribers.

        Events are only delivered when the bridge is active.
        If the bridge is not active, a warning is logged and the
        event is dropped.

        Args:
            event: Event name to publish.
            *args: Positional arguments passed to handlers.
            **kwargs: Keyword arguments passed to handlers.

        Example:
            >>> bridge.publish("step.execution.received", event_data)
            >>> bridge.publish("handler.error", event=event, error=exception)
        """
        if not self._active:
            log_warn(f"EventBridge not active, dropping event: {event}")
            return

        log_debug(f"Publishing event: {event}")
        self._emitter.emit(event, *args, **kwargs)

    def listener_count(self, event: str) -> int:
        """Get the number of listeners for an event.

        Args:
            event: Event name to check.

        Returns:
            Number of listeners subscribed to the event.

        Example:
            >>> count = bridge.listener_count("step.execution.received")
        """
        return len(self._emitter.listeners(event))

    def listeners(self, event: str) -> list[Callable[..., Any]]:
        """Get all listeners for an event.

        Args:
            event: Event name to get listeners for.

        Returns:
            List of handler callbacks.

        Example:
            >>> handlers = bridge.listeners("step.execution.received")
        """
        return list(self._emitter.listeners(event))

    @property
    def is_active(self) -> bool:
        """Check if the event bridge is active.

        Returns:
            True if the bridge is active and will deliver events.

        Example:
            >>> if bridge.is_active:
            ...     bridge.publish("my.event", data)
        """
        return self._active

    @property
    def event_schema(self) -> dict[str, str]:
        """Get the event schema documentation.

        Returns:
            Dict mapping event names to their expected payload types.

        Example:
            >>> schema = bridge.event_schema
            >>> print(schema["step.execution.received"])
            'FfiStepEvent'
        """
        return self._event_schema.copy()


__all__ = ["EventBridge", "EventNames"]
