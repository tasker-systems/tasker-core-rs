"""In-process domain event poller for real-time notifications.

This module provides the InProcessDomainEventPoller class that polls for
domain events from the in-process broadcast channel (fast path).

In-process events are used for real-time notifications that don't require
guaranteed delivery, such as:
- Slack notifications
- Real-time metrics updates
- UI refresh signals
- Log aggregation

For durable domain events (guaranteed delivery), the Rust orchestration
layer publishes events to PGMQ automatically after step completion.

Example:
    >>> from tasker_core import InProcessDomainEventPoller
    >>>
    >>> def on_event(event):
    ...     print(f"Received {event.event_name}: {event.payload}")
    ...
    >>> poller = InProcessDomainEventPoller()
    >>> poller.on_event(on_event)
    >>> poller.start()
    >>> # ... events flow to handlers ...
    >>> poller.stop()
"""

from __future__ import annotations

import threading
import time
from collections.abc import Callable
from typing import Any

from tasker_core._tasker_core import (  # type: ignore[attr-defined]
    poll_in_process_events as _poll_in_process_events,
)

from .logging import log_debug, log_error, log_info
from .types import InProcessDomainEvent

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
        return (
            self._running
            and self._thread is not None
            and self._thread.is_alive()
        )

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


__all__ = ["InProcessDomainEventPoller"]
