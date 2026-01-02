"""Domain events module for real-time notifications and event publishing.

This module provides:
- InProcessDomainEventPoller: Polls for domain events from the broadcast channel
- BasePublisher: Abstract base class for step event publishers with lifecycle hooks
- BaseSubscriber: Abstract base class for event subscribers with lifecycle hooks
- PublisherRegistry: Centralized registry for custom publishers
- SubscriberRegistry: Centralized registry for subscriber management
- StepEventContext: Context data for publishing step events

In-process events are used for real-time notifications that don't require
guaranteed delivery, such as:
- Slack notifications
- Real-time metrics updates
- UI refresh signals
- Log aggregation

For durable domain events (guaranteed delivery), the Rust orchestration
layer publishes events to PGMQ automatically after step completion.

Example (Publishing):
    >>> from tasker_core.domain_events import BasePublisher, StepEventContext
    >>>
    >>> class PaymentEventPublisher(BasePublisher):
    ...     @property
    ...     def publisher_name(self) -> str:
    ...         return "payment_event_publisher"
    ...
    ...     def transform_payload(self, ctx: StepEventContext) -> dict:
    ...         return {"payment_id": ctx.result.get("payment_id")}
    ...
    ...     def before_publish(self, event_name: str, payload: dict, metadata: dict) -> bool:
    ...         # Custom pre-publish validation
    ...         return payload.get("payment_id") is not None

Example (Subscribing):
    >>> from tasker_core.domain_events import BaseSubscriber
    >>>
    >>> class OrderCompletedSubscriber(BaseSubscriber):
    ...     @classmethod
    ...     def subscribes_to(cls) -> list[str]:
    ...         return ["order.completed", "order.updated"]
    ...
    ...     def handle(self, event: dict) -> None:
    ...         print(f"Order event: {event}")

Example (Registry Usage):
    >>> from tasker_core.domain_events import PublisherRegistry, SubscriberRegistry
    >>>
    >>> # Register custom publishers
    >>> pub_registry = PublisherRegistry.instance()
    >>> pub_registry.register(PaymentEventPublisher())
    >>>
    >>> # Register subscribers
    >>> sub_registry = SubscriberRegistry.instance()
    >>> sub_registry.register(OrderCompletedSubscriber)

Example (Polling):
    >>> from tasker_core import InProcessDomainEventPoller
    >>>
    >>> poller = InProcessDomainEventPoller()
    >>> poller.on_event(lambda e: print(f"Received {e.event_name}"))
    >>> poller.start()
"""

from __future__ import annotations

import threading
import time
from abc import ABC, abstractmethod
from collections.abc import Callable
from typing import Any, Literal

from pydantic import BaseModel, Field

from tasker_core._tasker_core import (  # type: ignore[attr-defined]
    poll_in_process_events as _poll_in_process_events,
)

from .logging import log_debug, log_error, log_info, log_warn
from .types import InProcessDomainEvent

# =============================================================================
# Cross-Language Type Definitions (TAS-112)
#
# These types match TypeScript's domain-events.ts for consistency.
# =============================================================================


class EventDeclaration(BaseModel):
    """Event declaration from YAML task template.

    Represents event configuration declared in step templates.
    This structure matches TypeScript's EventDeclaration interface.

    Example:
        >>> declaration = EventDeclaration(
        ...     name="payment.processed",
        ...     condition="success",
        ...     delivery_mode="broadcast",
        ...     publisher="PaymentEventPublisher",
        ... )
    """

    name: str = Field(description="Event name (e.g., 'payment.processed').")
    condition: (
        Literal["success", "failure", "retryable_failure", "permanent_failure", "always"] | None
    ) = Field(
        default=None,
        description="Publication condition: 'success', 'failure', 'retryable_failure', 'permanent_failure', 'always'.",
    )
    delivery_mode: Literal["durable", "fast", "broadcast"] | None = Field(
        default=None,
        description="Delivery mode: 'durable' (PGMQ), 'fast' (in-process), 'broadcast' (both).",
    )
    publisher: str | None = Field(
        default=None,
        description="Custom publisher name (optional).",
    )
    schema_: dict[str, Any] | None = Field(
        default=None,
        alias="schema",
        description="JSON schema for payload validation (optional).",
    )

    model_config = {"populate_by_name": True}

    @property
    def schema(self) -> dict[str, Any] | None:  # type: ignore[override]
        """Get the JSON schema for payload validation.

        Note: This property provides access to the schema field while avoiding
        conflict with Pydantic's deprecated schema() classmethod (Pydantic v1 compat).
        """
        return self.schema_


