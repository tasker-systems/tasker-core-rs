"""Tests for StepExecutionSubscriber internal methods.

These tests cover the internal execution logic of the StepExecutionSubscriber
without requiring a running worker.
"""

from __future__ import annotations

from unittest.mock import patch
from uuid import uuid4

import pytest

from tasker_core import (
    EventBridge,
    EventNames,
    FfiStepEvent,
    HandlerRegistry,
    StepExecutionSubscriber,
    StepHandler,
    StepHandlerResult,
)


def create_test_event(
    handler_name: str = "test_handler",
    input_data: dict | None = None,
) -> FfiStepEvent:
    """Create a test FfiStepEvent with Ruby-compatible nested structure.

    The structure mirrors Ruby's TaskSequenceStepWrapper:
    - step_definition.handler.callable -> handler name
    - task.context or task.task.context -> input data
    - workflow_step.attempts -> retry count
    """
    return FfiStepEvent(
        event_id=str(uuid4()),
        task_uuid=str(uuid4()),
        step_uuid=str(uuid4()),
        correlation_id=str(uuid4()),
        task_sequence_step={
            "workflow_step": {
                "name": "test_step",
                "attempts": 0,
                "max_attempts": 3,
            },
            "step_definition": {
                "name": "test_step",
                "handler": {
                    "callable": handler_name,
                    "initialization": {},
                },
            },
            "task": {
                "context": input_data or {},
            },
            "dependency_results": {},
        },
    )


class TestGetHandlerName:
    """Tests for _get_handler_name method."""

    def setup_method(self):
        """Reset singletons before each test."""
        EventBridge.reset_instance()
        HandlerRegistry.reset_instance()

    def teardown_method(self):
        """Clean up after each test."""
        EventBridge.reset_instance()
        HandlerRegistry.reset_instance()

    def test_get_handler_name_from_step_definition(self):
        """Test extracting handler name from step_definition.handler.callable (primary)."""
        bridge = EventBridge.instance()
        registry = HandlerRegistry.instance()
        subscriber = StepExecutionSubscriber(bridge, registry, "worker-001")

        event = FfiStepEvent(
            event_id=str(uuid4()),
            task_uuid=str(uuid4()),
            step_uuid=str(uuid4()),
            correlation_id=str(uuid4()),
            task_sequence_step={
                "step_definition": {
                    "handler": {
                        "callable": "my_custom_handler",
                    },
                },
            },
        )

        name = subscriber._get_handler_name(event)
        assert name == "my_custom_handler"

    def test_get_handler_name_from_step_template(self):
        """Test extracting handler name from step_template.handler_class (legacy fallback)."""
        bridge = EventBridge.instance()
        registry = HandlerRegistry.instance()
        subscriber = StepExecutionSubscriber(bridge, registry, "worker-001")

        event = FfiStepEvent(
            event_id=str(uuid4()),
            task_uuid=str(uuid4()),
            step_uuid=str(uuid4()),
            correlation_id=str(uuid4()),
            task_sequence_step={
                "step_template": {
                    "handler_class": "TemplateHandler",
                },
            },
        )

        name = subscriber._get_handler_name(event)
        assert name == "TemplateHandler"

    def test_get_handler_name_fallback_to_template_step_name(self):
        """Test fallback to workflow_step.template_step_name for batch processing."""
        bridge = EventBridge.instance()
        registry = HandlerRegistry.instance()
        subscriber = StepExecutionSubscriber(bridge, registry, "worker-001")

        event = FfiStepEvent(
            event_id=str(uuid4()),
            task_uuid=str(uuid4()),
            step_uuid=str(uuid4()),
            correlation_id=str(uuid4()),
            task_sequence_step={
                "workflow_step": {
                    "template_step_name": "fallback_step_name",
                },
            },
        )

        name = subscriber._get_handler_name(event)
        assert name == "fallback_step_name"

    def test_get_handler_name_unknown_when_nothing_set(self):
        """Test returning unknown_handler when nothing is set."""
        bridge = EventBridge.instance()
        registry = HandlerRegistry.instance()
        subscriber = StepExecutionSubscriber(bridge, registry, "worker-001")

        event = FfiStepEvent(
            event_id=str(uuid4()),
            task_uuid=str(uuid4()),
            step_uuid=str(uuid4()),
            correlation_id=str(uuid4()),
            task_sequence_step={},
        )

        name = subscriber._get_handler_name(event)
        assert name == "unknown_handler"


