"""Phase 4 (TAS-83) Tests: Event Bridge & Handler System.

Tests for:
- EventBridge pub/sub functionality
- HandlerRegistry registration and resolution
- StepHandler ABC contract
- StepContext and StepHandlerResult types
- StepExecutionSubscriber event routing
"""

from __future__ import annotations

from uuid import uuid4

import pytest

from tasker_core import (
    EventBridge,
    EventNames,
    FfiStepEvent,
    HandlerRegistry,
    StepContext,
    StepExecutionError,
    StepExecutionSubscriber,
    StepHandler,
    StepHandlerResult,
)

# =============================================================================
# EventBridge Tests
# =============================================================================


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


# =============================================================================
# HandlerRegistry Tests
# =============================================================================


class TestHandlerRegistry:
    """Tests for HandlerRegistry."""

    def setup_method(self):
        """Reset singleton before each test."""
        HandlerRegistry.reset_instance()

    def teardown_method(self):
        """Clean up after each test."""
        HandlerRegistry.reset_instance()

    def test_singleton_instance(self):
        """Test HandlerRegistry singleton pattern."""
        reg1 = HandlerRegistry.instance()
        reg2 = HandlerRegistry.instance()
        assert reg1 is reg2

    def test_reset_instance(self):
        """Test singleton reset creates new instance."""
        reg1 = HandlerRegistry.instance()
        HandlerRegistry.reset_instance()
        reg2 = HandlerRegistry.instance()
        assert reg1 is not reg2

    def test_register_and_resolve(self):
        """Test handler registration and resolution."""
        registry = HandlerRegistry()

        class TestHandler(StepHandler):
            handler_name = "test_handler"

            def call(self, _context):
                return StepHandlerResult.success_handler_result({})

        registry.register("test_handler", TestHandler)
        handler = registry.resolve("test_handler")

        assert handler is not None
        assert handler.name == "test_handler"

    def test_resolve_not_found(self):
        """Test resolving non-existent handler returns None."""
        registry = HandlerRegistry()
        handler = registry.resolve("non_existent")
        assert handler is None

    def test_is_registered(self):
        """Test is_registered method."""
        registry = HandlerRegistry()

        class TestHandler(StepHandler):
            handler_name = "test_handler"

            def call(self, _context):
                return StepHandlerResult.success_handler_result({})

        assert not registry.is_registered("test_handler")
        registry.register("test_handler", TestHandler)
        assert registry.is_registered("test_handler")

    def test_list_handlers(self):
        """Test list_handlers method."""
        registry = HandlerRegistry()

        class Handler1(StepHandler):
            handler_name = "handler1"

            def call(self, _context):
                return StepHandlerResult.success_handler_result({})

        class Handler2(StepHandler):
            handler_name = "handler2"

            def call(self, _context):
                return StepHandlerResult.success_handler_result({})

        registry.register("handler1", Handler1)
        registry.register("handler2", Handler2)

        handlers = registry.list_handlers()
        assert set(handlers) == {"handler1", "handler2"}

    def test_unregister(self):
        """Test unregister method."""
        registry = HandlerRegistry()

        class TestHandler(StepHandler):
            handler_name = "test_handler"

            def call(self, _context):
                return StepHandlerResult.success_handler_result({})

        registry.register("test_handler", TestHandler)
        assert registry.is_registered("test_handler")

        result = registry.unregister("test_handler")
        assert result is True
        assert not registry.is_registered("test_handler")

    def test_unregister_not_found(self):
        """Test unregister returns False for non-existent handler."""
        registry = HandlerRegistry()
        result = registry.unregister("non_existent")
        assert result is False

    def test_handler_count(self):
        """Test handler_count method."""
        registry = HandlerRegistry()
        assert registry.handler_count() == 0

        class TestHandler(StepHandler):
            handler_name = "test"

            def call(self, _context):
                return StepHandlerResult.success_handler_result({})

        registry.register("test", TestHandler)
        assert registry.handler_count() == 1

    def test_clear(self):
        """Test clear method."""
        registry = HandlerRegistry()

        class TestHandler(StepHandler):
            handler_name = "test"

            def call(self, _context):
                return StepHandlerResult.success_handler_result({})

        registry.register("test", TestHandler)
        assert registry.handler_count() == 1

        registry.clear()
        assert registry.handler_count() == 0

    def test_get_handler_class(self):
        """Test get_handler_class without instantiation."""
        registry = HandlerRegistry()

        class TestHandler(StepHandler):
            handler_name = "test"
            handler_version = "2.0.0"

            def call(self, _context):
                return StepHandlerResult.success_handler_result({})

        registry.register("test", TestHandler)

        handler_class = registry.get_handler_class("test")
        assert handler_class is TestHandler
        assert handler_class.handler_version == "2.0.0"

    def test_register_overwrites_existing(self):
        """Test registering same name overwrites previous handler."""
        registry = HandlerRegistry()

        class Handler1(StepHandler):
            handler_name = "test"
            handler_version = "1.0.0"

            def call(self, _context):
                return StepHandlerResult.success_handler_result({})

        class Handler2(StepHandler):
            handler_name = "test"
            handler_version = "2.0.0"

            def call(self, _context):
                return StepHandlerResult.success_handler_result({})

        registry.register("test", Handler1)
        registry.register("test", Handler2)

        handler = registry.resolve("test")
        assert handler.version == "2.0.0"

    def test_register_invalid_class_raises(self):
        """Test registering non-StepHandler raises ValueError."""
        registry = HandlerRegistry()

        class NotAHandler:
            pass

        with pytest.raises(ValueError):
            registry.register("test", NotAHandler)


