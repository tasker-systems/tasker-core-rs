"""Threaded event poller for step execution events.

This module provides the EventPoller class that polls for step events
from the FFI dispatch channel and emits them to registered handlers.

Example:
    >>> from tasker_core import EventPoller
    >>>
    >>> def on_step_event(event):
    ...     print(f"Processing step {event.step_uuid}")
    ...
    >>> poller = EventPoller()
    >>> poller.on_step_event(on_step_event)
    >>> poller.start()
    >>> # ... events flow to handlers ...
    >>> poller.stop()
"""

from __future__ import annotations

import threading
import time
from collections.abc import Callable
from typing import TYPE_CHECKING, Any

from tasker_core._tasker_core import (
    check_starvation_warnings as _check_starvation_warnings,
)
from tasker_core._tasker_core import (
    cleanup_timeouts as _cleanup_timeouts,
)
from tasker_core._tasker_core import (
    get_ffi_dispatch_metrics as _get_ffi_dispatch_metrics,
)
from tasker_core._tasker_core import (
    poll_step_events as _poll_step_events,
)

from .logging import log_debug, log_error, log_info, log_warn
from .types import FfiDispatchMetrics, FfiStepEvent

if TYPE_CHECKING:
    pass


# Type aliases for callbacks
StepEventCallback = Callable[[FfiStepEvent], None]
MetricsCallback = Callable[[FfiDispatchMetrics], None]
ErrorCallback = Callable[[Exception], None]


