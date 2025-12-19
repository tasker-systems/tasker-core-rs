# TAS-72-P4: Event Bridge & Handler System

## Overview

This phase implements the in-process event system and handler infrastructure, completing the step execution lifecycle from event receipt through handler execution to result submission.

## Prerequisites

- TAS-72-P1 through P3 complete
- Event polling and completion working

## Objectives

1. Implement `EventBridge` using `pyee`
2. Create `HandlerRegistry` for handler discovery and resolution
3. Define `StepHandler` base class with execution protocol
4. Implement `StepExecutionSubscriber` for event-to-handler routing
5. Complete end-to-end step execution flow

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        EventBridge (pyee)                        │
│  Events: step.execution.received, step.completion.sent,          │
│          handler.registered, handler.error                       │
└─────────────────────────────────────────────────────────────────┘
            │                                    ▲
            ▼                                    │
┌───────────────────────────────────────────────────────────────────┐
│                    StepExecutionSubscriber                         │
│  - Subscribes to step.execution.received                          │
│  - Resolves handler from HandlerRegistry                          │
│  - Executes handler.call(context)                                 │
│  - Publishes step.completion.sent with result                     │
└───────────────────────────────────────────────────────────────────┘
            │                                    ▲
            ▼                                    │
┌───────────────────────────────────────────────────────────────────┐
│                      HandlerRegistry                               │
│  - register(name, handler_class)                                  │
│  - resolve(handler_name) -> StepHandler                           │
│  - discover_handlers(paths)                                       │
└───────────────────────────────────────────────────────────────────┘
            │
            ▼
┌───────────────────────────────────────────────────────────────────┐
│                        StepHandler (ABC)                           │
│  - call(context: StepContext) -> StepHandlerResult                │
│  - name: str                                                      │
│  - version: str                                                   │
│  - capabilities: list[str]                                        │
└───────────────────────────────────────────────────────────────────┘
```

## EventBridge Implementation

```python
from pyee import EventEmitter
from typing import Callable, Any
import logging

logger = logging.getLogger(__name__)

class EventBridge:
    """In-process event bus for step execution coordination.

    This is the Python equivalent of Ruby's dry-events integration,
    providing pub/sub functionality for coordinating step handlers.

    Events:
        step.execution.received: Step ready for execution
        step.completion.sent: Step execution completed
        handler.registered: New handler registered
        handler.error: Handler execution error
        poller.metrics: Dispatch metrics update
        poller.error: Poller error

    Example:
        bridge = EventBridge.instance()
        bridge.subscribe("step.execution.received", my_handler)
        bridge.publish("step.execution.received", event_data)
    """

    _instance: Optional["EventBridge"] = None

    def __init__(self) -> None:
        self._emitter = EventEmitter()
        self._active = False
        self._setup_event_schema()

    @classmethod
    def instance(cls) -> "EventBridge":
        """Get the singleton EventBridge instance."""
        if cls._instance is None:
            cls._instance = cls()
        return cls._instance

    @classmethod
    def reset_instance(cls) -> None:
        """Reset the singleton (for testing)."""
        if cls._instance:
            cls._instance.stop()
        cls._instance = None

    def _setup_event_schema(self) -> None:
        """Define the event schema for documentation."""
        self._event_schema = {
            "step.execution.received": "FfiStepEvent",
            "step.completion.sent": "StepExecutionResult",
            "handler.registered": "tuple[str, type]",
            "handler.error": "tuple[FfiStepEvent, Exception]",
            "poller.metrics": "FfiDispatchMetrics",
            "poller.error": "Exception",
        }

    def start(self) -> None:
        """Activate the event bridge."""
        self._active = True
        logger.info("EventBridge started")

    def stop(self) -> None:
        """Deactivate the event bridge."""
        self._active = False
        self._emitter.remove_all_listeners()
        logger.info("EventBridge stopped")

    def subscribe(
        self,
        event: str,
        handler: Callable[..., Any],
    ) -> None:
        """Subscribe to an event.

        Args:
            event: Event name to subscribe to
            handler: Callback function
        """
        self._emitter.on(event, handler)
        logger.debug(f"Subscribed to {event}: {handler.__name__}")

    def unsubscribe(
        self,
        event: str,
        handler: Callable[..., Any],
    ) -> None:
        """Unsubscribe from an event."""
        self._emitter.remove_listener(event, handler)

    def publish(self, event: str, *args: Any, **kwargs: Any) -> None:
        """Publish an event.

        Args:
            event: Event name to publish
            *args: Positional arguments for handlers
            **kwargs: Keyword arguments for handlers
        """
        if not self._active:
            logger.warning(f"EventBridge not active, dropping event: {event}")
            return

        logger.debug(f"Publishing event: {event}")
        self._emitter.emit(event, *args, **kwargs)

    @property
    def is_active(self) -> bool:
        """Check if the event bridge is active."""
        return self._active