# =============================================================================
# StepHandler Tests
# =============================================================================


class TestStepHandler:
    """Tests for StepHandler ABC."""

    def test_handler_name_default(self):
        """Test handler name defaults to class name if not set."""

        class NoNameHandler(StepHandler):
            def call(self, _context):
                return StepHandlerResult.success_handler_result({})

        handler = NoNameHandler()
        assert handler.name == "NoNameHandler"

    def test_handler_name_custom(self):
        """Test custom handler name."""

        class CustomHandler(StepHandler):
            handler_name = "custom_handler"

            def call(self, _context):
                return StepHandlerResult.success_handler_result({})

        handler = CustomHandler()
        assert handler.name == "custom_handler"

    def test_handler_version_default(self):
        """Test default handler version."""

        class TestHandler(StepHandler):
            handler_name = "test"

            def call(self, _context):
                return StepHandlerResult.success_handler_result({})

        handler = TestHandler()
        assert handler.version == "1.0.0"

    def test_handler_version_custom(self):
        """Test custom handler version."""

        class TestHandler(StepHandler):
            handler_name = "test"
            handler_version = "2.5.0"

            def call(self, _context):
                return StepHandlerResult.success_handler_result({})

        handler = TestHandler()
        assert handler.version == "2.5.0"

    def test_capabilities_default(self):
        """Test default capabilities."""

        class TestHandler(StepHandler):
            handler_name = "test"

            def call(self, _context):
                return StepHandlerResult.success_handler_result({})

        handler = TestHandler()
        assert handler.capabilities == ["process"]

    def test_config_schema_default(self):
        """Test config_schema defaults to None."""

        class TestHandler(StepHandler):
            handler_name = "test"

            def call(self, _context):
                return StepHandlerResult.success_handler_result({})

        handler = TestHandler()
        assert handler.config_schema() is None

    def test_handler_repr(self):
        """Test handler string representation."""

        class TestHandler(StepHandler):
            handler_name = "test"
            handler_version = "1.0.0"

            def call(self, _context):
                return StepHandlerResult.success_handler_result({})

        handler = TestHandler()
        repr_str = repr(handler)
        assert "TestHandler" in repr_str
        assert "test" in repr_str


# =============================================================================
# StepContext Tests
# =============================================================================


