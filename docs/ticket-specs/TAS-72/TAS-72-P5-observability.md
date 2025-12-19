# TAS-72-P5: Domain Events & Observability

## Overview

This phase implements domain event publishing, in-process event polling (fast path), and comprehensive observability features including health checks, metrics, and configuration access.

## Prerequisites

- TAS-72-P1 through P4 complete
- Full step execution lifecycle working

## Objectives

1. Implement domain event publishing via FFI
2. Implement in-process event polling (fast path)
3. Create health check endpoints
4. Create metrics endpoints
5. Provide configuration access

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    Domain Event System                           │
│                                                                  │
│  ┌──────────────────┐          ┌──────────────────┐             │
│  │   Durable Path   │          │   Fast Path      │             │
│  │   (PGMQ-based)   │          │   (In-process)   │             │
│  │                  │          │                  │             │
│  │  publish_domain_ │          │  poll_in_process_│             │
│  │  event()         │          │  events()        │             │
│  └──────────────────┘          └──────────────────┘             │
│           │                            │                         │
│           ▼                            ▼                         │
│  ┌──────────────────┐          ┌──────────────────┐             │
│  │ PGMQ Queue       │          │ Broadcast Channel │             │
│  │ (Guaranteed)     │          │ (Best effort)     │             │
│  └──────────────────┘          └──────────────────┘             │
└─────────────────────────────────────────────────────────────────┘
```

## FFI Functions

### Domain Event Publishing

```rust
/// Publish a domain event through the durable path (PGMQ)
///
/// This path guarantees delivery for critical domain events.
#[pyfunction]
fn publish_domain_event(
    py: Python<'_>,
    event_type: &str,
    payload: &PyAny,
) -> PyResult<bool> {
    let system = get_worker_system()?;
    let payload_json: serde_json::Value = depythonize(payload)?;

    let event = DomainEvent {
        event_type: event_type.to_string(),
        payload: payload_json,
        timestamp: Utc::now(),
        correlation_id: None,
    };

    system.runtime.block_on(async {
        system
            .domain_event_publisher
            .publish(event)
            .await
            .map_err(|e| FFIError::new_err(e.to_string()))
    })?;

    Ok(true)
}
```

### In-Process Event Polling (Fast Path)

```rust
/// Poll for in-process domain events (fast path)
///
/// This is used for real-time notifications that don't require
/// guaranteed delivery (e.g., Slack notifications, metrics).
#[pyfunction]
fn poll_in_process_events(py: Python<'_>) -> PyResult<Option<PyObject>> {
    let system = get_worker_system()?;

    match &system.in_process_event_receiver {
        Some(receiver) => {
            match receiver.try_recv() {
                Ok(event) => {
                    let dict = pythonize(py, &event)?;
                    Ok(Some(dict.into()))
                }
                Err(_) => Ok(None),
            }
        }
        None => Ok(None),
    }
}
```

### Health Check

```rust
/// Get comprehensive health check status
#[pyfunction]
fn get_health_check(py: Python<'_>) -> PyResult<PyObject> {
    let system = get_worker_system()?;

    let health = system.runtime.block_on(async {
        system.system_handle.health_check().await
    }).map_err(|e| FFIError::new_err(e.to_string()))?;

    Ok(pythonize(py, &health)?.into())
}
```

### Metrics

```rust
/// Get worker metrics
#[pyfunction]
fn get_metrics(py: Python<'_>) -> PyResult<PyObject> {
    let system = get_worker_system()?;

    let metrics = system.runtime.block_on(async {
        system.system_handle.get_metrics().await
    }).map_err(|e| FFIError::new_err(e.to_string()))?;

    Ok(pythonize(py, &metrics)?.into())
}
```

### Configuration Access

```rust
/// Get current worker configuration
#[pyfunction]
fn get_config(py: Python<'_>) -> PyResult<PyObject> {
    let system = get_worker_system()?;
    let config = system.system_handle.config();
    Ok(pythonize(py, &config)?.into())
}
```

## Pydantic Models

### Domain Events

```python
from pydantic import BaseModel, Field
from typing import Any, Optional
from datetime import datetime
from uuid import UUID

class DomainEvent(BaseModel):
    """A domain event to be published."""
    event_type: str
    payload: dict[str, Any]
    correlation_id: Optional[UUID] = None
    timestamp: datetime = Field(default_factory=datetime.utcnow)