```

## HandlerRegistry Implementation

```python
from abc import ABC, abstractmethod
from typing import Type, Optional
import importlib
import pkgutil
import logging

logger = logging.getLogger(__name__)

class HandlerRegistry:
    """Registry for step handler classes.

    Provides handler discovery, registration, and resolution.
    Supports multiple discovery modes:
    - Manual registration via register()
    - Package scanning via discover_handlers()

    Example:
        registry = HandlerRegistry.instance()
        registry.register("my_handler", MyHandler)
        handler = registry.resolve("my_handler")
    """

    _instance: Optional["HandlerRegistry"] = None

    def __init__(self) -> None:
        self._handlers: dict[str, Type["StepHandler"]] = {}

    @classmethod
    def instance(cls) -> "HandlerRegistry":
        """Get the singleton registry instance."""
        if cls._instance is None:
            cls._instance = cls()
        return cls._instance

    @classmethod
    def reset_instance(cls) -> None:
        """Reset the singleton (for testing)."""
        cls._instance = None

    def register(
        self,
        name: str,
        handler_class: Type["StepHandler"],
    ) -> None:
        """Register a handler class.

        Args:
            name: Handler name (must match step definition)
            handler_class: StepHandler subclass
        """
        if name in self._handlers:
            logger.warning(f"Overwriting existing handler: {name}")

        self._handlers[name] = handler_class
        logger.info(f"Registered handler: {name} -> {handler_class.__name__}")

    def unregister(self, name: str) -> bool:
        """Unregister a handler."""
        if name in self._handlers:
            del self._handlers[name]
            return True
        return False

    def resolve(self, name: str) -> Optional["StepHandler"]:
        """Resolve and instantiate a handler by name.

        Args:
            name: Handler name to resolve

        Returns:
            Instantiated handler or None if not found
        """
        handler_class = self._handlers.get(name)
        if handler_class is None:
            logger.warning(f"Handler not found: {name}")
            return None

        try:
            return handler_class()
        except Exception as e:
            logger.error(f"Failed to instantiate handler {name}: {e}")
            return None

    def is_registered(self, name: str) -> bool:
        """Check if a handler is registered."""
        return name in self._handlers

    def list_handlers(self) -> list[str]:
        """List all registered handler names."""
        return list(self._handlers.keys())

    def discover_handlers(
        self,
        package_name: str,
        base_class: Optional[Type["StepHandler"]] = None,
    ) -> int:
        """Discover and register handlers from a package.

        Args:
            package_name: Package to scan (e.g., "myapp.handlers")
            base_class: Base class to filter by (default: StepHandler)

        Returns:
            Number of handlers discovered
        """
        base = base_class or StepHandler
        discovered = 0

        try:
            package = importlib.import_module(package_name)
        except ImportError as e:
            logger.error(f"Failed to import package {package_name}: {e}")
            return 0

        for _, module_name, _ in pkgutil.walk_packages(
            package.__path__,
            prefix=f"{package_name}.",
        ):
            try:
                module = importlib.import_module(module_name)
                for name in dir(module):
                    obj = getattr(module, name)
                    if (
                        isinstance(obj, type)
                        and issubclass(obj, base)
                        and obj is not base
                        and hasattr(obj, "handler_name")
                    ):
                        handler_name = obj.handler_name
                        self.register(handler_name, obj)
                        discovered += 1
            except Exception as e:
                logger.warning(f"Failed to scan module {module_name}: {e}")

        logger.info(f"Discovered {discovered} handlers in {package_name}")
        return discovered
```

## StepHandler Base Class

```python
from abc import ABC, abstractmethod
from pydantic import BaseModel, Field
from typing import Any, Optional
from uuid import UUID
import time

