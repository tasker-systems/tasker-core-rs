"""pytest configuration and fixtures for tasker_core tests.

This module provides shared fixtures for testing the tasker_core Python worker,
including EventBridge, HandlerRegistry, and sample step events/contexts.
"""

from __future__ import annotations

from collections.abc import Generator
from typing import TYPE_CHECKING
from uuid import uuid4

import pytest

if TYPE_CHECKING:
    from tasker_core import EventBridge, FfiStepEvent, HandlerRegistry, StepContext


@pytest.fixture(scope="session")
def tasker_core_module():
    """Provide the tasker_core module as a fixture."""
    import tasker_core

    return tasker_core


@pytest.fixture
def event_bridge() -> Generator[EventBridge, None, None]:
    """Provide a fresh EventBridge for each test.

    The bridge is automatically started and cleaned up after the test.
    """
    from tasker_core import EventBridge

    EventBridge.reset_instance()
    bridge = EventBridge.instance()
    bridge.start()
    yield bridge
    bridge.stop()
    EventBridge.reset_instance()


@pytest.fixture
def handler_registry() -> Generator[HandlerRegistry, None, None]:
    """Provide a fresh HandlerRegistry for each test.

    The registry is automatically cleaned up after the test.
    """
    from tasker_core import HandlerRegistry

    HandlerRegistry.reset_instance()
    registry = HandlerRegistry.instance()
    yield registry
    registry.clear()
    HandlerRegistry.reset_instance()


@pytest.fixture
def sample_ffi_step_event() -> FfiStepEvent:
    """Provide a sample FfiStepEvent for testing.

    Returns a fully populated event suitable for handler testing.
    """
    from tasker_core import FfiStepEvent

    return FfiStepEvent(
        event_id=str(uuid4()),
        task_uuid=str(uuid4()),
        step_uuid=str(uuid4()),
        correlation_id=str(uuid4()),
        task_sequence_step={
            "name": "test_step",
            "handler_name": "test_handler",
            "input_data": {"key": "value", "count": 5},
            "dependency_results": {
                "previous_step": {"result": "success", "data": [1, 2, 3]}
            },
            "step_config": {"timeout": 30, "retry_delay": 1000},
            "retry_count": 0,
            "max_retries": 3,
        },
    )


@pytest.fixture
def sample_step_context(sample_ffi_step_event: FfiStepEvent) -> StepContext:
    """Provide a sample StepContext for testing.

    Returns a context created from the sample_ffi_step_event fixture.
    """
    from tasker_core import StepContext

    return StepContext.from_ffi_event(sample_ffi_step_event, "test_handler")


@pytest.fixture
def minimal_ffi_step_event() -> FfiStepEvent:
    """Provide a minimal FfiStepEvent with only required fields.

    Useful for testing default value handling.
    """
    from tasker_core import FfiStepEvent

    return FfiStepEvent(
        event_id=str(uuid4()),
        task_uuid=str(uuid4()),
        step_uuid=str(uuid4()),
        correlation_id=str(uuid4()),
        task_sequence_step={},
    )


@pytest.fixture
def event_uuids() -> dict[str, str]:
    """Provide a consistent set of UUIDs for testing.

    Returns a dict with task_uuid, step_uuid, correlation_id, and event_id.
    """
    return {
        "task_uuid": str(uuid4()),
        "step_uuid": str(uuid4()),
        "correlation_id": str(uuid4()),
        "event_id": str(uuid4()),
    }


# Markers for test categorization
def pytest_configure(config):
    """Configure custom pytest markers."""
    config.addinivalue_line(
        "markers",
        "integration: marks tests as integration tests (require DATABASE_URL)",
    )
    config.addinivalue_line(
        "markers",
        "slow: marks tests as slow running",
    )