class InProcessDomainEvent(BaseModel):
    """An in-process domain event (fast path)."""
    event_id: UUID
    event_type: str
    payload: dict[str, Any]
    correlation_id: Optional[UUID] = None
    timestamp: datetime
    source: str
```

### Health Check

```python
from enum import Enum

class HealthStatus(str, Enum):
    """Health check status values."""
    HEALTHY = "healthy"
    DEGRADED = "degraded"
    UNHEALTHY = "unhealthy"

class ComponentHealth(BaseModel):
    """Health status of a single component."""
    name: str
    status: HealthStatus
    message: Optional[str] = None
    last_check: datetime

class HealthCheck(BaseModel):
    """Comprehensive health check result."""
    status: HealthStatus
    worker_id: str
    uptime_seconds: float
    components: dict[str, ComponentHealth]

    # Rust layer health
    rust: dict[str, Any]

    # Python layer health
    python: dict[str, Any]

    @property
    def is_healthy(self) -> bool:
        return self.status == HealthStatus.HEALTHY
```

### Metrics

```python
class WorkerMetrics(BaseModel):
    """Worker performance metrics."""
    worker_id: str
    uptime_seconds: float

    # Step execution metrics
    steps_processed: int
    steps_succeeded: int
    steps_failed: int
    steps_in_progress: int

    # Timing metrics
    avg_execution_time_ms: float
    max_execution_time_ms: float
    p95_execution_time_ms: float
    p99_execution_time_ms: float

    # Channel metrics
    dispatch_channel_pending: int
    completion_channel_pending: int
    dispatch_channel_saturation: float
    completion_channel_saturation: float

    # Domain event metrics
    domain_events_published: int
    domain_events_dropped: int

    # Error metrics
    handler_errors: int
    timeout_errors: int
    ffi_errors: int
```

### Configuration

```python
class WorkerConfig(BaseModel):
    """Worker configuration exposed from Rust."""
    worker_id: str
    namespace: str
    environment: str

    # Polling settings
    polling_interval_ms: int
    starvation_threshold_ms: int

    # Handler settings
    max_concurrent_handlers: int
    handler_timeout_ms: int

    # Channel settings
    dispatch_buffer_size: int
    completion_buffer_size: int
```

## Python Observability Layer

### DomainEventPublisher

```python
from typing import Any, Optional
from uuid import UUID
import logging

from tasker_core._tasker_core import publish_domain_event as _publish
from tasker_core.types import DomainEvent

logger = logging.getLogger(__name__)

class DomainEventPublisher:
    """Publisher for domain events.

    Provides a Python-friendly interface for publishing domain
    events through the durable (PGMQ) path.

    Example:
        publisher = DomainEventPublisher()
        publisher.publish(
            "order.created",
            {"order_id": "123", "amount": 100.00}
        )
    """

    def __init__(self, correlation_id: Optional[UUID] = None) -> None:
        self._default_correlation_id = correlation_id

    def publish(
        self,
        event_type: str,
        payload: dict[str, Any],
        correlation_id: Optional[UUID] = None,
    ) -> bool:
        """Publish a domain event.

        Args:
            event_type: Event type identifier
            payload: Event payload data
            correlation_id: Optional correlation ID

        Returns:
            True if published successfully
        """
        event = DomainEvent(
            event_type=event_type,
            payload=payload,
            correlation_id=correlation_id or self._default_correlation_id,
        )

        try:
            return _publish(event.event_type, event.model_dump())
        except Exception as e:
            logger.error(f"Failed to publish domain event: {e}")
            raise
```

### InProcessDomainEventPoller

```python
import threading
import time
from typing import Optional
from pyee import EventEmitter

from tasker_core._tasker_core import poll_in_process_events
from tasker_core.types import InProcessDomainEvent

logger = logging.getLogger(__name__)

