# TAS-72-P3: Event Dispatch System

## Overview

This phase implements the step event polling and completion system, enabling Python handlers to receive step execution events from the Rust orchestration layer and submit completion results.

## Prerequisites

- TAS-72-P1 complete (crate foundation)
- TAS-72-P2 complete (FFI bridge core)
- Worker can be bootstrapped and stopped

## Objectives

1. Implement `poll_step_events()` FFI function
2. Implement `complete_step_event()` FFI function
3. Create Python `EventPoller` class with threaded polling
4. Define Pydantic models for step events and results
5. Handle starvation detection and timeout cleanup

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    FfiDispatchChannel (Rust)                 │
│  - pending_events: HashMap<Uuid, PendingEvent>               │
│  - dispatch_receiver: mpsc::Receiver<DispatchMessage>        │
│  - completion_sender: mpsc::Sender<StepExecutionResult>      │
└─────────────────────────────────────────────────────────────┘
                              │
        ┌─────────────────────┴─────────────────────┐
        ▼                                           ▼
┌───────────────────┐                    ┌───────────────────┐
│ poll_step_events()│                    │complete_step_event│
│ Non-blocking poll │                    │ Submit results    │
│ Returns FfiEvent  │                    │ Fires callbacks   │
│ or None           │                    │                   │
└───────────────────┘                    └───────────────────┘
        │                                           ▲
        ▼                                           │
┌───────────────────────────────────────────────────────────────┐
│                    Python EventPoller                          │
│  - Runs in separate thread (10ms polling interval)            │
│  - Emits events to EventBridge                                │
│  - Handles timeouts and cleanup                               │
└───────────────────────────────────────────────────────────────┘
```

## FFI Functions

### poll_step_events

```rust
/// Poll for pending step execution events
///
/// This is a non-blocking call that returns the next available
/// step event or None if no events are pending.
#[pyfunction]
fn poll_step_events(py: Python<'_>) -> PyResult<Option<PyObject>> {
    let system = get_worker_system()?;

    match system.ffi_dispatch_channel.poll() {
        Some(event) => {
            let ffi_event = FfiStepEvent::from_dispatch_message(&event);
            let dict = pythonize(py, &ffi_event)?;
            Ok(Some(dict.into()))
        }
        None => Ok(None),
    }
}
```

### complete_step_event

```rust
/// Submit completion result for a step event
///
/// This function sends the result back to the completion channel
/// and triggers post-handler callbacks.
#[pyfunction]
fn complete_step_event(
    py: Python<'_>,
    event_id: &str,
    result: &PyAny,
) -> PyResult<bool> {
    let system = get_worker_system()?;
    let event_uuid = Uuid::parse_str(event_id)
        .map_err(|e| PyValueError::new_err(format!("Invalid event_id: {}", e)))?;

    let step_result: StepExecutionResult = depythonize(result)?;

    match system.ffi_dispatch_channel.complete(event_uuid, step_result) {
        Ok(()) => Ok(true),
        Err(e) => Err(FFIError::new_err(e.to_string())),
    }
}
```

### get_ffi_dispatch_metrics

```rust
/// Get metrics from the FFI dispatch channel
#[pyfunction]
fn get_ffi_dispatch_metrics(py: Python<'_>) -> PyResult<PyObject> {
    let system = get_worker_system()?;
    let metrics = system.ffi_dispatch_channel.get_metrics();
    Ok(pythonize(py, &metrics)?.into())
}

/// Check for starvation warnings (events pending > threshold)
#[pyfunction]
fn check_starvation_warnings(py: Python<'_>) -> PyResult<PyObject> {
    let system = get_worker_system()?;
    let warnings = system.ffi_dispatch_channel.check_starvation_warnings();
    Ok(pythonize(py, &warnings)?.into())
}
```

## Pydantic Models

### FfiStepEvent

```python
from pydantic import BaseModel, Field
from typing import Optional, Any
from uuid import UUID
from datetime import datetime