class TestStepContext:
    """Tests for StepContext model."""

    def create_ffi_event(self) -> FfiStepEvent:
        """Create a test FfiStepEvent with Ruby-compatible nested structure.

        The structure mirrors Ruby's TaskSequenceStepWrapper:
        - step_definition.handler.callable -> handler name
        - step_definition.handler.initialization -> step config
        - task.context -> input data
        - dependency_results -> results from parent steps
        - workflow_step.attempts -> retry count
        - workflow_step.max_attempts -> max retries
        """
        return FfiStepEvent(
            event_id=str(uuid4()),
            task_uuid=str(uuid4()),
            step_uuid=str(uuid4()),
            correlation_id=str(uuid4()),
            task_sequence_step={
                "workflow_step": {
                    "name": "test_step",
                    "attempts": 1,
                    "max_attempts": 5,
                },
                "step_definition": {
                    "name": "test_step",
                    "handler": {
                        "callable": "test_handler",
                        "initialization": {"timeout": 30},
                    },
                },
                "task": {
                    "context": {"key": "value"},
                },
                "dependency_results": {"dep1": {"result": "data"}},
            },
        )

    def test_from_ffi_event(self):
        """Test creating StepContext from FfiStepEvent."""
        event = self.create_ffi_event()
        context = StepContext.from_ffi_event(event, "test_handler")

        assert str(context.task_uuid) == event.task_uuid
        assert str(context.step_uuid) == event.step_uuid
        assert context.handler_name == "test_handler"
        assert context.input_data == {"key": "value"}
        assert context.dependency_results == {"dep1": {"result": "data"}}
        assert context.step_config == {"timeout": 30}
        assert context.retry_count == 1
        assert context.max_retries == 5

    def test_context_defaults(self):
        """Test StepContext defaults."""
        event = FfiStepEvent(
            event_id=str(uuid4()),
            task_uuid=str(uuid4()),
            step_uuid=str(uuid4()),
            correlation_id=str(uuid4()),
            task_sequence_step={},
        )
        context = StepContext.from_ffi_event(event, "test")

        assert context.input_data == {}
        assert context.dependency_results == {}
        assert context.step_config == {}
        assert context.retry_count == 0
        assert context.max_retries == 3


# =============================================================================
# StepHandlerResult Tests
# =============================================================================


class TestStepHandlerResult:
    """Tests for StepHandlerResult model."""

    def test_success_handler_result(self):
        """Test creating success result."""
        result = StepHandlerResult.success_handler_result(
            {"key": "value"},
            {"duration_ms": 100},
        )

        assert result.success is True
        assert result.result == {"key": "value"}
        assert result.metadata == {"duration_ms": 100}
        assert result.error_message is None
        assert result.error_type is None

    def test_failure_handler_result(self):
        """Test creating failure result."""
        result = StepHandlerResult.failure_handler_result(
            message="Something went wrong",
            error_type="ValidationError",
            retryable=False,
            metadata={"field": "email"},
        )

        assert result.success is False
        assert result.error_message == "Something went wrong"
        assert result.error_type == "ValidationError"
        assert result.retryable is False
        assert result.metadata == {"field": "email"}

    def test_failure_handler_result_defaults(self):
        """Test failure result defaults."""
        result = StepHandlerResult.failure_handler_result("Error")

        assert result.success is False
        assert result.error_type == "handler_error"
        assert result.retryable is True
        assert result.metadata == {}


# =============================================================================
# StepExecutionSubscriber Tests
# =============================================================================