class InProcessDomainEventPoller:
    """Poller for in-process domain events (fast path).

    Runs in a separate thread and emits events to subscribers
    for real-time notifications.

    Example:
        poller = InProcessDomainEventPoller(event_bridge)
        poller.start()

        @event_bridge.subscribe("domain.event.received")
        def on_domain_event(event):
            print(f"Received: {event.event_type}")
    """

    def __init__(
        self,
        event_emitter: EventEmitter,
        polling_interval_ms: int = 50,
    ) -> None:
        self._emitter = event_emitter
        self._polling_interval = polling_interval_ms / 1000.0
        self._running = False
        self._thread: Optional[threading.Thread] = None

    def start(self) -> None:
        """Start the domain event poller."""
        if self._running:
            raise RuntimeError("InProcessDomainEventPoller already running")

        self._running = True
        self._thread = threading.Thread(
            target=self._poll_loop,
            name="tasker-domain-event-poller",
            daemon=True,
        )
        self._thread.start()
        logger.info("InProcessDomainEventPoller started")

    def stop(self, timeout: float = 5.0) -> None:
        """Stop the domain event poller."""
        self._running = False
        if self._thread:
            self._thread.join(timeout=timeout)
            self._thread = None
        logger.info("InProcessDomainEventPoller stopped")

    def _poll_loop(self) -> None:
        """Main polling loop."""
        while self._running:
            try:
                event_data = poll_in_process_events()
                if event_data is not None:
                    event = InProcessDomainEvent.model_validate(event_data)
                    self._emitter.emit("domain.event.received", event)
                else:
                    time.sleep(self._polling_interval)
            except Exception as e:
                logger.error(f"Domain event polling error: {e}")
                time.sleep(self._polling_interval)

    @property
    def is_running(self) -> bool:
        return self._running
```

### Observability Module

```python
from tasker_core._tasker_core import (
    get_health_check as _get_health,
    get_metrics as _get_metrics,
    get_config as _get_config,
)
from tasker_core.types import HealthCheck, WorkerMetrics, WorkerConfig

def get_health_check() -> HealthCheck:
    """Get comprehensive health check status.

    Returns:
        HealthCheck with status of all components
    """
    data = _get_health()
    return HealthCheck.model_validate(data)

def get_metrics() -> WorkerMetrics:
    """Get worker performance metrics.

    Returns:
        WorkerMetrics with current statistics
    """
    data = _get_metrics()
    return WorkerMetrics.model_validate(data)

def get_config() -> WorkerConfig:
    """Get current worker configuration.

    Returns:
        WorkerConfig with runtime settings
    """
    data = _get_config()
    return WorkerConfig.model_validate(data)

def is_healthy() -> bool:
    """Quick health check.

    Returns:
        True if worker is healthy
    """
    return get_health_check().is_healthy
```

## Testing Strategy

### Unit Tests

```python
def test_domain_event_publisher():
    """Test domain event publishing."""
    # This requires a running worker
    pass

def test_health_check_model():
    """Test health check model validation."""
    data = {
        "status": "healthy",
        "worker_id": "test-worker",
        "uptime_seconds": 100.0,
        "components": {},
        "rust": {},
        "python": {},
    }
    health = HealthCheck.model_validate(data)
    assert health.is_healthy

def test_metrics_model():
    """Test metrics model validation."""
    data = {
        "worker_id": "test-worker",
        "uptime_seconds": 100.0,
        "steps_processed": 50,
        "steps_succeeded": 45,
        "steps_failed": 5,
        "steps_in_progress": 0,
        "avg_execution_time_ms": 50.0,
        "max_execution_time_ms": 200.0,
        "p95_execution_time_ms": 100.0,
        "p99_execution_time_ms": 150.0,
        "dispatch_channel_pending": 0,
        "completion_channel_pending": 0,
        "dispatch_channel_saturation": 0.0,
        "completion_channel_saturation": 0.0,
        "domain_events_published": 10,
        "domain_events_dropped": 0,
        "handler_errors": 3,
        "timeout_errors": 2,
        "ffi_errors": 0,
    }
    metrics = WorkerMetrics.model_validate(data)
    assert metrics.steps_processed == 50
```

## Acceptance Criteria

- [ ] `publish_domain_event()` publishes to durable path
- [ ] `poll_in_process_events()` receives fast-path events
- [ ] `get_health_check()` returns comprehensive status
- [ ] `get_metrics()` returns current worker metrics
- [ ] `get_config()` returns runtime configuration
- [ ] InProcessDomainEventPoller works in separate thread
- [ ] All Pydantic models validate correctly
- [ ] All unit tests pass