class StepExecutionEvent(BaseModel):
    """The full step execution event payload."""
    task_uuid: UUID
    step_uuid: UUID
    task_name: str
    step_name: str
    handler_name: str
    handler_version: str
    input_data: dict[str, Any] = Field(default_factory=dict)
    dependency_results: dict[str, Any] = Field(default_factory=dict)
    step_config: dict[str, Any] = Field(default_factory=dict)
    retry_count: int = 0
    max_retries: int = 3
    created_at: datetime

class FfiStepEvent(BaseModel):
    """Event received from FFI poll_step_events()."""
    event_id: UUID
    task_uuid: UUID
    step_uuid: UUID
    correlation_id: UUID
    execution_event: StepExecutionEvent
    trace_id: Optional[str] = None
    span_id: Optional[str] = None

    class Config:
        frozen = True  # Events are immutable
```

### StepExecutionResult

```python
from enum import Enum

class ResultStatus(str, Enum):
    """Step execution result status."""
    SUCCESS = "success"
    FAILURE = "failure"
    RETRYABLE_ERROR = "retryable_error"
    PERMANENT_ERROR = "permanent_error"

class StepError(BaseModel):
    """Error details for failed step execution."""
    error_type: str
    message: str
    stack_trace: Optional[str] = None
    retryable: bool = True
    metadata: dict[str, Any] = Field(default_factory=dict)

class StepExecutionResult(BaseModel):
    """Result submitted via complete_step_event()."""
    step_uuid: UUID
    task_uuid: UUID
    event_id: UUID
    success: bool
    status: ResultStatus
    execution_time_ms: int
    output: Optional[dict[str, Any]] = None
    error: Optional[StepError] = None
    worker_id: str
    correlation_id: UUID
    metadata: dict[str, Any] = Field(default_factory=dict)

    @classmethod
    def success_result(
        cls,
        event: FfiStepEvent,
        output: dict[str, Any],
        execution_time_ms: int,
        worker_id: str,
        metadata: Optional[dict[str, Any]] = None,
    ) -> "StepExecutionResult":
        """Create a successful result."""
        return cls(
            step_uuid=event.step_uuid,
            task_uuid=event.task_uuid,
            event_id=event.event_id,
            success=True,
            status=ResultStatus.SUCCESS,
            execution_time_ms=execution_time_ms,
            output=output,
            worker_id=worker_id,
            correlation_id=event.correlation_id,
            metadata=metadata or {},
        )

    @classmethod
    def failure_result(
        cls,
        event: FfiStepEvent,
        error: StepError,
        execution_time_ms: int,
        worker_id: str,
        metadata: Optional[dict[str, Any]] = None,
    ) -> "StepExecutionResult":
        """Create a failure result."""
        return cls(
            step_uuid=event.step_uuid,
            task_uuid=event.task_uuid,
            event_id=event.event_id,
            success=False,
            status=ResultStatus.RETRYABLE_ERROR if error.retryable else ResultStatus.PERMANENT_ERROR,
            execution_time_ms=execution_time_ms,
            error=error,
            worker_id=worker_id,
            correlation_id=event.correlation_id,
            metadata=metadata or {},
        )
```

### Dispatch Metrics

```python
class FfiDispatchMetrics(BaseModel):
    """Metrics from the FFI dispatch channel."""
    pending_events: int
    oldest_event_age_ms: Optional[int] = None
    events_dispatched: int
    events_completed: int
    events_timed_out: int
    completion_channel_saturation: float

class StarvationWarning(BaseModel):
    """Warning for events that have been pending too long."""
    event_id: UUID
    step_uuid: UUID
    pending_duration_ms: int
    threshold_ms: int
```

## Python EventPoller

```python
import threading
import time
from typing import Callable, Optional
from pyee import EventEmitter

from tasker_core._tasker_core import (
    poll_step_events,
    complete_step_event,
    get_ffi_dispatch_metrics,
)
from tasker_core.types import FfiStepEvent, StepExecutionResult

