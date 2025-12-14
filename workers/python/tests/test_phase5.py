"""Phase 5 tests for domain events and observability.

These tests verify TAS-84 (TAS-72-P5) implementation:
- In-process domain event polling
- InProcessDomainEventPoller class
- Health check endpoint
- Metrics endpoint
- Worker configuration endpoint
"""

from __future__ import annotations

import pytest

import tasker_core
from tasker_core import (
    ComponentHealth,
    DomainEventMetadata,
    HealthCheck,
    InProcessDomainEvent,
    InProcessDomainEventPoller,
    WorkerConfig,
    WorkerMetrics,
    WorkerNotInitializedError,
    get_health_check,
    get_metrics,
    get_worker_config,
    poll_in_process_events,
)


class TestPhase5Imports:
    """Test that Phase 5 symbols are properly exported."""

    def test_domain_event_imports(self):
        """Test domain event related imports."""
        assert hasattr(tasker_core, "poll_in_process_events")
        assert hasattr(tasker_core, "InProcessDomainEventPoller")
        assert hasattr(tasker_core, "DomainEventMetadata")
        assert hasattr(tasker_core, "InProcessDomainEvent")

    def test_observability_imports(self):
        """Test observability related imports."""
        assert hasattr(tasker_core, "get_health_check")
        assert hasattr(tasker_core, "get_metrics")
        assert hasattr(tasker_core, "get_worker_config")
        assert hasattr(tasker_core, "ComponentHealth")
        assert hasattr(tasker_core, "HealthCheck")
        assert hasattr(tasker_core, "WorkerMetrics")
        assert hasattr(tasker_core, "WorkerConfig")


class TestDomainEventModels:
    """Test domain event Pydantic models."""

    def test_domain_event_metadata(self):
        """Test DomainEventMetadata model."""
        from uuid import uuid4

        metadata = DomainEventMetadata(
            task_uuid=uuid4(),
            step_uuid=uuid4(),
            step_name="test_step",
            namespace="test",
            correlation_id=uuid4(),
            fired_at="2025-01-01T00:00:00Z",
            fired_by="test-worker",
        )

        assert metadata.step_name == "test_step"
        assert metadata.namespace == "test"
        assert metadata.fired_by == "test-worker"

    def test_domain_event_metadata_optional_fields(self):
        """Test DomainEventMetadata with optional fields."""
        from uuid import uuid4

        metadata = DomainEventMetadata(
            task_uuid=uuid4(),
            correlation_id=uuid4(),
            fired_at="2025-01-01T00:00:00Z",
            fired_by="test-worker",
        )

        assert metadata.step_uuid is None
        assert metadata.step_name is None
        assert metadata.namespace is None

    def test_in_process_domain_event(self):
        """Test InProcessDomainEvent model."""
        from uuid import uuid4

        metadata = DomainEventMetadata(
            task_uuid=uuid4(),
            correlation_id=uuid4(),
            fired_at="2025-01-01T00:00:00Z",
            fired_by="test-worker",
        )

        event = InProcessDomainEvent(
            event_id=uuid4(),
            event_name="step.completed",
            event_version="1.0",
            metadata=metadata,
            payload={"result": "success"},
        )

        assert event.event_name == "step.completed"
        assert event.event_version == "1.0"
        assert event.payload == {"result": "success"}