class TestStepExecutionSubscriber:
    """Tests for StepExecutionSubscriber."""

    def setup_method(self):
        """Reset singletons before each test."""
        EventBridge.reset_instance()
        HandlerRegistry.reset_instance()

    def teardown_method(self):
        """Clean up after each test."""
        EventBridge.reset_instance()
        HandlerRegistry.reset_instance()

    def test_subscriber_start_stop(self):
        """Test subscriber start/stop lifecycle."""
        bridge = EventBridge.instance()
        registry = HandlerRegistry.instance()
        subscriber = StepExecutionSubscriber(bridge, registry, "worker-001")

        assert not subscriber.is_active

        subscriber.start()
        assert subscriber.is_active
        assert bridge.listener_count(EventNames.STEP_EXECUTION_RECEIVED) == 1

        subscriber.stop()
        assert not subscriber.is_active
        assert bridge.listener_count(EventNames.STEP_EXECUTION_RECEIVED) == 0

    def test_subscriber_start_idempotent(self):
        """Test start() is idempotent."""
        bridge = EventBridge.instance()
        registry = HandlerRegistry.instance()
        subscriber = StepExecutionSubscriber(bridge, registry, "worker-001")

        subscriber.start()
        subscriber.start()  # Should not add duplicate listener
        assert bridge.listener_count(EventNames.STEP_EXECUTION_RECEIVED) == 1

    def test_subscriber_stop_idempotent(self):
        """Test stop() is idempotent."""
        bridge = EventBridge.instance()
        registry = HandlerRegistry.instance()
        subscriber = StepExecutionSubscriber(bridge, registry, "worker-001")

        subscriber.stop()  # Not started, should not raise
        assert not subscriber.is_active

    def test_worker_id_property(self):
        """Test worker_id property."""
        bridge = EventBridge.instance()
        registry = HandlerRegistry.instance()
        subscriber = StepExecutionSubscriber(bridge, registry, "worker-123")

        assert subscriber.worker_id == "worker-123"


class TestStepExecutionError:
    """Tests for StepExecutionError."""

    def test_basic_error(self):
        """Test basic error creation."""
        error = StepExecutionError("Something failed")
        assert str(error) == "Something failed"
        assert error.error_type == "step_execution_error"
        assert error.retryable is True

    def test_error_with_options(self):
        """Test error with custom options."""
        error = StepExecutionError(
            "Validation failed",
            error_type="validation_error",
            retryable=False,
        )
        assert error.error_type == "validation_error"
        assert error.retryable is False


# =============================================================================
# Integration Tests
# =============================================================================


class TestHandlerIntegration:
    """Integration tests for the handler system."""

    def setup_method(self):
        """Reset singletons before each test."""
        EventBridge.reset_instance()
        HandlerRegistry.reset_instance()

    def teardown_method(self):
        """Clean up after each test."""
        EventBridge.reset_instance()
        HandlerRegistry.reset_instance()

    def test_handler_execution_flow(self):
        """Test complete handler registration and resolution."""

        class ProcessOrderHandler(StepHandler):
            handler_name = "process_order"
            handler_version = "1.0.0"

            def call(self, context: StepContext) -> StepHandlerResult:
                order_id = context.input_data.get("order_id", "unknown")
                return StepHandlerResult.success_handler_result(
                    {
                        "order_id": order_id,
                        "status": "processed",
                    }
                )

        registry = HandlerRegistry.instance()
        registry.register("process_order", ProcessOrderHandler)

        handler = registry.resolve("process_order")
        assert handler is not None

        # Create mock context with Ruby-compatible nested structure
        event = FfiStepEvent(
            event_id=str(uuid4()),
            task_uuid=str(uuid4()),
            step_uuid=str(uuid4()),
            correlation_id=str(uuid4()),
            task_sequence_step={
                "step_definition": {
                    "handler": {
                        "callable": "process_order",
                    },
                },
                "task": {
                    "context": {"order_id": "ORD-123"},
                },
            },
        )
        context = StepContext.from_ffi_event(event, "process_order")

        result = handler.call(context)
        assert result.success is True
        assert result.result["order_id"] == "ORD-123"
        assert result.result["status"] == "processed"

    def test_event_bridge_handler_flow(self):
        """Test EventBridge with handler registration event."""
        bridge = EventBridge.instance()
        bridge.start()

        registered = []
        bridge.subscribe(
            EventNames.HANDLER_REGISTERED,
            lambda name, cls: registered.append((name, cls)),
        )

        class TestHandler(StepHandler):
            handler_name = "test"

            def call(self, _context):
                return StepHandlerResult.success_handler_result({})

        # Simulate handler registration notification
        bridge.publish(EventNames.HANDLER_REGISTERED, "test", TestHandler)

        assert len(registered) == 1
        assert registered[0][0] == "test"
        assert registered[0][1] is TestHandler

        bridge.stop()