class TestExecuteHandler:
    """Tests for _execute_handler method."""

    def setup_method(self):
        """Reset singletons before each test."""
        EventBridge.reset_instance()
        HandlerRegistry.reset_instance()

    def teardown_method(self):
        """Clean up after each test."""
        EventBridge.reset_instance()
        HandlerRegistry.reset_instance()

    def test_execute_handler_success(self):
        """Test executing a handler that returns success."""

        class SuccessHandler(StepHandler):
            handler_name = "success_handler"

            def call(self, _context):
                return StepHandlerResult.success_handler_result({"success": True})

        bridge = EventBridge.instance()
        registry = HandlerRegistry.instance()
        subscriber = StepExecutionSubscriber(bridge, registry, "worker-001")

        event = create_test_event()
        handler = SuccessHandler()

        result = subscriber._execute_handler(event, handler)

        assert result.success is True
        assert result.result == {"success": True}

    def test_execute_handler_with_input_data(self):
        """Test handler receives input data from event."""

        class DataHandler(StepHandler):
            handler_name = "data_handler"

            def call(self, context):
                value = context.input_data.get("value", 0)
                return StepHandlerResult.success_handler_result({"doubled": value * 2})

        bridge = EventBridge.instance()
        registry = HandlerRegistry.instance()
        subscriber = StepExecutionSubscriber(bridge, registry, "worker-001")

        event = create_test_event(input_data={"value": 5})
        handler = DataHandler()

        result = subscriber._execute_handler(event, handler)

        assert result.success is True
        assert result.result == {"doubled": 10}


class TestCreateErrorResults:
    """Tests for error result creation methods."""

    def setup_method(self):
        """Reset singletons before each test."""
        EventBridge.reset_instance()
        HandlerRegistry.reset_instance()

    def teardown_method(self):
        """Clean up after each test."""
        EventBridge.reset_instance()
        HandlerRegistry.reset_instance()

    def test_create_handler_not_found_result(self):
        """Test creating handler not found result."""
        bridge = EventBridge.instance()
        registry = HandlerRegistry.instance()
        subscriber = StepExecutionSubscriber(bridge, registry, "worker-001")

        result = subscriber._create_handler_not_found_result("missing_handler")

        assert result.success is False
        assert result.error_type == "handler_not_found"
        assert "missing_handler" in result.error_message
        assert result.retryable is False

    def test_create_error_result(self):
        """Test creating error result from exception."""
        bridge = EventBridge.instance()
        registry = HandlerRegistry.instance()
        subscriber = StepExecutionSubscriber(bridge, registry, "worker-001")

        error = ValueError("Something went wrong")
        result = subscriber._create_error_result(error)

        assert result.success is False
        assert result.error_type == "ValueError"
        assert "Something went wrong" in result.error_message
        assert result.retryable is True
        assert "traceback" in result.metadata


class TestHandleExecutionEvent:
    """Tests for _handle_execution_event method with mocking."""

    def setup_method(self):
        """Reset singletons before each test."""
        EventBridge.reset_instance()
        HandlerRegistry.reset_instance()

    def teardown_method(self):
        """Clean up after each test."""
        EventBridge.reset_instance()
        HandlerRegistry.reset_instance()

    @patch("tasker_core.step_execution_subscriber._complete_step_event")
    def test_handle_execution_event_with_registered_handler(self, mock_complete):
        """Test receiving step with registered handler."""
        mock_complete.return_value = True

        class TestHandler(StepHandler):
            handler_name = "test_handler"

            def call(self, _context):
                return StepHandlerResult.success_handler_result({"processed": True})

        bridge = EventBridge.instance()
        bridge.start()
        registry = HandlerRegistry.instance()
        registry.register("test_handler", TestHandler)

        subscriber = StepExecutionSubscriber(bridge, registry, "worker-001")

        event = create_test_event("test_handler")
        subscriber._handle_execution_event(event)

        # Verify FFI was called
        mock_complete.assert_called_once()
        call_args = mock_complete.call_args
        assert call_args[0][0] == event.event_id

    @patch("tasker_core.step_execution_subscriber._complete_step_event")
    def test_handle_execution_event_handler_not_found(self, mock_complete):
        """Test receiving step with unregistered handler."""
        mock_complete.return_value = True

        bridge = EventBridge.instance()
        bridge.start()
        registry = HandlerRegistry.instance()
        # Don't register any handler

        subscriber = StepExecutionSubscriber(bridge, registry, "worker-001")

        event = create_test_event("nonexistent_handler")
        subscriber._handle_execution_event(event)

        # Verify FFI was called with failure result
        # Handler not found is not retryable, so it's permanent_error
        mock_complete.assert_called_once()
        call_args = mock_complete.call_args
        result_dict = call_args[0][1]
        assert result_dict["status"] == "permanent_error"

    @patch("tasker_core.step_execution_subscriber._complete_step_event")
    def test_handle_execution_event_handler_exception(self, mock_complete):
        """Test receiving step when handler raises exception."""
        mock_complete.return_value = True

        class FailingHandler(StepHandler):
            handler_name = "failing_handler"

            def call(self, _context):
                raise RuntimeError("Handler crashed")

        bridge = EventBridge.instance()
        bridge.start()
        registry = HandlerRegistry.instance()
        registry.register("failing_handler", FailingHandler)

        subscriber = StepExecutionSubscriber(bridge, registry, "worker-001")

        event = create_test_event("failing_handler")
        subscriber._handle_execution_event(event)

        # Verify FFI was called with failure result
        # Exception handling is retryable by default
        mock_complete.assert_called_once()
        call_args = mock_complete.call_args
        result_dict = call_args[0][1]
        assert result_dict["status"] == "retryable_error"
        assert "RuntimeError" in result_dict["error"]["error_type"]


