# TAS-72-P6: Testing & Documentation

## Overview

This phase focuses on comprehensive testing of the Python worker, including unit tests, integration tests with database, example handlers for various workflow patterns, and complete documentation.

## Prerequisites

- TAS-72-P1 through P5 complete
- Full worker functionality implemented

## Objectives

1. Achieve >80% test coverage on Python code
2. Create integration tests with database
3. Implement example handlers for workflow patterns
4. Write comprehensive documentation
5. Generate type stubs if needed

## Test Structure

```
tests/
├── conftest.py                    # Global fixtures
├── unit/                          # Unit tests (no DB)
│   ├── test_types.py              # Pydantic model tests
│   ├── test_event_bridge.py       # EventBridge tests
│   ├── test_handler_registry.py   # HandlerRegistry tests
│   ├── test_step_handler.py       # StepHandler tests
│   └── test_observability.py      # Observability tests
├── ffi/                           # FFI function tests
│   ├── test_bootstrap.py          # Bootstrap/shutdown
│   ├── test_event_dispatch.py     # Event polling/completion
│   ├── test_domain_events.py      # Domain event publishing
│   └── test_observability_ffi.py  # Health/metrics FFI
├── integration/                   # Integration tests (require DB)
│   ├── conftest.py                # DB fixtures
│   ├── test_linear_workflow.py    # Linear workflow
│   ├── test_diamond_workflow.py   # Diamond dependency
│   ├── test_dag_workflow.py       # Complex DAG
│   └── test_error_handling.py     # Error scenarios
└── handlers/                      # Test handler examples
    ├── __init__.py
    ├── examples/
    │   ├── __init__.py
    │   ├── linear_handlers.py
    │   ├── diamond_handlers.py
    │   └── dag_handlers.py
    └── fixtures/
        └── templates/
            ├── linear_workflow.yaml
            ├── diamond_workflow.yaml
            └── dag_workflow.yaml
```

## Test Fixtures

### conftest.py (Global)

```python
import pytest
from typing import Generator

@pytest.fixture(scope="session")
def worker_module():
    """Provide the tasker_core module."""
    import tasker_core
    return tasker_core

@pytest.fixture
def event_bridge():
    """Provide a fresh EventBridge for each test."""
    from tasker_core.event_bridge import EventBridge
    bridge = EventBridge()
    bridge.start()
    yield bridge
    bridge.stop()
    EventBridge.reset_instance()

@pytest.fixture
def handler_registry():
    """Provide a fresh HandlerRegistry for each test."""
    from tasker_core.handler_registry import HandlerRegistry
    registry = HandlerRegistry()
    yield registry
    HandlerRegistry.reset_instance()
```

### conftest.py (Integration)

```python
import pytest
import os
from typing import Generator

@pytest.fixture(scope="session")
def database_url() -> str:
    """Get database URL from environment."""
    url = os.environ.get("DATABASE_URL")
    if not url:
        pytest.skip("DATABASE_URL not set")
    return url

@pytest.fixture(scope="function")
def worker_system(database_url: str) -> Generator:
    """Bootstrap worker for integration tests."""
    from tasker_core import bootstrap_worker, stop_worker
    from tasker_core.types import BootstrapConfig

    config = BootstrapConfig(
        namespace="test",
        worker_id=f"test-worker-{os.getpid()}",
    )

    result = bootstrap_worker(config)
    assert result.success

    yield result

    stop_worker()
```

## Unit Test Examples

### test_types.py

```python
import pytest
from uuid import uuid4
from datetime import datetime, timezone

from tasker_core.types import (
    FfiStepEvent,
    StepExecutionEvent,
    StepExecutionResult,
    StepHandlerResult,
    StepError,
    ResultStatus,
)

class TestStepExecutionEvent:
    def test_minimal_event(self):
        """Test event with minimal required fields."""
        event = StepExecutionEvent(
            task_uuid=uuid4(),
            step_uuid=uuid4(),
            task_name="test_task",
            step_name="test_step",
            handler_name="test_handler",
            handler_version="1.0.0",
            created_at=datetime.now(timezone.utc),
        )
        assert event.retry_count == 0
        assert event.max_retries == 3
        assert event.input_data == {}

    def test_full_event(self):
        """Test event with all fields populated."""
        event = StepExecutionEvent(
            task_uuid=uuid4(),
            step_uuid=uuid4(),
            task_name="test_task",
            step_name="test_step",
            handler_name="test_handler",
            handler_version="1.0.0",
            input_data={"key": "value"},
            dependency_results={"dep1": {"result": "ok"}},
            step_config={"timeout": 30},
            retry_count=2,
            max_retries=5,
            created_at=datetime.now(timezone.utc),
        )
        assert event.input_data == {"key": "value"}
        assert event.retry_count == 2

class TestStepHandlerResult:
    def test_success_factory(self):
        """Test success result factory."""
        result = StepHandlerResult.success(
            {"output": "value"},
            metadata={"timing": 100},
        )
        assert result.success is True
        assert result.result == {"output": "value"}
        assert result.metadata == {"timing": 100}

    def test_failure_factory(self):
        """Test failure result factory."""
        result = StepHandlerResult.failure(
            "Something went wrong",
            error_type="validation_error",
            retryable=False,
        )
        assert result.success is False
        assert result.error_message == "Something went wrong"
        assert result.error_type == "validation_error"
        assert result.retryable is False

class TestStepExecutionResult:
    def test_success_result(self):
        """Test creating a success execution result."""
        event = FfiStepEvent(
            event_id=uuid4(),
            task_uuid=uuid4(),
            step_uuid=uuid4(),
            correlation_id=uuid4(),
            execution_event=StepExecutionEvent(
                task_uuid=uuid4(),
                step_uuid=uuid4(),
                task_name="test",
                step_name="step",
                handler_name="handler",
                handler_version="1.0",
                created_at=datetime.now(timezone.utc),
            ),
        )

        result = StepExecutionResult.success_result(
            event=event,
            output={"data": "result"},
            execution_time_ms=150,
            worker_id="test-worker",
        )

        assert result.success is True
        assert result.status == ResultStatus.SUCCESS
        assert result.execution_time_ms == 150
```