class StepContext(BaseModel):
    """Context provided to step handlers."""
    event: "FfiStepEvent"
    task_uuid: UUID
    step_uuid: UUID
    correlation_id: UUID
    handler_name: str
    input_data: dict[str, Any] = Field(default_factory=dict)
    dependency_results: dict[str, Any] = Field(default_factory=dict)
    step_config: dict[str, Any] = Field(default_factory=dict)
    retry_count: int = 0
    max_retries: int = 3

    class Config:
        arbitrary_types_allowed = True

class StepHandlerResult(BaseModel):
    """Result from a step handler execution."""
    success: bool
    result: Optional[dict[str, Any]] = None
    error_message: Optional[str] = None
    error_type: Optional[str] = None
    retryable: bool = True
    metadata: dict[str, Any] = Field(default_factory=dict)

    @classmethod
    def success(
        cls,
        result: dict[str, Any],
        metadata: Optional[dict[str, Any]] = None,
    ) -> "StepHandlerResult":
        """Create a success result."""
        return cls(
            success=True,
            result=result,
            metadata=metadata or {},
        )

    @classmethod
    def failure(
        cls,
        message: str,
        error_type: str = "handler_error",
        retryable: bool = True,
        metadata: Optional[dict[str, Any]] = None,
    ) -> "StepHandlerResult":
        """Create a failure result."""
        return cls(
            success=False,
            error_message=message,
            error_type=error_type,
            retryable=retryable,
            metadata=metadata or {},
        )


class StepHandler(ABC):
    """Abstract base class for step handlers.

    All step handlers must inherit from this class and implement
    the `call` method.

    Example:
        class MyHandler(StepHandler):
            handler_name = "my_handler"

            def call(self, context: StepContext) -> StepHandlerResult:
                # Do work
                return StepHandlerResult.success({"key": "value"})
    """

    # Class attribute - must be set by subclasses
    handler_name: str = ""
    handler_version: str = "1.0.0"

    @abstractmethod
    def call(self, context: StepContext) -> StepHandlerResult:
        """Execute the step handler logic.

        Args:
            context: Execution context with input data and config

        Returns:
            StepHandlerResult indicating success or failure
        """
        ...

    @property
    def name(self) -> str:
        """Return the handler name."""
        return self.handler_name or self.__class__.__name__

    @property
    def version(self) -> str:
        """Return the handler version."""
        return self.handler_version

    @property
    def capabilities(self) -> list[str]:
        """Return handler capabilities."""
        return ["process"]

    def config_schema(self) -> Optional[dict[str, Any]]:
        """Return JSON schema for handler configuration."""
        return None
```

## StepExecutionSubscriber

```python
import logging
import time
from typing import Optional

from tasker_core._tasker_core import complete_step_event
from tasker_core.event_bridge import EventBridge
from tasker_core.handler_registry import HandlerRegistry
from tasker_core.types import (
    FfiStepEvent,
    StepExecutionResult,
    StepError,
    StepContext,
)

logger = logging.getLogger(__name__)