class EventPoller:
    """Threaded event poller for step execution events.

    The EventPoller runs in a separate thread and polls for step events
    from the FFI dispatch channel. Events are emitted to registered
    handlers via callbacks.

    This matches the Ruby EventPoller pattern with a dedicated polling
    thread that yields control between polls.

    Threading Model:
        - Main Thread: Python application, handler execution
        - Polling Thread: Dedicated background thread for event polling
        - Rust Threads: Rust worker runtime (separate from Python)

    Why Polling is Necessary:
        Python's GIL prevents Rust from directly calling Python methods
        from Rust threads. Polling allows Python to control thread context
        and safely process events.

    Performance Characteristics:
        - Poll Interval: 10ms (configurable)
        - Max Latency: ~10ms from event generation to processing start
        - CPU Usage: Minimal (yields during sleep)
        - Error Recovery: Automatic with exponential backoff

    Example:
        >>> poller = EventPoller(polling_interval_ms=10)
        >>> poller.on_step_event(handle_step)
        >>> poller.on_error(handle_error)
        >>> poller.start()
        >>>
        >>> # Events flow to handle_step callback
        >>>
        >>> poller.stop()

    Attributes:
        polling_interval_ms: Milliseconds between polls when no events.
        starvation_check_interval: Polls between starvation checks.
        cleanup_interval: Polls between timeout cleanups.
    """

    # Default polling interval in milliseconds
    DEFAULT_POLL_INTERVAL_MS = 10

    # Check for starvation warnings every N poll iterations
    # At 10ms poll interval, 100 iterations = 1 second
    DEFAULT_STARVATION_CHECK_INTERVAL = 100

    # Cleanup timed-out events every N poll iterations
    # At 10ms poll interval, 1000 iterations = 10 seconds
    DEFAULT_CLEANUP_INTERVAL = 1000

    def __init__(
        self,
        polling_interval_ms: int = DEFAULT_POLL_INTERVAL_MS,
        starvation_check_interval: int = DEFAULT_STARVATION_CHECK_INTERVAL,
        cleanup_interval: int = DEFAULT_CLEANUP_INTERVAL,
    ) -> None:
        """Initialize the EventPoller.

        Args:
            polling_interval_ms: Milliseconds between polls when no events.
            starvation_check_interval: Number of polls between starvation checks.
            cleanup_interval: Number of polls between timeout cleanups.
        """
        self._polling_interval = polling_interval_ms / 1000.0
        self._starvation_check_interval = starvation_check_interval
        self._cleanup_interval = cleanup_interval
        self._running = False
        self._thread: threading.Thread | None = None
        self._poll_count = 0

        # Callbacks
        self._step_event_callbacks: list[StepEventCallback] = []
        self._metrics_callbacks: list[MetricsCallback] = []
        self._error_callbacks: list[ErrorCallback] = []

    def on_step_event(self, callback: StepEventCallback) -> None:
        """Register a callback for step events.

        The callback will be invoked on the polling thread when a step
        event is received from the FFI dispatch channel.

        Args:
            callback: Function that accepts an FfiStepEvent.

        Example:
            >>> def handle_step(event: FfiStepEvent):
            ...     print(f"Processing {event.step_uuid}")
            >>> poller.on_step_event(handle_step)
        """
        self._step_event_callbacks.append(callback)

    def on_metrics(self, callback: MetricsCallback) -> None:
        """Register a callback for metrics updates.

        The callback is invoked periodically with FFI dispatch metrics.

        Args:
            callback: Function that accepts FfiDispatchMetrics.

        Example:
            >>> def handle_metrics(metrics: FfiDispatchMetrics):
            ...     if metrics.starvation_detected:
            ...         print(f"Warning: {metrics.starving_event_count} starving")
            >>> poller.on_metrics(handle_metrics)
        """
        self._metrics_callbacks.append(callback)

    def on_error(self, callback: ErrorCallback) -> None:
        """Register a callback for errors.

        The callback is invoked when an error occurs during polling.

        Args:
            callback: Function that accepts an Exception.

        Example:
            >>> def handle_error(error: Exception):
            ...     log_error(f"Poller error: {error}")
            >>> poller.on_error(handle_error)
        """
        self._error_callbacks.append(callback)

    def start(self) -> None:
        """Start the event polling thread.

        Creates a daemon thread that continuously polls for events.
        The thread will stop when stop() is called or the main program exits.

        Raises:
            RuntimeError: If the poller is already running.

        Example:
            >>> poller.start()
            >>> assert poller.is_running
        """
        if self._running:
            raise RuntimeError("EventPoller already running")

        self._running = True
        self._thread = threading.Thread(
            target=self._poll_loop,
            name="tasker-event-poller",
            daemon=True,
        )
        self._thread.start()

        log_info("EventPoller started", {"interval_ms": str(int(self._polling_interval * 1000))})

    def stop(self, timeout: float = 5.0) -> None:
        """Stop the event polling thread.

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

        log_info("Stopping EventPoller...")
        self._running = False

        if self._thread is not None:
            self._thread.join(timeout=timeout)
            self._thread = None

        log_info("EventPoller stopped")

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

    def get_metrics(self) -> FfiDispatchMetrics | None:
        """Get current FFI dispatch metrics.

        This can be called from any thread to check the current state
        of the dispatch channel.

        Returns:
            FfiDispatchMetrics if available, None on error.

        Example:
            >>> metrics = poller.get_metrics()
            >>> if metrics and metrics.starvation_detected:
            ...     print("Starvation detected!")
        """
        try:
            metrics_dict = _get_ffi_dispatch_metrics()
            return FfiDispatchMetrics.model_validate(metrics_dict)
        except Exception as e:
            log_debug(f"Failed to get FFI dispatch metrics: {e}")
            return None

    def _poll_loop(self) -> None:
        """Main polling loop (runs in separate thread)."""
        log_debug("EventPoller: Starting poll loop")

        while self._running:
            try:
                self._poll_count += 1

                # Poll for next event
                event_data = _poll_step_events()

                if event_data is not None:
                    self._process_event(event_data)
                else:
                    # No event available, sleep before next poll
                    time.sleep(self._polling_interval)

                # Periodic starvation check
                if self._poll_count % self._starvation_check_interval == 0:
                    self._check_starvation()

                # Periodic cleanup of timed-out events
                if self._poll_count % self._cleanup_interval == 0:
                    self._cleanup_timeouts()

            except Exception as e:
                self._emit_error(e)
                # Sleep longer on error to avoid tight error loops
                time.sleep(self._polling_interval * 10)

        log_debug("EventPoller: Poll loop terminated")

    def _process_event(self, event_data: dict[str, Any]) -> None:
        """Process a polled event through callbacks."""
        try:
            event = FfiStepEvent.model_validate(event_data)
            log_debug(
                "EventPoller: Processing event",
                {"event_id": event.event_id, "step_uuid": event.step_uuid},
            )

            for callback in self._step_event_callbacks:
                try:
                    callback(event)
                except Exception as e:
                    log_error(
                        f"EventPoller: Callback error: {e}",
                        {"event_id": event.event_id},
                    )
                    self._emit_error(e)

        except Exception as e:
            log_error(f"EventPoller: Failed to process event: {e}")
            self._emit_error(e)

    def _check_starvation(self) -> None:
        """Check for starvation warnings and emit metrics."""
        try:
            # Trigger Rust-side starvation logging
            _check_starvation_warnings()

            # Get and emit metrics
            metrics_dict = _get_ffi_dispatch_metrics()
            metrics = FfiDispatchMetrics.model_validate(metrics_dict)

            for callback in self._metrics_callbacks:
                try:
                    callback(metrics)
                except Exception as e:
                    log_debug(f"EventPoller: Metrics callback error: {e}")

            if metrics.starvation_detected:
                log_warn(
                    "EventPoller: Starvation detected",
                    {
                        "pending_count": str(metrics.pending_count),
                        "starving_count": str(metrics.starving_event_count),
                    },
                )

        except Exception as e:
            # Don't let starvation check errors affect polling
            log_debug(f"EventPoller: Starvation check error (non-fatal): {e}")

    def _cleanup_timeouts(self) -> None:
        """Clean up timed-out pending events."""
        try:
            count = _cleanup_timeouts()
            if count > 0:
                log_warn(
                    "EventPoller: Cleaned up timed-out events",
                    {"count": str(count)},
                )
        except Exception as e:
            log_debug(f"EventPoller: Cleanup error (non-fatal): {e}")

    def _emit_error(self, error: Exception) -> None:
        """Emit an error to registered callbacks."""
        import contextlib

        for callback in self._error_callbacks:
            # Use contextlib.suppress to ignore errors in error callbacks
            # (we don't want error callbacks to cause more errors)
            with contextlib.suppress(Exception):
                callback(error)


__all__ = ["EventPoller"]
