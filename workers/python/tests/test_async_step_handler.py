"""Async step handler tests (TAS-131).

These tests verify that both synchronous and asynchronous step handlers
work correctly with the StepExecutionSubscriber.

The execution subscriber detects whether a handler's call() method is
async and handles it accordingly, maintaining backward compatibility
with sync handlers while enabling async patterns for I/O-bound operations.
"""

from __future__ import annotations

import asyncio
import inspect
from uuid import uuid4

import pytest

from tasker_core import (
    EventBridge,
    FfiStepEvent,
    HandlerRegistry,
    StepContext,
    StepExecutionSubscriber,
    StepHandler,
    StepHandlerResult,
)


class TestAsyncHandlerDetection:
    """Tests for detecting async vs sync handlers."""

    def test_sync_handler_not_detected_as_async(self):
        """Test that sync handlers are correctly identified."""

        class SyncHandler(StepHandler):
            handler_name = "sync_test"

            def call(self, _context: StepContext) -> StepHandlerResult:
                return StepHandlerResult.success({"sync": True})

        handler = SyncHandler()
        assert not inspect.iscoroutinefunction(handler.call)

    def test_async_handler_detected_as_async(self):
        """Test that async handlers are correctly identified."""

        class AsyncHandler(StepHandler):
            handler_name = "async_test"

            async def call(self, _context: StepContext) -> StepHandlerResult:
                await asyncio.sleep(0)
                return StepHandlerResult.success({"async": True})

        handler = AsyncHandler()
        assert inspect.iscoroutinefunction(handler.call)


class TestSyncHandlerExecution:
    """Tests for synchronous handler execution."""

    def setup_method(self):
        """Reset singletons before each test."""
        EventBridge.reset_instance()
        HandlerRegistry.reset_instance()

    def teardown_method(self):
        """Clean up after each test."""
        EventBridge.reset_instance()
        HandlerRegistry.reset_instance()

    def test_sync_handler_returns_result(self):
        """Test that sync handlers return results directly."""

        class SyncHandler(StepHandler):
            handler_name = "sync_processor"

            def call(self, context: StepContext) -> StepHandlerResult:
                value = context.input_data.get("value", 0)
                return StepHandlerResult.success({"processed_value": value * 2})

        registry = HandlerRegistry.instance()
        registry.register("sync_processor", SyncHandler)

        handler = registry.resolve("sync_processor")
        assert handler is not None

        event = self._create_test_event({"value": 21})
        context = StepContext.from_ffi_event(event, "sync_processor")

        result = handler.call(context)
        assert result.is_success
        assert result.result["processed_value"] == 42

    def test_sync_handler_failure(self):
        """Test that sync handlers can return failure results."""

        class FailingSyncHandler(StepHandler):
            handler_name = "failing_sync"

            def call(self, _context: StepContext) -> StepHandlerResult:
                return StepHandlerResult.failure(
                    message="Sync failure",
                    error_type="test_error",
                    retryable=False,
                )

        registry = HandlerRegistry.instance()
        registry.register("failing_sync", FailingSyncHandler)

        handler = registry.resolve("failing_sync")
        event = self._create_test_event({})
        context = StepContext.from_ffi_event(event, "failing_sync")

        result = handler.call(context)
        assert not result.is_success
        assert result.error_message == "Sync failure"
        assert not result.retryable

    def _create_test_event(self, input_data: dict) -> FfiStepEvent:
        """Create a test FfiStepEvent."""
        return FfiStepEvent(
            event_id=str(uuid4()),
            task_uuid=str(uuid4()),
            step_uuid=str(uuid4()),
            correlation_id=str(uuid4()),
            task_sequence_step={
                "task": {"context": input_data},
                "step_definition": {"handler": {"callable": "test_handler"}},
            },
        )