class StepExecutionSubscriber:
    """Subscriber that routes step events to handlers.

    Subscribes to step.execution.received events and:
    1. Resolves the appropriate handler
    2. Executes the handler
    3. Submits the result via FFI
    4. Publishes step.completion.sent event
    """

    def __init__(
        self,
        event_bridge: EventBridge,
        handler_registry: HandlerRegistry,
        worker_id: str,
    ) -> None:
        self._event_bridge = event_bridge
        self._registry = handler_registry
        self._worker_id = worker_id
        self._active = False

    def start(self) -> None:
        """Start subscribing to execution events."""
        self._event_bridge.subscribe(
            "step.execution.received",
            self._handle_execution_event,
        )
        self._active = True
        logger.info("StepExecutionSubscriber started")

    def stop(self) -> None:
        """Stop subscribing."""
        self._event_bridge.unsubscribe(
            "step.execution.received",
            self._handle_execution_event,
        )
        self._active = False
        logger.info("StepExecutionSubscriber stopped")

    def _handle_execution_event(self, event: FfiStepEvent) -> None:
        """Handle a step execution event."""
        start_time = time.time()
        handler_name = event.execution_event.handler_name

        logger.info(
            f"Executing step {event.step_uuid} with handler {handler_name}"
        )

        try:
            # Resolve handler
            handler = self._registry.resolve(handler_name)
            if handler is None:
                result = self._create_handler_not_found_result(event, start_time)
            else:
                # Execute handler
                result = self._execute_handler(event, handler, start_time)

        except Exception as e:
            logger.exception(f"Error executing handler {handler_name}")
            result = self._create_error_result(event, e, start_time)
            self._event_bridge.publish("handler.error", (event, e))

        # Submit result via FFI
        execution_time_ms = int((time.time() - start_time) * 1000)
        self._submit_result(event, result, execution_time_ms)

    def _execute_handler(
        self,
        event: FfiStepEvent,
        handler: "StepHandler",
        start_time: float,
    ) -> "StepHandlerResult":
        """Execute a handler and return the result."""
        context = StepContext(
            event=event,
            task_uuid=event.task_uuid,
            step_uuid=event.step_uuid,
            correlation_id=event.correlation_id,
            handler_name=handler.name,
            input_data=event.execution_event.input_data,
            dependency_results=event.execution_event.dependency_results,
            step_config=event.execution_event.step_config,
            retry_count=event.execution_event.retry_count,
            max_retries=event.execution_event.max_retries,
        )

        return handler.call(context)

    def _create_handler_not_found_result(
        self,
        event: FfiStepEvent,
        start_time: float,
    ) -> "StepHandlerResult":
        """Create result for handler not found."""
        return StepHandlerResult.failure(
            message=f"Handler not found: {event.execution_event.handler_name}",
            error_type="handler_not_found",
            retryable=False,
        )

    def _create_error_result(
        self,
        event: FfiStepEvent,
        error: Exception,
        start_time: float,
    ) -> "StepHandlerResult":
        """Create result for handler execution error."""
        import traceback

        return StepHandlerResult.failure(
            message=str(error),
            error_type="handler_error",
            retryable=True,
            metadata={"traceback": traceback.format_exc()},
        )

    def _submit_result(
        self,
        event: FfiStepEvent,
        handler_result: "StepHandlerResult",
        execution_time_ms: int,
    ) -> None:
        """Submit result via FFI and publish completion event."""
        if handler_result.success:
            result = StepExecutionResult.success_result(
                event=event,
                output=handler_result.result or {},
                execution_time_ms=execution_time_ms,
                worker_id=self._worker_id,
                metadata=handler_result.metadata,
            )
        else:
            result = StepExecutionResult.failure_result(
                event=event,
                error=StepError(
                    error_type=handler_result.error_type or "unknown",
                    message=handler_result.error_message or "Unknown error",
                    retryable=handler_result.retryable,
                    metadata=handler_result.metadata,
                ),
                execution_time_ms=execution_time_ms,
                worker_id=self._worker_id,
            )

        # Submit via FFI
        try:
            complete_step_event(str(event.event_id), result.model_dump())
            self._event_bridge.publish("step.completion.sent", result)
        except Exception as e:
            logger.error(f"Failed to submit result: {e}")
            # Result submission failure is serious - log but don't retry
            raise

    @property
    def is_active(self) -> bool:
        return self._active
```

## Testing Strategy

### Unit Tests

```python
def test_event_bridge_publish_subscribe():
    """Test basic pub/sub functionality."""
    bridge = EventBridge()
    bridge.start()

    received = []
    bridge.subscribe("test.event", lambda x: received.append(x))
    bridge.publish("test.event", "data")

    assert received == ["data"]
    bridge.stop()

def test_handler_registry_register_resolve():
    """Test handler registration and resolution."""
    registry = HandlerRegistry()

    class TestHandler(StepHandler):
        handler_name = "test_handler"
        def call(self, context):
            return StepHandlerResult.success({})

    registry.register("test_handler", TestHandler)
    handler = registry.resolve("test_handler")

    assert handler is not None
    assert handler.name == "test_handler"

def test_step_handler_result_factory_methods():
    """Test StepHandlerResult factory methods."""
    success = StepHandlerResult.success({"key": "value"})
    assert success.success is True
    assert success.result == {"key": "value"}

    failure = StepHandlerResult.failure("error", retryable=False)
    assert failure.success is False
    assert failure.retryable is False
```

## Acceptance Criteria

- [ ] EventBridge provides working pub/sub
- [ ] HandlerRegistry can register and resolve handlers
- [ ] StepHandler base class enforces contract
- [ ] StepExecutionSubscriber routes events to handlers
- [ ] Results are submitted via FFI after execution
- [ ] Errors are properly caught and converted to results
- [ ] All unit tests pass
