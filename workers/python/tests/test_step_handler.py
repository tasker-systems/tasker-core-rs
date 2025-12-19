"""Step handler tests.

These tests verify:
- StepHandler ABC contract
- StepContext model and extraction from FFI events
- StepHandlerResult factory methods
- StepExecutionSubscriber event routing
- StepExecutionError exception
- Handler integration flow
"""

from __future__ import annotations

from uuid import uuid4

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


class TestStepHandler:
    """Tests for StepHandler ABC."""

    def test_handler_name_default(self):
        """Test handler name defaults to class name if not set."""

        class NoNameHandler(StepHandler):
            def call(self, _context):
                return StepHandlerResult.success({})

        handler = NoNameHandler()
        assert handler.name == "NoNameHandler"

    def test_handler_name_custom(self):
        """Test custom handler name."""

        class CustomHandler(StepHandler):
            handler_name = "custom_handler"

            def call(self, _context):
                return StepHandlerResult.success({})

        handler = CustomHandler()
        assert handler.name == "custom_handler"

    def test_handler_version_default(self):
        """Test default handler version."""

        class TestHandler(StepHandler):
            handler_name = "test"

            def call(self, _context):
                return StepHandlerResult.success({})

        handler = TestHandler()
        assert handler.version == "1.0.0"

    def test_handler_version_custom(self):
        """Test custom handler version."""

        class TestHandler(StepHandler):
            handler_name = "test"
            handler_version = "2.5.0"

            def call(self, _context):
                return StepHandlerResult.success({})

        handler = TestHandler()
        assert handler.version == "2.5.0"

    def test_capabilities_default(self):
        """Test default capabilities."""

        class TestHandler(StepHandler):
            handler_name = "test"

            def call(self, _context):
                return StepHandlerResult.success({})

        handler = TestHandler()
        assert handler.capabilities == ["process"]

    def test_config_schema_default(self):
        """Test config_schema defaults to None."""

        class TestHandler(StepHandler):
            handler_name = "test"

            def call(self, _context):
                return StepHandlerResult.success({})

        handler = TestHandler()
        assert handler.config_schema() is None

    def test_handler_repr(self):
        """Test handler string representation."""

        class TestHandler(StepHandler):
            handler_name = "test"
            handler_version = "1.0.0"

            def call(self, _context):
                return StepHandlerResult.success({})

        handler = TestHandler()
        repr_str = repr(handler)
        assert "TestHandler" in repr_str
        assert "test" in repr_str


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

    def test_step_inputs_from_workflow_step(self):
        """Test StepContext correctly populates step_inputs from workflow_step.inputs."""
        event = FfiStepEvent(
            event_id=str(uuid4()),
            task_uuid=str(uuid4()),
            step_uuid=str(uuid4()),
            correlation_id=str(uuid4()),
            task_sequence_step={
                "workflow_step": {
                    "name": "batch_step",
                    "attempts": 0,
                    "max_attempts": 3,
                    "inputs": {
                        "batch_size": 100,
                        "start_cursor": 0,
                        "end_cursor": 1000,
                        "custom_config": {"key": "value"},
                    },
                },
                "step_definition": {
                    "name": "batch_step",
                    "handler": {"callable": "batch_handler"},
                },
                "task": {"context": {}},
            },
        )
        context = StepContext.from_ffi_event(event, "batch_handler")

        # Verify step_inputs are correctly extracted from workflow_step.inputs
        assert context.step_inputs == {
            "batch_size": 100,
            "start_cursor": 0,
            "end_cursor": 1000,
            "custom_config": {"key": "value"},
        }
        assert context.step_inputs["batch_size"] == 100
        assert context.step_inputs["custom_config"]["key"] == "value"

    def test_step_inputs_defaults_to_empty_dict(self):
        """Test step_inputs defaults to empty dict when workflow_step.inputs is missing."""
        event = FfiStepEvent(
            event_id=str(uuid4()),
            task_uuid=str(uuid4()),
            step_uuid=str(uuid4()),
            correlation_id=str(uuid4()),
            task_sequence_step={
                "workflow_step": {
                    "name": "test_step",
                    "attempts": 0,
                    "max_attempts": 3,
                    # No 'inputs' key
                },
            },
        )
        context = StepContext.from_ffi_event(event, "test_handler")

        assert context.step_inputs == {}

    def test_step_inputs_handles_none_value(self):
        """Test step_inputs handles None value gracefully."""
        event = FfiStepEvent(
            event_id=str(uuid4()),
            task_uuid=str(uuid4()),
            step_uuid=str(uuid4()),
            correlation_id=str(uuid4()),
            task_sequence_step={
                "workflow_step": {
                    "name": "test_step",
                    "attempts": 0,
                    "max_attempts": 3,
                    "inputs": None,  # Explicitly None
                },
            },
        )
        context = StepContext.from_ffi_event(event, "test_handler")

        # Should handle None and default to empty dict
        assert context.step_inputs == {}


class TestStepHandlerResult:
    """Tests for StepHandlerResult model."""

    def test_ok(self):
        """Test creating success result with ok()."""
        result = StepHandlerResult.success(
            {"key": "value"},
            {"duration_ms": 100},
        )

        assert result.is_success is True
        assert result.result == {"key": "value"}
        assert result.metadata == {"duration_ms": 100}
        assert result.error_message is None
        assert result.error_type is None

    def test_success_handler_result_alias(self):
        """Test success_handler_result alias for backward compatibility."""
        result = StepHandlerResult.success_handler_result(
            {"key": "value"},
            {"duration_ms": 100},
        )

        assert result.is_success is True
        assert result.result == {"key": "value"}

    def test_failure(self):
        """Test creating failure result."""
        result = StepHandlerResult.failure(
            message="Something went wrong",
            error_type="ValidationError",
            retryable=False,
            metadata={"field": "email"},
        )

        assert result.is_success is False
        assert result.error_message == "Something went wrong"
        assert result.error_type == "ValidationError"
        assert result.retryable is False
        assert result.metadata == {"field": "email"}

    def test_failure_with_error_code(self):
        """Test failure result with error_code."""
        result = StepHandlerResult.failure(
            message="Something went wrong",
            error_type="ValidationError",
            retryable=False,
            error_code="ERR_VALIDATION_001",
        )

        assert result.is_success is False
        assert result.error_code == "ERR_VALIDATION_001"

    def test_failure_handler_result_alias(self):
        """Test failure_handler_result alias for backward compatibility."""
        result = StepHandlerResult.failure_handler_result(
            message="Something went wrong",
            error_type="ValidationError",
            retryable=False,
        )

        assert result.is_success is False
        assert result.error_message == "Something went wrong"

    def test_failure_defaults(self):
        """Test failure result defaults."""
        result = StepHandlerResult.failure("Error")

        assert result.is_success is False
        assert result.error_type == "handler_error"
        assert result.retryable is True
        assert result.metadata == {}
        assert result.error_code is None


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
                return StepHandlerResult.success(
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
        assert result.is_success is True
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
                return StepHandlerResult.success({})

        # Simulate handler registration notification
        bridge.publish(EventNames.HANDLER_REGISTERED, "test", TestHandler)

        assert len(registered) == 1
        assert registered[0][0] == "test"
        assert registered[0][1] is TestHandler

        bridge.stop()