### test_event_bridge.py

```python
import pytest

from tasker_core.event_bridge import EventBridge

class TestEventBridge:
    def test_start_stop(self, event_bridge):
        """Test starting and stopping the bridge."""
        assert event_bridge.is_active is True
        event_bridge.stop()
        assert event_bridge.is_active is False

    def test_subscribe_publish(self, event_bridge):
        """Test basic pub/sub."""
        received = []

        event_bridge.subscribe("test.event", lambda x: received.append(x))
        event_bridge.publish("test.event", "payload")

        assert received == ["payload"]

    def test_multiple_subscribers(self, event_bridge):
        """Test multiple subscribers receive events."""
        results = []

        event_bridge.subscribe("test.event", lambda x: results.append(f"a:{x}"))
        event_bridge.subscribe("test.event", lambda x: results.append(f"b:{x}"))
        event_bridge.publish("test.event", "data")

        assert "a:data" in results
        assert "b:data" in results

    def test_unsubscribe(self, event_bridge):
        """Test unsubscribing removes handler."""
        received = []
        handler = lambda x: received.append(x)

        event_bridge.subscribe("test.event", handler)
        event_bridge.publish("test.event", "first")
        event_bridge.unsubscribe("test.event", handler)
        event_bridge.publish("test.event", "second")

        assert received == ["first"]

    def test_inactive_drops_events(self, event_bridge):
        """Test that inactive bridge drops events."""
        received = []
        event_bridge.subscribe("test.event", lambda x: received.append(x))

        event_bridge.stop()
        event_bridge.publish("test.event", "dropped")

        assert received == []
```

### test_handler_registry.py

```python
import pytest

from tasker_core.handler_registry import HandlerRegistry
from tasker_core.step_handler import StepHandler, StepHandlerResult, StepContext

class MockHandler(StepHandler):
    handler_name = "mock_handler"

    def call(self, context: StepContext) -> StepHandlerResult:
        return StepHandlerResult.success({"mock": True})

class TestHandlerRegistry:
    def test_register_and_resolve(self, handler_registry):
        """Test basic registration and resolution."""
        handler_registry.register("mock_handler", MockHandler)
        handler = handler_registry.resolve("mock_handler")

        assert handler is not None
        assert handler.name == "mock_handler"

    def test_resolve_unknown_returns_none(self, handler_registry):
        """Test resolving unknown handler returns None."""
        handler = handler_registry.resolve("unknown_handler")
        assert handler is None

    def test_is_registered(self, handler_registry):
        """Test checking if handler is registered."""
        handler_registry.register("mock_handler", MockHandler)

        assert handler_registry.is_registered("mock_handler") is True
        assert handler_registry.is_registered("unknown") is False

    def test_list_handlers(self, handler_registry):
        """Test listing registered handlers."""
        handler_registry.register("handler_a", MockHandler)
        handler_registry.register("handler_b", MockHandler)

        handlers = handler_registry.list_handlers()
        assert "handler_a" in handlers
        assert "handler_b" in handlers

    def test_unregister(self, handler_registry):
        """Test unregistering a handler."""
        handler_registry.register("mock_handler", MockHandler)
        assert handler_registry.is_registered("mock_handler")

        result = handler_registry.unregister("mock_handler")
        assert result is True
        assert not handler_registry.is_registered("mock_handler")
```

## Example Handlers

### linear_handlers.py