class TestObservabilityModels:
    """Test observability Pydantic models."""

    def test_component_health(self):
        """Test ComponentHealth model."""
        component = ComponentHealth(
            name="database",
            status="healthy",
            pool_size=10,
            pool_idle=5,
        )

        assert component.name == "database"
        assert component.status == "healthy"
        assert component.pool_size == 10
        assert component.pool_idle == 5

    def test_component_health_optional_fields(self):
        """Test ComponentHealth with optional fields."""
        component = ComponentHealth(
            name="custom",
            status="healthy",
        )

        assert component.pool_size is None
        assert component.pool_idle is None

    def test_health_check(self):
        """Test HealthCheck model."""
        db_component = ComponentHealth(
            name="database",
            status="healthy",
            pool_size=10,
        )

        health = HealthCheck(
            status="healthy",
            is_running=True,
            components={"database": db_component},
            rust={"environment": "test"},
            python={"handler_count": 5},
        )

        assert health.status == "healthy"
        assert health.is_running is True
        assert "database" in health.components
        assert health.rust["environment"] == "test"

    def test_worker_metrics(self):
        """Test WorkerMetrics model."""
        metrics = WorkerMetrics(
            dispatch_channel_pending=5,
            oldest_pending_age_ms=100,
            starvation_detected=False,
            database_pool_size=10,
            database_pool_idle=3,
            environment="test",
        )

        assert metrics.dispatch_channel_pending == 5
        assert metrics.oldest_pending_age_ms == 100
        assert metrics.starvation_detected is False
        assert metrics.database_pool_size == 10
        assert metrics.environment == "test"

    def test_worker_metrics_defaults(self):
        """Test WorkerMetrics with defaults."""
        metrics = WorkerMetrics()

        assert metrics.dispatch_channel_pending == 0
        assert metrics.oldest_pending_age_ms is None
        assert metrics.starvation_detected is False
        assert metrics.steps_processed == 0

    def test_worker_config(self):
        """Test WorkerConfig model."""
        config = WorkerConfig(
            environment="production",
            supported_namespaces=["payments", "orders"],
            web_api_enabled=True,
            database_pool_size=20,
            polling_interval_ms=10,
            starvation_threshold_ms=1000,
            max_concurrent_handlers=10,
            handler_timeout_ms=30000,
        )

        assert config.environment == "production"
        assert config.supported_namespaces == ["payments", "orders"]
        assert config.web_api_enabled is True
        assert config.polling_interval_ms == 10
        assert config.max_concurrent_handlers == 10

    def test_worker_config_defaults(self):
        """Test WorkerConfig with defaults."""
        config = WorkerConfig()

        assert config.environment is None
        assert config.supported_namespaces == []
        assert config.web_api_enabled is False
        assert config.polling_interval_ms == 10
        assert config.starvation_threshold_ms == 1000


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


class TestObservabilityFunctionsWithoutWorker:
    """Test observability functions when worker is not running."""

    def test_poll_in_process_events_without_worker(self):
        """poll_in_process_events returns None or raises when worker not running."""
        # This should either return None or raise
        try:
            result = poll_in_process_events()
            # If it doesn't raise, result should be None
            assert result is None
        except RuntimeError:
            # Expected when worker not initialized
            pass

    def test_get_health_check_without_worker(self):
        """get_health_check raises when worker not running."""
        with pytest.raises(WorkerNotInitializedError):
            get_health_check()

    def test_get_metrics_without_worker(self):
        """get_metrics raises when worker not running."""
        with pytest.raises(WorkerNotInitializedError):
            get_metrics()

    def test_get_worker_config_without_worker(self):
        """get_worker_config raises when worker not running."""
        with pytest.raises(WorkerNotInitializedError):
            get_worker_config()


class TestModelSerialization:
    """Test model serialization and deserialization."""

    def test_health_check_from_dict(self):
        """Test HealthCheck can be created from dict."""
        data = {
            "status": "healthy",
            "is_running": True,
            "components": {},
            "rust": {"environment": "test"},
            "python": {},
        }

        health = HealthCheck.model_validate(data)
        assert health.status == "healthy"
        assert health.is_running is True

    def test_worker_metrics_from_dict(self):
        """Test WorkerMetrics can be created from dict."""
        data = {
            "dispatch_channel_pending": 10,
            "database_pool_size": 5,
            "environment": "test",
        }

        metrics = WorkerMetrics.model_validate(data)
        assert metrics.dispatch_channel_pending == 10
        assert metrics.database_pool_size == 5

    def test_worker_config_from_dict(self):
        """Test WorkerConfig can be created from dict."""
        data = {
            "environment": "production",
            "supported_namespaces": ["default"],
            "web_api_enabled": True,
            "database_pool_size": 10,
            "polling_interval_ms": 20,
        }

        config = WorkerConfig.model_validate(data)
        assert config.environment == "production"
        assert config.polling_interval_ms == 20

    def test_in_process_domain_event_from_dict(self):
        """Test InProcessDomainEvent can be created from dict."""
        from uuid import uuid4

        task_uuid = uuid4()
        correlation_id = uuid4()

        data = {
            "event_id": str(uuid4()),
            "event_name": "task.completed",
            "event_version": "1.0",
            "metadata": {
                "task_uuid": str(task_uuid),
                "correlation_id": str(correlation_id),
                "fired_at": "2025-01-01T00:00:00Z",
                "fired_by": "test-worker",
            },
            "payload": {"result": "success"},
        }

        event = InProcessDomainEvent.model_validate(data)
        assert event.event_name == "task.completed"
        assert event.payload == {"result": "success"}
        assert event.metadata.task_uuid == task_uuid