class TestAsyncHandlerExecution:
    """Tests for asynchronous handler execution."""

    def setup_method(self):
        """Reset singletons before each test."""
        EventBridge.reset_instance()
        HandlerRegistry.reset_instance()

    def teardown_method(self):
        """Clean up after each test."""
        EventBridge.reset_instance()
        HandlerRegistry.reset_instance()

    @pytest.mark.asyncio
    async def test_async_handler_awaitable(self):
        """Test that async handlers return awaitables."""

        class AsyncHandler(StepHandler):
            handler_name = "async_processor"

            async def call(self, context: StepContext) -> StepHandlerResult:
                await asyncio.sleep(0.01)
                value = context.input_data.get("value", 0)
                return StepHandlerResult.success({"processed_value": value * 2})

        registry = HandlerRegistry.instance()
        registry.register("async_processor", AsyncHandler)

        handler = registry.resolve("async_processor")
        assert handler is not None

        event = self._create_test_event({"value": 21})
        context = StepContext.from_ffi_event(event, "async_processor")

        # Call returns a coroutine
        coro = handler.call(context)
        assert inspect.iscoroutine(coro)

        # Await the coroutine
        result = await coro
        assert result.is_success
        assert result.result["processed_value"] == 42

    @pytest.mark.asyncio
    async def test_async_handler_with_real_async_work(self):
        """Test async handler that does actual async operations."""

        class AsyncIOHandler(StepHandler):
            handler_name = "async_io_handler"

            async def call(self, _context: StepContext) -> StepHandlerResult:
                # Simulate async I/O (e.g., database query, HTTP request)
                results = []
                for i in range(3):
                    await asyncio.sleep(0.01)
                    results.append(i * 2)
                return StepHandlerResult.success({"results": results})

        handler = AsyncIOHandler()
        event = self._create_test_event({})
        context = StepContext.from_ffi_event(event, "async_io_handler")

        result = await handler.call(context)
        assert result.is_success
        assert result.result["results"] == [0, 2, 4]

    @pytest.mark.asyncio
    async def test_async_handler_failure(self):
        """Test that async handlers can return failure results."""

        class FailingAsyncHandler(StepHandler):
            handler_name = "failing_async"

            async def call(self, _context: StepContext) -> StepHandlerResult:
                await asyncio.sleep(0.01)
                return StepHandlerResult.failure(
                    message="Async failure",
                    error_type="async_error",
                    retryable=True,
                )

        handler = FailingAsyncHandler()
        event = self._create_test_event({})
        context = StepContext.from_ffi_event(event, "failing_async")

        result = await handler.call(context)
        assert not result.is_success
        assert result.error_message == "Async failure"
        assert result.retryable

    @pytest.mark.asyncio
    async def test_async_handler_exception(self):
        """Test that exceptions in async handlers are properly raised."""

        class ExceptionAsyncHandler(StepHandler):
            handler_name = "exception_async"

            async def call(self, _context: StepContext) -> StepHandlerResult:
                await asyncio.sleep(0.01)
                raise ValueError("Async exception test")

        handler = ExceptionAsyncHandler()
        event = self._create_test_event({})
        context = StepContext.from_ffi_event(event, "exception_async")

        with pytest.raises(ValueError, match="Async exception test"):
            await handler.call(context)

    def _create_test_event(self, input_data: dict) -> FfiStepEvent:
        """Create a test FfiStepEvent."""
        return FfiStepEvent(
            event_id=str(uuid4()),
            task_uuid=str(uuid4()),
            step_uuid=str(uuid4()),
            correlation_id=str(uuid4()),
            task_sequence_step={
                "task": {"context": input_data},
                "step_definition": {"handler": {"callable": "test_handler"}},
            },
        )


class TestSubscriberAsyncHandling:
    """Tests for StepExecutionSubscriber handling of async handlers."""

    def setup_method(self):
        """Reset singletons before each test."""
        EventBridge.reset_instance()
        HandlerRegistry.reset_instance()

    def teardown_method(self):
        """Clean up after each test."""
        EventBridge.reset_instance()
        HandlerRegistry.reset_instance()

    def test_subscriber_executes_sync_handler(self):
        """Test subscriber correctly executes sync handlers."""

        class SyncHandler(StepHandler):
            handler_name = "sync_sub_test"
            executed = False

            def call(self, _context: StepContext) -> StepHandlerResult:
                SyncHandler.executed = True
                return StepHandlerResult.success({"sync": True})

        SyncHandler.executed = False

        bridge = EventBridge.instance()
        registry = HandlerRegistry.instance()
        registry.register("sync_sub_test", SyncHandler)

        subscriber = StepExecutionSubscriber(bridge, registry, "worker-test")

        # Use the subscriber's internal method to test execution
        handler = registry.resolve("sync_sub_test")
        event = self._create_test_event("sync_sub_test")

        result = subscriber._execute_handler(event, handler)

        assert SyncHandler.executed
        assert result.is_success
        assert result.result["sync"] is True

    def test_subscriber_executes_async_handler(self):
        """Test subscriber correctly executes async handlers."""

        class AsyncHandler(StepHandler):
            handler_name = "async_sub_test"
            executed = False

            async def call(self, _context: StepContext) -> StepHandlerResult:
                await asyncio.sleep(0.01)
                AsyncHandler.executed = True
                return StepHandlerResult.success({"async": True})

        AsyncHandler.executed = False

        bridge = EventBridge.instance()
        registry = HandlerRegistry.instance()
        registry.register("async_sub_test", AsyncHandler)

        subscriber = StepExecutionSubscriber(bridge, registry, "worker-test")

        handler = registry.resolve("async_sub_test")
        event = self._create_test_event("async_sub_test")

        # The subscriber should detect the async handler and run it properly
        result = subscriber._execute_handler(event, handler)

        assert AsyncHandler.executed
        assert result.is_success
        assert result.result["async"] is True

    def test_subscriber_handles_async_handler_exception(self):
        """Test subscriber handles exceptions from async handlers."""

        class FailingAsyncHandler(StepHandler):
            handler_name = "failing_async_sub"

            async def call(self, _context: StepContext) -> StepHandlerResult:
                await asyncio.sleep(0.01)
                raise RuntimeError("Async handler failed")

        bridge = EventBridge.instance()
        registry = HandlerRegistry.instance()
        registry.register("failing_async_sub", FailingAsyncHandler)

        subscriber = StepExecutionSubscriber(bridge, registry, "worker-test")

        handler = registry.resolve("failing_async_sub")
        event = self._create_test_event("failing_async_sub")

        # The subscriber should propagate the exception
        with pytest.raises(RuntimeError, match="Async handler failed"):
            subscriber._execute_handler(event, handler)

    def _create_test_event(self, handler_name: str) -> FfiStepEvent:
        """Create a test FfiStepEvent."""
        return FfiStepEvent(
            event_id=str(uuid4()),
            task_uuid=str(uuid4()),
            step_uuid=str(uuid4()),
            correlation_id=str(uuid4()),
            task_sequence_step={
                "task": {"context": {}},
                "step_definition": {"handler": {"callable": handler_name}},
                "workflow_step": {"attempts": 0, "max_attempts": 3},
            },
        )