class StepResult(BaseModel):
    """Step result passed to publishers.

    A simplified representation of step execution results for event publishing.
    This is lighter than StepHandlerResult and matches TypeScript's StepResult.

    Example:
        >>> result = StepResult(
        ...     success=True,
        ...     result={"transaction_id": "tx-123", "amount": 100.00},
        ...     metadata={"duration_ms": 150},
        ... )
    """

    success: bool = Field(description="Whether the step succeeded.")
    result: dict[str, Any] | None = Field(
        default=None,
        description="Step handler's return value.",
    )
    metadata: dict[str, Any] | None = Field(
        default=None,
        description="Execution metadata.",
    )


class PublishContext(BaseModel):
    """Context passed to publisher's publish() method.

    Cross-language standard API (TAS-96). Provides all information needed
    for publishing an event in a single structure.

    Example:
        >>> ctx = PublishContext(
        ...     event_name="payment.processed",
        ...     step_result=StepResult(success=True, result={"amount": 100}),
        ...     event_declaration=EventDeclaration(name="payment.processed"),
        ...     step_context=step_event_context,
        ... )
        >>> publisher.publish(ctx)
    """

    event_name: str = Field(description="Event name.")
    step_result: StepResult = Field(description="Step result.")
    event_declaration: EventDeclaration | None = Field(
        default=None,
        description="Event declaration from YAML.",
    )
    step_context: StepEventContext | None = Field(
        default=None,
        description="Step execution context.",
    )

    model_config = {"arbitrary_types_allowed": True}


# =============================================================================
# Step Event Context and Base Classes
# =============================================================================