class TestSubmitResult:
    """Tests for _submit_result method with mocking."""

    def setup_method(self):
        """Reset singletons before each test."""
        EventBridge.reset_instance()
        HandlerRegistry.reset_instance()

    def teardown_method(self):
        """Clean up after each test."""
        EventBridge.reset_instance()
        HandlerRegistry.reset_instance()

    @patch("tasker_core.step_execution_subscriber._complete_step_event")
    def test_submit_result_success(self, mock_complete):
        """Test submitting successful result."""
        mock_complete.return_value = True

        bridge = EventBridge.instance()
        bridge.start()
        registry = HandlerRegistry.instance()
        subscriber = StepExecutionSubscriber(bridge, registry, "worker-001")

        event = create_test_event()
        handler_result = StepHandlerResult.success_handler_result({"data": "value"})

        # Track event publication
        published = []
        bridge.subscribe(EventNames.STEP_COMPLETION_SENT, lambda r: published.append(r))

        subscriber._submit_result(event, handler_result, execution_time_ms=100)

        # Verify completion event was published
        assert len(published) == 1
        mock_complete.assert_called_once()

    @patch("tasker_core.step_execution_subscriber._complete_step_event")
    def test_submit_result_failure_permanent(self, mock_complete):
        """Test submitting non-retryable failure result."""
        mock_complete.return_value = True

        bridge = EventBridge.instance()
        bridge.start()
        registry = HandlerRegistry.instance()
        subscriber = StepExecutionSubscriber(bridge, registry, "worker-001")

        event = create_test_event()
        handler_result = StepHandlerResult.failure_handler_result(
            message="Something failed",
            error_type="test_error",
            retryable=False,
        )

        subscriber._submit_result(event, handler_result, execution_time_ms=50)

        # Verify FFI was called with permanent_error (not retryable)
        mock_complete.assert_called_once()
        call_args = mock_complete.call_args
        result_dict = call_args[0][1]
        assert result_dict["status"] == "permanent_error"
        assert result_dict["error"]["error_type"] == "test_error"

    @patch("tasker_core.step_execution_subscriber._complete_step_event")
    def test_submit_result_failure_retryable(self, mock_complete):
        """Test submitting retryable failure result."""
        mock_complete.return_value = True

        bridge = EventBridge.instance()
        bridge.start()
        registry = HandlerRegistry.instance()
        subscriber = StepExecutionSubscriber(bridge, registry, "worker-001")

        event = create_test_event()
        handler_result = StepHandlerResult.failure_handler_result(
            message="Temporary failure",
            error_type="test_error",
            retryable=True,
        )

        subscriber._submit_result(event, handler_result, execution_time_ms=50)

        # Verify FFI was called with retryable_error
        mock_complete.assert_called_once()
        call_args = mock_complete.call_args
        result_dict = call_args[0][1]
        assert result_dict["status"] == "retryable_error"
        assert result_dict["error"]["error_type"] == "test_error"

    @patch("tasker_core.step_execution_subscriber._complete_step_event")
    def test_submit_result_ffi_failure(self, mock_complete):
        """Test when FFI returns failure."""
        mock_complete.return_value = False  # FFI indicates failure

        bridge = EventBridge.instance()
        bridge.start()
        registry = HandlerRegistry.instance()
        subscriber = StepExecutionSubscriber(bridge, registry, "worker-001")

        event = create_test_event()
        handler_result = StepHandlerResult.success_handler_result({})

        # Track event publication
        published = []
        bridge.subscribe(EventNames.STEP_COMPLETION_SENT, lambda r: published.append(r))

        subscriber._submit_result(event, handler_result, execution_time_ms=100)

        # Completion event should NOT be published when FFI fails
        assert len(published) == 0

    @patch("tasker_core.step_execution_subscriber._complete_step_event")
    def test_submit_result_ffi_exception(self, mock_complete):
        """Test when FFI raises exception."""
        mock_complete.side_effect = RuntimeError("FFI error")

        bridge = EventBridge.instance()
        bridge.start()
        registry = HandlerRegistry.instance()
        subscriber = StepExecutionSubscriber(bridge, registry, "worker-001")

        event = create_test_event()
        handler_result = StepHandlerResult.success_handler_result({})

        # Exception should propagate
        with pytest.raises(RuntimeError, match="FFI error"):
            subscriber._submit_result(event, handler_result, execution_time_ms=100)
