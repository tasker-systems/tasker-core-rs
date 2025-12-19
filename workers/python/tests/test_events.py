"""Event system tests.

These tests verify:
- EventBridge pub/sub functionality
- EventNames constants
- EventPoller class for FFI event polling
- InProcessDomainEventPoller for domain events
"""

from __future__ import annotations

import pytest

from tasker_core import EventBridge, EventNames, EventPoller, InProcessDomainEventPoller


class TestEventBridge:
    """Tests for EventBridge pub/sub functionality."""

    def setup_method(self):
        """Reset singleton before each test."""
        EventBridge.reset_instance()

    def teardown_method(self):
        """Clean up after each test."""
        EventBridge.reset_instance()

    def test_singleton_instance(self):
        """Test EventBridge singleton pattern."""
        bridge1 = EventBridge.instance()
        bridge2 = EventBridge.instance()
        assert bridge1 is bridge2

    def test_reset_instance(self):
        """Test singleton reset creates new instance."""
        bridge1 = EventBridge.instance()
        EventBridge.reset_instance()
        bridge2 = EventBridge.instance()
        assert bridge1 is not bridge2

    def test_start_stop(self):
        """Test start/stop lifecycle."""
        bridge = EventBridge.instance()
        assert not bridge.is_active

        bridge.start()
        assert bridge.is_active

        bridge.stop()
        assert not bridge.is_active

    def test_start_idempotent(self):
        """Test start() is idempotent."""
        bridge = EventBridge.instance()
        bridge.start()
        bridge.start()  # Should not raise
        assert bridge.is_active

    def test_stop_idempotent(self):
        """Test stop() is idempotent."""
        bridge = EventBridge.instance()
        bridge.stop()  # Not started, should not raise
        assert not bridge.is_active

    def test_subscribe_publish(self):
        """Test basic subscribe and publish."""
        bridge = EventBridge.instance()
        bridge.start()

        received = []
        bridge.subscribe("test.event", lambda x: received.append(x))
        bridge.publish("test.event", "data")

        assert received == ["data"]

    def test_subscribe_publish_multiple_args(self):
        """Test publish with multiple arguments."""
        bridge = EventBridge.instance()
        bridge.start()

        received = []
        bridge.subscribe("test.event", lambda x, y: received.append((x, y)))
        bridge.publish("test.event", "a", "b")

        assert received == [("a", "b")]

    def test_subscribe_publish_kwargs(self):
        """Test publish with keyword arguments."""
        bridge = EventBridge.instance()
        bridge.start()

        received = []
        bridge.subscribe("test.event", lambda **kw: received.append(kw))
        bridge.publish("test.event", key="value")

        assert received == [{"key": "value"}]

    def test_multiple_subscribers(self):
        """Test multiple subscribers for same event."""
        bridge = EventBridge.instance()
        bridge.start()

        received1 = []
        received2 = []
        bridge.subscribe("test.event", lambda x: received1.append(x))
        bridge.subscribe("test.event", lambda x: received2.append(x))
        bridge.publish("test.event", "data")

        assert received1 == ["data"]
        assert received2 == ["data"]

    def test_unsubscribe(self):
        """Test unsubscribing from event."""
        bridge = EventBridge.instance()
        bridge.start()

        received = []

        def handler(x):
            received.append(x)

        bridge.subscribe("test.event", handler)
        bridge.publish("test.event", "first")

        bridge.unsubscribe("test.event", handler)
        bridge.publish("test.event", "second")

        assert received == ["first"]

    def test_subscribe_once(self):
        """Test subscribe_once for single invocation."""
        bridge = EventBridge.instance()
        bridge.start()

        received = []
        bridge.subscribe_once("test.event", lambda x: received.append(x))
        bridge.publish("test.event", "first")
        bridge.publish("test.event", "second")

        assert received == ["first"]

    def test_publish_when_inactive_drops_event(self):
        """Test that events are dropped when bridge is inactive."""
        bridge = EventBridge.instance()
        # Don't start the bridge

        received = []
        bridge.subscribe("test.event", lambda x: received.append(x))
        bridge.publish("test.event", "data")

        assert received == []

    def test_listener_count(self):
        """Test listener_count method."""
        bridge = EventBridge.instance()
        assert bridge.listener_count("test.event") == 0

        bridge.subscribe("test.event", lambda: None)
        assert bridge.listener_count("test.event") == 1

        bridge.subscribe("test.event", lambda: None)
        assert bridge.listener_count("test.event") == 2

    def test_listeners(self):
        """Test listeners method."""
        bridge = EventBridge.instance()

        def handler1():
            pass

        def handler2():
            pass

        bridge.subscribe("test.event", handler1)
        bridge.subscribe("test.event", handler2)

        listeners = bridge.listeners("test.event")
        assert len(listeners) == 2

    def test_event_schema(self):
        """Test event_schema property."""
        bridge = EventBridge.instance()
        schema = bridge.event_schema

        assert EventNames.STEP_EXECUTION_RECEIVED in schema
        assert EventNames.STEP_COMPLETION_SENT in schema
        assert EventNames.HANDLER_ERROR in schema

    def test_stop_removes_all_listeners(self):
        """Test stop() removes all listeners."""
        bridge = EventBridge.instance()
        bridge.start()
        bridge.subscribe("test.event", lambda: None)
        assert bridge.listener_count("test.event") == 1

        bridge.stop()
        assert bridge.listener_count("test.event") == 0