```python
from tasker_core.step_handler import StepHandler, StepHandlerResult, StepContext

class FetchDataHandler(StepHandler):
    """Handler that fetches data (step 1 of linear workflow)."""
    handler_name = "fetch_data"

    def call(self, context: StepContext) -> StepHandlerResult:
        # Simulate fetching data
        data = {
            "source": context.input_data.get("source", "default"),
            "items": [{"id": 1}, {"id": 2}, {"id": 3}],
        }
        return StepHandlerResult.success(data)

class TransformDataHandler(StepHandler):
    """Handler that transforms data (step 2 of linear workflow)."""
    handler_name = "transform_data"

    def call(self, context: StepContext) -> StepHandlerResult:
        # Get result from previous step
        prev_result = context.dependency_results.get("fetch_data", {})
        items = prev_result.get("items", [])

        # Transform items
        transformed = [{"id": item["id"], "processed": True} for item in items]
        return StepHandlerResult.success({"transformed_items": transformed})

class StoreDataHandler(StepHandler):
    """Handler that stores data (step 3 of linear workflow)."""
    handler_name = "store_data"

    def call(self, context: StepContext) -> StepHandlerResult:
        # Get transformed data
        prev_result = context.dependency_results.get("transform_data", {})
        items = prev_result.get("transformed_items", [])

        # Simulate storing
        stored_ids = [item["id"] for item in items]
        return StepHandlerResult.success({
            "stored_count": len(stored_ids),
            "stored_ids": stored_ids,
        })
```

### diamond_handlers.py

```python
from tasker_core.step_handler import StepHandler, StepHandlerResult, StepContext

class InitHandler(StepHandler):
    """Initial handler (diamond top)."""
    handler_name = "diamond_init"

    def call(self, context: StepContext) -> StepHandlerResult:
        return StepHandlerResult.success({"initialized": True, "value": 100})

class PathAHandler(StepHandler):
    """Path A handler (diamond left branch)."""
    handler_name = "diamond_path_a"

    def call(self, context: StepContext) -> StepHandlerResult:
        init_result = context.dependency_results.get("diamond_init", {})
        value = init_result.get("value", 0)
        return StepHandlerResult.success({"path_a_result": value * 2})

class PathBHandler(StepHandler):
    """Path B handler (diamond right branch)."""
    handler_name = "diamond_path_b"

    def call(self, context: StepContext) -> StepHandlerResult:
        init_result = context.dependency_results.get("diamond_init", {})
        value = init_result.get("value", 0)
        return StepHandlerResult.success({"path_b_result": value + 50})

class MergeHandler(StepHandler):
    """Merge handler (diamond bottom)."""
    handler_name = "diamond_merge"

    def call(self, context: StepContext) -> StepHandlerResult:
        path_a = context.dependency_results.get("diamond_path_a", {})
        path_b = context.dependency_results.get("diamond_path_b", {})

        a_result = path_a.get("path_a_result", 0)
        b_result = path_b.get("path_b_result", 0)

        return StepHandlerResult.success({
            "merged_result": a_result + b_result,
            "path_a_value": a_result,
            "path_b_value": b_result,
        })
```

## Integration Test Example

### test_linear_workflow.py

```python
import pytest
import time

from tasker_core import bootstrap_worker, stop_worker, get_health_check
from tasker_core.bootstrap import Bootstrap
from tasker_core.types import BootstrapConfig

@pytest.mark.integration
class TestLinearWorkflow:
    def test_full_linear_workflow(self, worker_system, database_url):
        """Test complete linear workflow execution."""
        # Register handlers
        from tests.handlers.examples.linear_handlers import (
            FetchDataHandler,
            TransformDataHandler,
            StoreDataHandler,
        )

        from tasker_core.handler_registry import HandlerRegistry
        registry = HandlerRegistry.instance()
        registry.register("fetch_data", FetchDataHandler)
        registry.register("transform_data", TransformDataHandler)
        registry.register("store_data", StoreDataHandler)

        # Start event processing
        bootstrap = Bootstrap.instance()
        bootstrap.start_event_processing()

        # Wait for workflow completion (via orchestration)
        # This would be triggered by creating a task in the database

        # Verify health
        health = get_health_check()
        assert health.is_healthy

        bootstrap.stop()
```

## Documentation Structure

```
docs/
├── README.md                      # Package overview
├── getting-started.md             # Quick start guide
├── installation.md                # Installation instructions
├── configuration.md               # Configuration reference
├── api/                           # API reference
│   ├── bootstrap.md
│   ├── step-handler.md
│   ├── event-bridge.md
│   ├── observability.md
│   └── types.md
├── guides/                        # How-to guides
│   ├── creating-handlers.md
│   ├── error-handling.md
│   ├── testing-handlers.md
│   └── deployment.md
└── examples/                      # Example code
    ├── linear-workflow.md
    ├── diamond-workflow.md
    └── dag-workflow.md
```

## Coverage Configuration

### pyproject.toml

```toml
[tool.coverage.run]
source = ["python/tasker_core"]
omit = [
    "python/tasker_core/_tasker_core.pyi",
    "tests/*",
]
branch = true

[tool.coverage.report]
exclude_lines = [
    "pragma: no cover",
    "def __repr__",
    "raise NotImplementedError",
    "if TYPE_CHECKING:",
]
fail_under = 80
show_missing = true
```

## Acceptance Criteria

- [ ] >80% test coverage on Python code
- [ ] All unit tests pass
- [ ] Integration tests pass with database
- [ ] Example handlers demonstrate workflow patterns
- [ ] API documentation complete
- [ ] Getting started guide written
- [ ] Type stubs generated (if needed)