class EventPoller:
    """Threaded event poller for step execution events.

    The EventPoller runs in a separate thread and polls for step events
    from the FFI dispatch channel. Events are emitted to registered
    handlers via the EventBridge.

    Example:
        poller = EventPoller(event_bridge, polling_interval_ms=10)
        poller.start()
        # ... events flow to handlers ...
        poller.stop()
    """

    def __init__(
        self,
        event_emitter: EventEmitter,
        polling_interval_ms: int = 10,
        starvation_check_interval_ms: int = 5000,
    ) -> None:
        self._emitter = event_emitter
        self._polling_interval = polling_interval_ms / 1000.0
        self._starvation_check_interval = starvation_check_interval_ms / 1000.0
        self._running = False
        self._thread: Optional[threading.Thread] = None
        self._last_starvation_check = time.time()

    def start(self) -> None:
        """Start the event polling thread."""
        if self._running:
            raise RuntimeError("EventPoller already running")

        self._running = True
        self._thread = threading.Thread(
            target=self._poll_loop,
            name="tasker-event-poller",
            daemon=True,
        )
        self._thread.start()

    def stop(self, timeout: float = 5.0) -> None:
        """Stop the event polling thread."""
        self._running = False
        if self._thread:
            self._thread.join(timeout=timeout)
            self._thread = None

    def _poll_loop(self) -> None:
        """Main polling loop (runs in separate thread)."""
        while self._running:
            try:
                # Poll for next event
                event_data = poll_step_events()

                if event_data is not None:
                    event = FfiStepEvent.model_validate(event_data)
                    self._emitter.emit("step.execution.received", event)
                else:
                    # No event available, sleep before next poll
                    time.sleep(self._polling_interval)

                # Periodic starvation check
                if time.time() - self._last_starvation_check > self._starvation_check_interval:
                    self._check_starvation()
                    self._last_starvation_check = time.time()

            except Exception as e:
                self._emitter.emit("poller.error", e)
                time.sleep(self._polling_interval)

    def _check_starvation(self) -> None:
        """Check for starvation warnings and emit events."""
        try:
            metrics = get_ffi_dispatch_metrics()
            self._emitter.emit("poller.metrics", metrics)

            # Could also check for starvation warnings here
        except Exception as e:
            self._emitter.emit("poller.error", e)

    @property
    def is_running(self) -> bool:
        """Check if the poller is running."""
        return self._running and self._thread is not None and self._thread.is_alive()
```

## Testing Strategy

### Unit Tests

```python
import pytest
from unittest.mock import patch, MagicMock
from tasker_core.types import FfiStepEvent, StepExecutionResult

def test_poll_returns_none_when_empty():
    """Test that poll returns None when no events pending."""
    with patch("tasker_core._tasker_core.poll_step_events", return_value=None):
        from tasker_core._tasker_core import poll_step_events
        assert poll_step_events() is None

def test_poll_returns_event_data():
    """Test that poll returns event data when available."""
    mock_event = {
        "event_id": "123e4567-e89b-12d3-a456-426614174000",
        "task_uuid": "123e4567-e89b-12d3-a456-426614174001",
        "step_uuid": "123e4567-e89b-12d3-a456-426614174002",
        "correlation_id": "123e4567-e89b-12d3-a456-426614174003",
        "execution_event": {
            "task_uuid": "123e4567-e89b-12d3-a456-426614174001",
            "step_uuid": "123e4567-e89b-12d3-a456-426614174002",
            "task_name": "test_task",
            "step_name": "test_step",
            "handler_name": "test_handler",
            "handler_version": "1.0.0",
            "created_at": "2025-01-01T00:00:00Z",
        },
    }
    event = FfiStepEvent.model_validate(mock_event)
    assert event.step_uuid is not None

def test_complete_step_event_success():
    """Test successful completion submission."""
    # Test with mock FFI
    pass

def test_event_poller_starts_and_stops():
    """Test event poller lifecycle."""
    from pyee import EventEmitter
    emitter = EventEmitter()
    poller = EventPoller(emitter)

    poller.start()
    assert poller.is_running

    poller.stop()
    assert not poller.is_running
```

## Acceptance Criteria

- [ ] `poll_step_events()` returns events or None
- [ ] `complete_step_event()` submits results successfully
- [ ] `get_ffi_dispatch_metrics()` returns current metrics
- [ ] EventPoller runs in separate thread without blocking
- [ ] Events are validated with Pydantic models
- [ ] Starvation detection works and emits warnings
- [ ] All unit tests pass