class TestMixedHandlerScenarios:
    """Tests for scenarios with both sync and async handlers."""

    def setup_method(self):
        """Reset singletons before each test."""
        EventBridge.reset_instance()
        HandlerRegistry.reset_instance()

    def teardown_method(self):
        """Clean up after each test."""
        EventBridge.reset_instance()
        HandlerRegistry.reset_instance()

    def test_registry_holds_both_sync_and_async(self):
        """Test that registry can hold and resolve both handler types."""

        class SyncHandler(StepHandler):
            handler_name = "mixed_sync"

            def call(self, _context: StepContext) -> StepHandlerResult:
                return StepHandlerResult.success({"type": "sync"})

        class AsyncHandler(StepHandler):
            handler_name = "mixed_async"

            async def call(self, _context: StepContext) -> StepHandlerResult:
                return StepHandlerResult.success({"type": "async"})

        registry = HandlerRegistry.instance()
        registry.register("mixed_sync", SyncHandler)
        registry.register("mixed_async", AsyncHandler)

        sync_handler = registry.resolve("mixed_sync")
        async_handler = registry.resolve("mixed_async")

        assert sync_handler is not None
        assert async_handler is not None
        assert not inspect.iscoroutinefunction(sync_handler.call)
        assert inspect.iscoroutinefunction(async_handler.call)

    def test_subscriber_routes_correctly_to_both_types(self):
        """Test subscriber routes events to correct handler type."""

        class SyncHandler(StepHandler):
            handler_name = "route_sync"
            call_count = 0

            def call(self, _context: StepContext) -> StepHandlerResult:
                SyncHandler.call_count += 1
                return StepHandlerResult.success({"handler": "sync"})

        class AsyncHandler(StepHandler):
            handler_name = "route_async"
            call_count = 0

            async def call(self, _context: StepContext) -> StepHandlerResult:
                await asyncio.sleep(0.01)
                AsyncHandler.call_count += 1
                return StepHandlerResult.success({"handler": "async"})

        SyncHandler.call_count = 0
        AsyncHandler.call_count = 0

        bridge = EventBridge.instance()
        registry = HandlerRegistry.instance()
        registry.register("route_sync", SyncHandler)
        registry.register("route_async", AsyncHandler)

        subscriber = StepExecutionSubscriber(bridge, registry, "worker-test")

        # Execute sync handler
        sync_handler = registry.resolve("route_sync")
        sync_event = self._create_test_event("route_sync")
        sync_result = subscriber._execute_handler(sync_event, sync_handler)

        # Execute async handler
        async_handler = registry.resolve("route_async")
        async_event = self._create_test_event("route_async")
        async_result = subscriber._execute_handler(async_event, async_handler)

        assert SyncHandler.call_count == 1
        assert AsyncHandler.call_count == 1
        assert sync_result.result["handler"] == "sync"
        assert async_result.result["handler"] == "async"

    def _create_test_event(self, handler_name: str) -> FfiStepEvent:
        """Create a test FfiStepEvent."""
        return FfiStepEvent(
            event_id=str(uuid4()),
            task_uuid=str(uuid4()),
            step_uuid=str(uuid4()),
            correlation_id=str(uuid4()),
            task_sequence_step={
                "task": {"context": {}},
                "step_definition": {"handler": {"callable": handler_name}},
                "workflow_step": {"attempts": 0, "max_attempts": 3},
            },
        )