class StepEventContext(BaseModel):
    """Context for publishing step events.

    This model provides all the information needed to publish an event
    after a step completes. It matches the cross-language StepEventContext
    structure used in Ruby and Rust workers.

    Example:
        >>> ctx = StepEventContext(
        ...     task_uuid="abc-123",
        ...     step_uuid="def-456",
        ...     step_name="process_payment",
        ...     namespace="payments",
        ...     correlation_id="corr-789",
        ...     result={"amount": 100.00},
        ... )
    """

    task_uuid: str = Field(description="The task UUID.")
    step_uuid: str = Field(description="The step UUID.")
    step_name: str = Field(description="The step handler name.")
    namespace: str = Field(description="The task namespace.")
    correlation_id: str = Field(description="Correlation ID for tracing.")
    result: dict[str, Any] | None = Field(
        default=None,
        description="The step result data (on success).",
    )
    metadata: dict[str, Any] = Field(
        default_factory=dict,
        description="Additional metadata about the execution.",
    )

    @classmethod
    def from_step_context(
        cls,
        step_context: Any,
        result: dict[str, Any] | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> StepEventContext:
        """Create from a StepContext.

        Args:
            step_context: The step execution context.
            result: Optional result data.
            metadata: Optional additional metadata.

        Returns:
            A StepEventContext populated from the step context.
        """
        return cls(
            task_uuid=str(step_context.task_uuid),
            step_uuid=str(step_context.step_uuid),
            step_name=step_context.handler_name,
            namespace=step_context.event.task_sequence_step.get("task", {})
            .get("task", {})
            .get("namespace", "default"),
            correlation_id=str(step_context.correlation_id),
            result=result,
            metadata=metadata or {},
        )


class BasePublisher(ABC):
    """Abstract base class for step event publishers with lifecycle hooks.

    Publishers are responsible for emitting domain events after step completion.
    They can publish to external systems like Kafka, webhooks, or message queues.

    The publishing flow follows lifecycle hooks:
    1. should_publish() - Determine if event should be published
    2. transform_payload() - Transform the event payload
    3. additional_metadata() - Add custom metadata
    4. before_publish() - Pre-publish hook (can abort)
    5. [actual publish via Rust orchestration]
    6. after_publish() - Post-publish hook
    7. on_publish_error() - Error handling hook (if publish fails)

    Subclasses must implement:
    - publisher_name: Property returning the publisher name

    Optional overrides:
    - should_publish(): Conditionally publish based on context
    - transform_payload(): Transform the event payload
    - additional_metadata(): Add custom metadata
    - before_publish(): Pre-publish validation/logging
    - after_publish(): Post-publish actions
    - on_publish_error(): Error handling

    Example:
        >>> class SlackNotificationPublisher(BasePublisher):
        ...     @property
        ...     def publisher_name(self) -> str:
        ...         return "slack_notification"
        ...
        ...     def transform_payload(self, ctx: StepEventContext) -> dict:
        ...         return {"message": f"Step {ctx.step_name} completed"}
        ...
        ...     def should_publish(self, ctx: StepEventContext) -> bool:
        ...         return ctx.namespace == "production"
        ...
        ...     def before_publish(self, event_name: str, payload: dict, metadata: dict) -> bool:
        ...         log_info(f"Publishing {event_name}")
        ...         return True
    """

    @property
    @abstractmethod
    def publisher_name(self) -> str:
        """Return the publisher name for registry lookup.

        Must match the `publisher:` field in YAML configuration.

        Returns:
            A unique identifier for this publisher.
        """
        ...

    def should_publish(self, ctx: StepEventContext) -> bool:  # noqa: ARG002
        """Determine whether to publish for this context.

        Override this to conditionally publish based on the step name,
        namespace, or other context attributes. The YAML condition is
        evaluated first by Rust orchestration.

        Args:
            ctx: The step event context.

        Returns:
            True if the event should be published (default: True).
        """
        return True

    def transform_payload(self, ctx: StepEventContext) -> dict[str, Any]:
        """Transform the event payload before publishing.

        Override this to customize the payload structure for your
        external system.

        Args:
            ctx: The step event context.

        Returns:
            A dictionary payload for the event.
        """
        return ctx.result or {}

    def additional_metadata(
        self,
        _ctx: StepEventContext,
    ) -> dict[str, Any]:
        """Add custom metadata to the event.

        Override to add custom metadata fields to every published event.

        Args:
            _ctx: The step event context.

        Returns:
            Additional metadata to merge into event metadata.
        """
        return {}

    def before_publish(
        self,
        event_name: str,
        _payload: dict[str, Any],
        _metadata: dict[str, Any],
    ) -> bool:
        """Hook called before publishing.

        Override for pre-publish validation, logging, or metrics.
        Return False to prevent publishing.

        Args:
            event_name: The event name.
            _payload: The transformed payload.
            _metadata: The event metadata.

        Returns:
            True to continue publishing, False to abort.
        """
        log_debug(f"Publishing event: {event_name}")
        return True

    def after_publish(
        self,
        event_name: str,
        _payload: dict[str, Any],
        _metadata: dict[str, Any],
    ) -> None:
        """Hook called after successful publishing.

        Override for post-publish logging, metrics, or cleanup.

        Args:
            event_name: The event name.
            _payload: The transformed payload.
            _metadata: The event metadata.
        """
        log_debug(f"Event published: {event_name}")

    def on_publish_error(
        self,
        event_name: str,
        error: Exception,
        _payload: dict[str, Any],
    ) -> None:
        """Hook called if publishing fails.

        Override for error handling, logging, or fallback behavior.

        Args:
            event_name: The event name.
            error: The error that occurred.
            _payload: The transformed payload.
        """
        log_error(f"Failed to publish event {event_name}: {error}")

    def publish(self, ctx: StepEventContext, event_name: str | None = None) -> bool:
        """Publish an event with unified lifecycle hooks.

        Coordinates the full publish lifecycle: condition check, payload
        transformation, metadata building, and lifecycle hooks.

        Args:
            ctx: The step event context with all event data.
            event_name: Optional event name (defaults to step_name).

        Returns:
            True if event was published, False if skipped or failed.
        """
        event_name = event_name or ctx.step_name

        # Check publishing conditions
        if not self.should_publish(ctx):
            return False

        # Transform payload
        payload = self.transform_payload(ctx)

        # Build metadata
        from datetime import datetime, timezone

        base_metadata: dict[str, Any] = {
            "publisher": self.publisher_name,
            "published_at": datetime.now(timezone.utc).isoformat(),
            "task_uuid": ctx.task_uuid,
            "step_uuid": ctx.step_uuid,
            "step_name": ctx.step_name,
            "namespace": ctx.namespace,
            "correlation_id": ctx.correlation_id,
        }

        metadata = {**base_metadata, **ctx.metadata, **self.additional_metadata(ctx)}

        try:
            # Pre-publish hook (can abort)
            if not self.before_publish(event_name, payload, metadata):
                return False

            # Actual publishing is handled by Rust orchestration
            # This method prepares and validates the event

            # Post-publish hook
            self.after_publish(event_name, payload, metadata)
            return True

        except Exception as e:
            self.on_publish_error(event_name, e, payload)
            return False


class DefaultPublisher(BasePublisher):
    """Default publisher that passes step result through unchanged."""

    @property
    def publisher_name(self) -> str:
        """Return the default publisher name."""
        return "default"

    def transform_payload(self, ctx: StepEventContext) -> dict[str, Any]:
        """Return the step result as-is."""
        return ctx.result or {}


class BaseSubscriber(ABC):
    """Abstract base class for domain event subscribers with lifecycle hooks.

    Subscribers listen for domain events and handle them accordingly.
    They define which event patterns they subscribe to and how to handle
    incoming events.

    The handling flow follows lifecycle hooks:
    1. before_handle() - Pre-handle validation (can skip)
    2. handle() - Main event handling
    3. after_handle() - Post-handle cleanup
    4. on_handle_error() - Error handling (if handle fails)

    Subclasses must implement:
    - subscribes_to(): Return list of event patterns to subscribe to
    - handle(event): Handle the received event

    Optional overrides:
    - before_handle(): Pre-handle validation
    - after_handle(): Post-handle cleanup
    - on_handle_error(): Error handling

    Event patterns can be:
    - Exact names: "order.completed"
    - Wildcard patterns: "order.*" or "*.completed"
    - Catch-all: "*"

    Example:
        >>> class MetricsCollectorSubscriber(BaseSubscriber):
        ...     @classmethod
        ...     def subscribes_to(cls) -> list[str]:
        ...         return ["step.completed", "step.failed"]
        ...
        ...     def handle(self, event: dict) -> None:
        ...         event_name = event.get("event_name")
        ...         if event_name == "step.completed":
        ...             metrics.increment("steps_completed")
        ...
        ...     def before_handle(self, event: dict) -> bool:
        ...         # Skip events without proper metadata
        ...         return "task_uuid" in event
    """

    def __init__(self) -> None:
        """Initialize the subscriber."""
        self._active = False
        self._subscriptions: list[str] = []

    @classmethod
    @abstractmethod
    def subscribes_to(cls) -> list[str]:
        """Return list of event patterns to subscribe to.

        Returns:
            List of event name patterns (e.g., ["order.completed", "payment.*"]).
        """
        ...

    @abstractmethod
    def handle(self, event: dict[str, Any]) -> None:
        """Handle the received event.

        Subclasses MUST implement this method.

        Args:
            event: The event data dictionary containing event_name, payload, etc.
        """
        ...

    @property
    def active(self) -> bool:
        """Check if the subscriber is active.

        Returns:
            True if the subscriber is actively listening for events.
        """
        return self._active

    @property
    def subscriptions(self) -> list[str]:
        """Get subscribed patterns.

        Returns:
            List of event patterns this subscriber is listening to.
        """
        return self._subscriptions.copy()

    def before_handle(self, event: dict[str, Any]) -> bool:  # noqa: ARG002
        """Hook called before handling an event.

        Override for pre-processing, validation, or filtering.
        Return False to skip handling this event.

        Args:
            event: The event data dictionary.

        Returns:
            True to continue handling, False to skip.
        """
        return True

    def after_handle(self, _event: dict[str, Any]) -> None:  # noqa: B027
        """Hook called after successful handling.

        Override for post-processing, metrics, or cleanup.

        Args:
            _event: The event data dictionary.
        """
        pass

    def on_handle_error(self, event: dict[str, Any], error: Exception) -> None:
        """Hook called if handling fails.

        Override for custom error handling, alerts, or retry logic.
        Note: Domain events use fire-and-forget semantics.

        Args:
            event: The event data dictionary.
            error: The error that occurred.
        """
        event_name = event.get("event_name", "unknown")
        log_error(f"{self.__class__.__name__}: Failed to handle event {event_name}: {error}")

    def matches(self, event_name: str) -> bool:
        """Check if this subscriber should handle the given event name.

        Supports wildcard matching with "*":
        - "*" matches everything
        - "payment.*" matches "payment.processed", "payment.failed"
        - "order.created" matches exactly "order.created"

        Args:
            event_name: The event name to check.

        Returns:
            True if this subscriber handles the event.
        """
        import fnmatch

        return any(fnmatch.fnmatch(event_name, pattern) for pattern in self.subscribes_to())

    def start(self, poller: InProcessDomainEventPoller) -> None:
        """Start listening for events.

        Registers with the InProcessDomainEventPoller.

        Args:
            poller: The event poller to register with.
        """
        if self._active:
            return

        self._active = True
        patterns = self.subscribes_to()

        for pattern in patterns:
            # Create a closure with explicit type annotation
            def make_handler(p: str) -> DomainEventCallback:
                def handler(e: InProcessDomainEvent) -> None:
                    self._handle_if_matches(e, p)

                return handler

            poller.on_event(make_handler(pattern))
            self._subscriptions.append(pattern)
            log_info(f"Subscriber {self.__class__.__name__} subscribed to pattern: {pattern}")

        log_info(
            f"Subscriber {self.__class__.__name__} started",
            {"subscription_count": str(len(self._subscriptions))},
        )

    def stop(self) -> None:
        """Stop listening for events."""
        if not self._active:
            return

        self._active = False
        self._subscriptions = []
        log_info(f"Subscriber {self.__class__.__name__} stopped")

    def _handle_if_matches(self, event: InProcessDomainEvent, pattern: str) -> None:
        """Handle event if it matches the pattern."""
        import fnmatch

        if not fnmatch.fnmatch(event.event_name, pattern):
            return

        self.handle_event_safely(event.model_dump())

    def handle_event_safely(self, event: dict[str, Any]) -> None:
        """Safely handle an event with error capture.

        Coordinates the lifecycle hooks for event handling.

        Args:
            event: The event data dictionary.
        """
        if not self._active:
            return

        try:
            if not self.before_handle(event):
                return

            self.handle(event)
            self.after_handle(event)

        except Exception as e:
            self.on_handle_error(event, e)


# Type aliases for callbacks
DomainEventCallback = Callable[[InProcessDomainEvent], None]
ErrorCallback = Callable[[Exception], None]


class InProcessDomainEventPoller:
    """Threaded poller for in-process domain events.

    The InProcessDomainEventPoller runs in a separate thread and polls for
    domain events from the Rust broadcast channel. Events are emitted to
    registered handlers via callbacks.

    This is the "fast path" for domain events - suitable for real-time
    notifications that don't require guaranteed delivery.

    Threading Model:
        - Main Thread: Python application, event handlers
        - Polling Thread: Dedicated background thread for event polling
        - Rust Threads: Rust worker runtime (separate from Python)

    Performance Characteristics:
        - Poll Interval: 10ms (configurable)
        - Max Latency: ~10ms from event generation to handler execution
        - CPU Usage: Minimal (yields during sleep)
        - Delivery: Best-effort (may miss events if lagged)

    Example:
        >>> poller = InProcessDomainEventPoller(polling_interval_ms=10)
        >>> poller.on_event(handle_domain_event)
        >>> poller.on_error(handle_error)
        >>> poller.start()
        >>>
        >>> # Events flow to handle_domain_event callback
        >>>
        >>> poller.stop()

    Attributes:
        polling_interval_ms: Milliseconds between polls when no events.
        max_events_per_poll: Maximum events to process per poll iteration.
    """

    # Default polling interval in milliseconds
    DEFAULT_POLL_INTERVAL_MS = 10

    # Maximum events to process in one poll iteration
    # Prevents tight loop if many events are pending
    DEFAULT_MAX_EVENTS_PER_POLL = 100

    def __init__(
        self,
        polling_interval_ms: int = DEFAULT_POLL_INTERVAL_MS,
        max_events_per_poll: int = DEFAULT_MAX_EVENTS_PER_POLL,
    ) -> None:
        """Initialize the InProcessDomainEventPoller.

        Args:
            polling_interval_ms: Milliseconds between polls when no events.
            max_events_per_poll: Maximum events to process per poll iteration.
        """
        self._polling_interval = polling_interval_ms / 1000.0
        self._max_events_per_poll = max_events_per_poll
        self._running = False
        self._thread: threading.Thread | None = None
        self._poll_count = 0
        self._events_processed = 0
        self._events_lagged = 0

        # Callbacks
        self._event_callbacks: list[DomainEventCallback] = []
        self._error_callbacks: list[ErrorCallback] = []

    def on_event(self, callback: DomainEventCallback) -> None:
        """Register a callback for domain events.

        The callback will be invoked on the polling thread when a domain
        event is received from the in-process broadcast channel.

        Args:
            callback: Function that accepts an InProcessDomainEvent.

        Example:
            >>> def handle_event(event: InProcessDomainEvent):
            ...     if event.event_name == "step.completed":
            ...         print(f"Step {event.metadata.step_uuid} completed")
            >>> poller.on_event(handle_event)
        """
        self._event_callbacks.append(callback)

    def on_error(self, callback: ErrorCallback) -> None:
        """Register a callback for errors.

        The callback is invoked when an error occurs during polling.

        Args:
            callback: Function that accepts an Exception.

        Example:
            >>> def handle_error(error: Exception):
            ...     log_error(f"Domain event poller error: {error}")
            >>> poller.on_error(handle_error)
        """
        self._error_callbacks.append(callback)

    def start(self) -> None:
        """Start the domain event polling thread.

        Creates a daemon thread that continuously polls for events.
        The thread will stop when stop() is called or the main program exits.

        Raises:
            RuntimeError: If the poller is already running.

        Example:
            >>> poller.start()
            >>> assert poller.is_running
        """
        if self._running:
            raise RuntimeError("InProcessDomainEventPoller already running")

        self._running = True
        self._thread = threading.Thread(
            target=self._poll_loop,
            name="tasker-domain-event-poller",
            daemon=True,
        )
        self._thread.start()

        log_info(
            "InProcessDomainEventPoller started",
            {"interval_ms": str(int(self._polling_interval * 1000))},
        )

    def stop(self, timeout: float = 5.0) -> None:
        """Stop the domain event polling thread.

        Signals the polling thread to stop and waits for it to finish.
        Safe to call even if the poller is not running.

        Args:
            timeout: Maximum seconds to wait for thread to stop.

        Example:
            >>> poller.stop()
            >>> assert not poller.is_running
        """
        if not self._running:
            return

        log_info("Stopping InProcessDomainEventPoller...")
        self._running = False

        if self._thread is not None:
            self._thread.join(timeout=timeout)
            self._thread = None

        log_info(
            "InProcessDomainEventPoller stopped",
            {
                "events_processed": str(self._events_processed),
                "events_lagged": str(self._events_lagged),
            },
        )

    @property
    def is_running(self) -> bool:
        """Check if the poller is currently running.

        Returns:
            True if the polling thread is active.
        """
        return self._running and self._thread is not None and self._thread.is_alive()

    @property
    def stats(self) -> dict[str, int]:
        """Get polling statistics.

        Returns:
            Dictionary with poll_count, events_processed, events_lagged.
        """
        return {
            "poll_count": self._poll_count,
            "events_processed": self._events_processed,
            "events_lagged": self._events_lagged,
        }

    def _poll_loop(self) -> None:
        """Main polling loop (runs in separate thread)."""
        log_debug("InProcessDomainEventPoller: Starting poll loop")

        while self._running:
            try:
                self._poll_count += 1
                events_this_poll = 0

                # Poll for events (up to max_events_per_poll)
                while events_this_poll < self._max_events_per_poll:
                    event_data = _poll_in_process_events()

                    if event_data is None:
                        # No more events available
                        break

                    # Check if we got a lag warning (indicated by special payload)
                    # The Rust FFI returns None for lagged events
                    self._process_event(event_data)
                    events_this_poll += 1
                    self._events_processed += 1

                # Sleep if no events were found
                if events_this_poll == 0:
                    time.sleep(self._polling_interval)

            except Exception as e:
                self._emit_error(e)
                # Sleep longer on error to avoid tight error loops
                time.sleep(self._polling_interval * 10)

        log_debug("InProcessDomainEventPoller: Poll loop terminated")

    def _process_event(self, event_data: dict[str, Any]) -> None:
        """Process a polled event through callbacks."""
        try:
            event = InProcessDomainEvent.model_validate(event_data)
            log_debug(
                "InProcessDomainEventPoller: Processing event",
                {
                    "event_id": str(event.event_id),
                    "event_name": event.event_name,
                },
            )

            for callback in self._event_callbacks:
                try:
                    callback(event)
                except Exception as e:
                    log_error(
                        f"InProcessDomainEventPoller: Callback error: {e}",
                        {"event_id": str(event.event_id)},
                    )
                    self._emit_error(e)

        except Exception as e:
            log_error(f"InProcessDomainEventPoller: Failed to process event: {e}")
            self._emit_error(e)

    def _emit_error(self, error: Exception) -> None:
        """Emit an error to registered callbacks."""
        import contextlib

        for callback in self._error_callbacks:
            # Use contextlib.suppress to ignore errors in error callbacks
            with contextlib.suppress(Exception):
                callback(error)


# =============================================================================
# Publisher and Subscriber Registries
# =============================================================================


class PublisherRegistry:
    """Singleton registry for custom publishers.

    Provides centralized management for step event publishers. Publishers can be
    registered by name and retrieved during step execution for event publishing.

    The registry supports:
    - Registration by name or auto-registration using publisher_name property
    - Default publisher fallback
    - Freeze capability to prevent late registration in production
    - Required publisher validation

    Thread Safety:
        The registry is thread-safe for read operations after freeze.
        Registration should happen during application startup.

    Example:
        >>> # Get singleton instance
        >>> registry = PublisherRegistry.instance()
        >>>
        >>> # Register a custom publisher
        >>> registry.register(SlackNotificationPublisher())
        >>>
        >>> # Get publisher (falls back to default)
        >>> publisher = registry.get_or_default("slack_notification")
        >>>
        >>> # Freeze in production
        >>> registry.freeze()
    """

    _instance: PublisherRegistry | None = None
    _lock = threading.Lock()

    def __init__(self) -> None:
        """Initialize the publisher registry."""
        self._publishers: dict[str, BasePublisher] = {}
        self._frozen = False
        self._default_publisher = DefaultPublisher()

    @classmethod
    def instance(cls) -> PublisherRegistry:
        """Get the singleton instance.

        Returns:
            The singleton PublisherRegistry instance.
        """
        if cls._instance is None:
            with cls._lock:
                if cls._instance is None:
                    cls._instance = cls()
        return cls._instance

    @classmethod
    def reset(cls) -> None:
        """Reset the singleton (for testing).

        Clears the singleton instance, allowing a fresh registry on next access.
        """
        with cls._lock:
            cls._instance = None

    def register(self, publisher: BasePublisher, name: str | None = None) -> None:
        """Register a publisher.

        Args:
            publisher: The publisher instance to register.
            name: Optional name override. Defaults to publisher.publisher_name.

        Raises:
            RuntimeError: If registry is frozen.
            ValueError: If publisher is not a BasePublisher instance.
        """
        if self._frozen:
            raise RuntimeError("PublisherRegistry is frozen, cannot register new publishers")

        if not isinstance(publisher, BasePublisher):
            raise ValueError(f"Expected BasePublisher instance, got {type(publisher)}")

        publisher_name = name or publisher.publisher_name
        if publisher_name in self._publishers:
            log_warn(f"Overwriting existing publisher: {publisher_name}")

        self._publishers[publisher_name] = publisher
        log_info(f"Registered publisher: {publisher_name}")

    def get(self, name: str) -> BasePublisher | None:
        """Get a publisher by name.

        Args:
            name: The publisher name.

        Returns:
            The publisher instance or None if not found.
        """
        return self._publishers.get(name)

    def get_or_default(self, name: str) -> BasePublisher:
        """Get a publisher by name, falling back to default.

        Args:
            name: The publisher name.

        Returns:
            The publisher instance or DefaultPublisher if not found.
        """
        return self._publishers.get(name, self._default_publisher)

    def validate_required(self, names: list[str]) -> list[str]:
        """Validate that required publishers are registered.

        Args:
            names: List of required publisher names.

        Returns:
            List of missing publisher names (empty if all present).
        """
        return [name for name in names if name not in self._publishers]

    def freeze(self) -> None:
        """Freeze the registry to prevent further registration.

        Call this after application startup to catch late registrations.
        """
        self._frozen = True
        log_info(f"PublisherRegistry frozen with {len(self._publishers)} publishers")

    def unfreeze(self) -> None:
        """Unfreeze the registry (for testing)."""
        self._frozen = False

    @property
    def is_frozen(self) -> bool:
        """Check if the registry is frozen.

        Returns:
            True if the registry is frozen.
        """
        return self._frozen

    def list_publishers(self) -> list[str]:
        """List all registered publisher names.

        Returns:
            List of registered publisher names.
        """
        return list(self._publishers.keys())

    def clear(self) -> None:
        """Clear all registered publishers (for testing).

        Raises:
            RuntimeError: If registry is frozen.
        """
        if self._frozen:
            raise RuntimeError("PublisherRegistry is frozen, cannot clear")
        self._publishers.clear()
        log_debug("Cleared all publishers from registry")


class SubscriberRegistry:
    """Singleton registry for domain event subscribers.

    Provides centralized management for event subscribers. Subscribers can be
    registered by class or instance, and started/stopped as a group.

    The registry supports:
    - Class-based registration (instantiated on start)
    - Instance-based registration (for pre-configured subscribers)
    - Bulk start/stop operations
    - Statistics gathering

    Thread Safety:
        Registration should happen during application startup.
        Start/stop operations are not thread-safe.

    Example:
        >>> # Get singleton instance
        >>> registry = SubscriberRegistry.instance()
        >>>
        >>> # Register subscriber class
        >>> registry.register(MetricsCollectorSubscriber)
        >>>
        >>> # Register pre-configured instance
        >>> registry.register_instance(SlackNotifier(channel="#alerts"))
        >>>
        >>> # Start all subscribers with a poller
        >>> poller = InProcessDomainEventPoller()
        >>> registry.start_all(poller)
        >>>
        >>> # Later: stop all subscribers
        >>> registry.stop_all()
    """

    _instance: SubscriberRegistry | None = None
    _lock = threading.Lock()

    def __init__(self) -> None:
        """Initialize the subscriber registry."""
        self._subscriber_classes: dict[str, type[BaseSubscriber]] = {}
        self._subscriber_instances: dict[str, BaseSubscriber] = {}
        self._active_subscribers: list[BaseSubscriber] = []

    @classmethod
    def instance(cls) -> SubscriberRegistry:
        """Get the singleton instance.

        Returns:
            The singleton SubscriberRegistry instance.
        """
        if cls._instance is None:
            with cls._lock:
                if cls._instance is None:
                    cls._instance = cls()
        return cls._instance

    @classmethod
    def reset(cls) -> None:
        """Reset the singleton (for testing).

        Stops all active subscribers and clears the instance.
        """
        with cls._lock:
            if cls._instance is not None:
                cls._instance.stop_all()
            cls._instance = None

    def register(self, subscriber_class: type[BaseSubscriber]) -> None:
        """Register a subscriber class.

        The class will be instantiated when start_all() is called.

        Args:
            subscriber_class: The subscriber class to register.

        Raises:
            ValueError: If not a BaseSubscriber subclass.
        """
        if not isinstance(subscriber_class, type) or not issubclass(
            subscriber_class, BaseSubscriber
        ):
            raise ValueError(f"Expected BaseSubscriber subclass, got {subscriber_class}")

        name = subscriber_class.__name__
        if name in self._subscriber_classes:
            log_warn(f"Overwriting existing subscriber class: {name}")

        self._subscriber_classes[name] = subscriber_class
        log_info(f"Registered subscriber class: {name}")

    def register_instance(self, subscriber: BaseSubscriber, name: str | None = None) -> None:
        """Register a pre-configured subscriber instance.

        Use this for subscribers that need specific configuration.

        Args:
            subscriber: The subscriber instance to register.
            name: Optional name override. Defaults to class name.

        Raises:
            ValueError: If not a BaseSubscriber instance.
        """
        if not isinstance(subscriber, BaseSubscriber):
            raise ValueError(f"Expected BaseSubscriber instance, got {type(subscriber)}")

        instance_name = name or subscriber.__class__.__name__
        if instance_name in self._subscriber_instances:
            log_warn(f"Overwriting existing subscriber instance: {instance_name}")

        self._subscriber_instances[instance_name] = subscriber
        log_info(f"Registered subscriber instance: {instance_name}")

    def start_all(self, poller: InProcessDomainEventPoller) -> int:
        """Start all registered subscribers.

        Instantiates class-based subscribers and starts all subscribers
        with the provided poller.

        Args:
            poller: The event poller to use.

        Returns:
            Number of subscribers started.
        """
        started = 0

        # Instantiate and start class-based subscribers
        for name, subscriber_class in self._subscriber_classes.items():
            try:
                instance = subscriber_class()
                instance.start(poller)
                self._active_subscribers.append(instance)
                started += 1
                log_debug(f"Started subscriber from class: {name}")
            except Exception as e:
                log_error(f"Failed to start subscriber {name}: {e}")

        # Start pre-configured instance subscribers
        for name, instance in self._subscriber_instances.items():
            try:
                instance.start(poller)
                self._active_subscribers.append(instance)
                started += 1
                log_debug(f"Started subscriber instance: {name}")
            except Exception as e:
                log_error(f"Failed to start subscriber {name}: {e}")

        log_info(f"Started {started} subscribers")
        return started

    def stop_all(self) -> int:
        """Stop all active subscribers.

        Returns:
            Number of subscribers stopped.
        """
        stopped = 0

        for subscriber in self._active_subscribers:
            try:
                subscriber.stop()
                stopped += 1
            except Exception as e:
                log_error(f"Failed to stop subscriber: {e}")

        self._active_subscribers.clear()
        log_info(f"Stopped {stopped} subscribers")
        return stopped

    def list_registered(self) -> dict[str, list[str]]:
        """List all registered subscribers.

        Returns:
            Dictionary with 'classes' and 'instances' lists.
        """
        return {
            "classes": list(self._subscriber_classes.keys()),
            "instances": list(self._subscriber_instances.keys()),
        }

    @property
    def active_count(self) -> int:
        """Get count of active subscribers.

        Returns:
            Number of currently active subscribers.
        """
        return len(self._active_subscribers)

    def stats(self) -> dict[str, Any]:
        """Get registry statistics.

        Returns:
            Dictionary with registration and activity counts.
        """
        return {
            "registered_classes": len(self._subscriber_classes),
            "registered_instances": len(self._subscriber_instances),
            "active_subscribers": len(self._active_subscribers),
            "subscriptions": [
                {
                    "class": sub.__class__.__name__,
                    "patterns": sub.subscriptions,
                    "active": sub.active,
                }
                for sub in self._active_subscribers
            ],
        }

    def clear(self) -> None:
        """Clear all registrations (for testing).

        Stops active subscribers first.
        """
        self.stop_all()
        self._subscriber_classes.clear()
        self._subscriber_instances.clear()
        log_debug("Cleared all subscribers from registry")


__all__ = [
    # Cross-language type definitions (TAS-112)
    "EventDeclaration",
    "StepResult",
    "PublishContext",
    # Step event context
    "StepEventContext",
    # Publisher system
    "BasePublisher",
    "DefaultPublisher",
    "PublisherRegistry",
    # Subscriber system
    "BaseSubscriber",
    "SubscriberRegistry",
    # Event polling
    "InProcessDomainEventPoller",
]