class TestEventNames:
    """Tests for EventNames constants."""

    def test_event_names_exist(self):
        """Test all expected event names exist."""
        assert EventNames.STEP_EXECUTION_RECEIVED == "step.execution.received"
        assert EventNames.STEP_COMPLETION_SENT == "step.completion.sent"
        assert EventNames.HANDLER_REGISTERED == "handler.registered"
        assert EventNames.HANDLER_ERROR == "handler.error"
        assert EventNames.POLLER_METRICS == "poller.metrics"
        assert EventNames.POLLER_ERROR == "poller.error"


class TestEventPoller:
    """Test EventPoller class functionality."""

    def test_event_poller_init(self):
        """Test EventPoller initialization."""
        poller = EventPoller(
            polling_interval_ms=20,
            starvation_check_interval=50,
            cleanup_interval=100,
        )
        assert poller._polling_interval == 0.020
        assert poller._starvation_check_interval == 50
        assert poller._cleanup_interval == 100
        assert not poller.is_running

    def test_event_poller_default_values(self):
        """Test EventPoller default initialization."""
        poller = EventPoller()
        assert poller._polling_interval == 0.010  # 10ms
        assert poller._starvation_check_interval == 100
        assert poller._cleanup_interval == 1000

    def test_event_poller_callback_registration(self):
        """Test registering callbacks with EventPoller."""
        poller = EventPoller()
        events_received = []
        errors_received = []

        poller.on_step_event(lambda e: events_received.append(e))
        poller.on_error(lambda e: errors_received.append(e))

        assert len(poller._step_event_callbacks) == 1
        assert len(poller._error_callbacks) == 1

    def test_event_poller_multiple_callbacks(self):
        """Test registering multiple callbacks."""
        poller = EventPoller()
        callback1_called = []
        callback2_called = []

        poller.on_step_event(lambda e: callback1_called.append(e))
        poller.on_step_event(lambda e: callback2_called.append(e))

        assert len(poller._step_event_callbacks) == 2

    def test_event_poller_start_raises_if_already_running(self):
        """Test that starting twice raises RuntimeError."""
        poller = EventPoller()
        # We can't actually start without a running worker,
        # but we can test the flag logic
        poller._running = True

        with pytest.raises(RuntimeError, match="already running"):
            poller.start()

        # Reset for cleanup
        poller._running = False

    def test_event_poller_stop_when_not_running(self):
        """Test stop() is safe when poller not running."""
        poller = EventPoller()
        # Should not raise
        poller.stop()
        assert not poller.is_running

    def test_event_poller_is_running_property(self):
        """Test is_running property."""
        poller = EventPoller()
        assert poller.is_running is False

        # Simulate running state
        poller._running = True
        # Still False because no thread
        assert poller.is_running is False

        poller._running = False

    def test_event_poller_get_metrics_without_worker(self):
        """Test get_metrics returns None without worker running."""
        poller = EventPoller()
        metrics = poller.get_metrics()
        assert metrics is None


class TestInProcessDomainEventPoller:
    """Test InProcessDomainEventPoller class."""

    def test_poller_initialization(self):
        """Test poller can be initialized."""
        poller = InProcessDomainEventPoller()
        assert not poller.is_running

    def test_poller_with_custom_interval(self):
        """Test poller with custom polling interval."""
        poller = InProcessDomainEventPoller(
            polling_interval_ms=50,
            max_events_per_poll=50,
        )
        assert poller._polling_interval == 0.05
        assert poller._max_events_per_poll == 50

    def test_poller_callback_registration(self):
        """Test callback registration."""
        poller = InProcessDomainEventPoller()

        events_received = []

        def callback(event):
            events_received.append(event)

        def error_callback(error):
            pass

        poller.on_event(callback)
        poller.on_error(error_callback)

        assert len(poller._event_callbacks) == 1
        assert len(poller._error_callbacks) == 1

    def test_poller_stats(self):
        """Test poller statistics."""
        poller = InProcessDomainEventPoller()
        stats = poller.stats

        assert "poll_count" in stats
        assert "events_processed" in stats
        assert "events_lagged" in stats
        assert stats["poll_count"] == 0
        assert stats["events_processed"] == 0

    def test_poller_double_start_raises(self):
        """Test that starting twice raises error."""
        poller = InProcessDomainEventPoller()

        # Can't actually start without a worker
        # But we can test the logic
        poller._running = True
        with pytest.raises(RuntimeError, match="already running"):
            poller.start()

        poller._running = False
