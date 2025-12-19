"""Tests for Pydantic models.

These tests verify all Pydantic models work correctly across:
- Bootstrap and worker configuration models
- Event dispatch models (step execution, FFI events)
- Domain event models
- Observability models (health checks, metrics)
- Batch processing models
"""

from __future__ import annotations

from uuid import uuid4

import pytest


class TestBootstrapModels:
    """Test bootstrap and worker configuration Pydantic models."""

    def test_bootstrap_config_defaults(self):
        """Test BootstrapConfig with default values."""
        from tasker_core import BootstrapConfig

        config = BootstrapConfig()
        assert config.worker_id is None
        assert config.namespace == "default"
        assert config.config_path is None
        assert config.log_level == "info"

    def test_bootstrap_config_custom_values(self):
        """Test BootstrapConfig with custom values."""
        from tasker_core import BootstrapConfig

        config = BootstrapConfig(
            worker_id="my-worker",
            namespace="payments",
            log_level="debug",
        )
        assert config.worker_id == "my-worker"
        assert config.namespace == "payments"
        assert config.log_level == "debug"

    def test_bootstrap_config_invalid_log_level(self):
        """Test BootstrapConfig rejects invalid log level."""
        from pydantic import ValidationError

        from tasker_core import BootstrapConfig

        with pytest.raises(ValidationError):
            BootstrapConfig(log_level="invalid")

    def test_bootstrap_config_extra_fields_forbidden(self):
        """Test BootstrapConfig rejects extra fields."""
        from pydantic import ValidationError

        from tasker_core import BootstrapConfig

        with pytest.raises(ValidationError):
            BootstrapConfig(unknown_field="value")

    def test_bootstrap_result_model(self):
        """Test BootstrapResult model."""
        from tasker_core import BootstrapResult

        result = BootstrapResult(
            success=True,
            handle_id="abc-123",
            worker_id="python-worker-abc",
            status="started",
            message="Worker started",
        )
        assert result.success is True
        assert result.handle_id == "abc-123"
        assert result.error_message is None

    def test_worker_status_model(self):
        """Test WorkerStatus model."""
        from tasker_core import WorkerStatus

        # Test with minimal data (not running)
        status = WorkerStatus(running=False, error="Not initialized")
        assert status.running is False
        assert status.error == "Not initialized"
        assert status.environment is None

        # Test with full data (running)
        status = WorkerStatus(
            running=True,
            environment="test",
            worker_core_status="Running",
            web_api_enabled=True,
            supported_namespaces=["default", "payments"],
            database_pool_size=10,
            database_pool_idle=5,
        )
        assert status.running is True
        assert status.database_pool_size == 10

    def test_worker_state_enum(self):
        """Test WorkerState enum values."""
        from tasker_core import WorkerState

        assert WorkerState.STARTING == "starting"
        assert WorkerState.RUNNING == "running"
        assert WorkerState.SHUTTING_DOWN == "shutting_down"
        assert WorkerState.STOPPED == "stopped"
        assert WorkerState.ERROR == "error"

    def test_step_handler_call_result(self):
        """Test StepHandlerCallResult model."""
        from tasker_core import StepHandlerCallResult

        result = StepHandlerCallResult(
            success=True,
            result={"processed": 100},
            metadata={"duration_ms": 150},
        )
        assert result.success is True
        assert result.result["processed"] == 100
        assert result.error_message is None

    def test_log_context_model(self):
        """Test LogContext model."""
        from tasker_core import LogContext

        context = LogContext(
            correlation_id="abc-123",
            task_uuid="task-456",
            operation="process",
        )
        assert context.correlation_id == "abc-123"
        assert context.step_uuid is None

        # Test model_dump excludes None values
        dump = context.model_dump()
        assert "correlation_id" in dump
        assert dump["step_uuid"] is None


class TestEventDispatchModels:
    """Test Pydantic models for event dispatch."""

    def test_result_status_enum(self):
        """Test ResultStatus enum values."""
        from tasker_core import ResultStatus

        assert ResultStatus.SUCCESS == "completed"
        assert ResultStatus.FAILURE == "error"
        assert ResultStatus.RETRYABLE_ERROR == "retryable_error"
        assert ResultStatus.PERMANENT_ERROR == "permanent_error"

    def test_step_error_model(self):
        """Test StepError model."""
        from tasker_core import StepError

        error = StepError(
            error_type="ValidationError",
            message="Invalid input data",
            retryable=False,
            metadata={"field": "email"},
        )
        assert error.error_type == "ValidationError"
        assert error.message == "Invalid input data"
        assert error.retryable is False
        assert error.metadata["field"] == "email"
        assert error.stack_trace is None

    def test_step_error_with_stack_trace(self):
        """Test StepError with stack trace."""
        from tasker_core import StepError

        error = StepError(
            error_type="RuntimeError",
            message="Something went wrong",
            stack_trace="File 'foo.py', line 10\n  raise RuntimeError()",
            retryable=True,
        )
        assert error.stack_trace is not None
        assert "line 10" in error.stack_trace

    def test_step_execution_result_model(self):
        """Test StepExecutionResult model."""
        from tasker_core import ResultStatus, StepExecutionResult

        step_uuid = uuid4()
        task_uuid = uuid4()

        result = StepExecutionResult(
            step_uuid=step_uuid,
            task_uuid=task_uuid,
            success=True,
            status=ResultStatus.SUCCESS.value,
            execution_time_ms=150,
            result={"processed": 100},
            worker_id="python-worker-1",
        )
        assert result.success is True
        assert result.status == "completed"
        assert result.execution_time_ms == 150
        assert result.result["processed"] == 100
        assert result.error is None

    def test_step_execution_result_success_factory(self):
        """Test StepExecutionResult.success_result factory method."""
        from tasker_core import StepExecutionResult

        step_uuid = uuid4()
        task_uuid = uuid4()

        result = StepExecutionResult.success_result(
            step_uuid=step_uuid,
            task_uuid=task_uuid,
            result={"data": "test"},
            execution_time_ms=100,
            worker_id="test-worker",
        )
        assert result.success is True
        assert result.status == "completed"
        assert result.result["data"] == "test"
        assert result.worker_id == "test-worker"

    def test_step_execution_result_failure_factory(self):
        """Test StepExecutionResult.failure_result factory method."""
        from tasker_core import StepError, StepExecutionResult

        step_uuid = uuid4()
        task_uuid = uuid4()

        error = StepError(
            error_type="TimeoutError",
            message="Operation timed out",
            retryable=True,
        )
        result = StepExecutionResult.failure_result(
            step_uuid=step_uuid,
            task_uuid=task_uuid,
            error=error,
            execution_time_ms=30000,
            worker_id="test-worker",
        )
        assert result.success is False
        assert result.status == "retryable_error"
        assert result.error is not None
        assert result.error.retryable is True

    def test_step_execution_result_permanent_failure(self):
        """Test StepExecutionResult with permanent error."""
        from tasker_core import StepError, StepExecutionResult

        step_uuid = uuid4()
        task_uuid = uuid4()

        error = StepError(
            error_type="ValidationError",
            message="Invalid data",
            retryable=False,
        )
        result = StepExecutionResult.failure_result(
            step_uuid=step_uuid,
            task_uuid=task_uuid,
            error=error,
            execution_time_ms=50,
            worker_id="test-worker",
        )
        assert result.success is False
        assert result.status == "permanent_error"

    def test_ffi_step_event_model(self):
        """Test FfiStepEvent model."""
        from tasker_core import FfiStepEvent

        event = FfiStepEvent(
            event_id=str(uuid4()),
            task_uuid=str(uuid4()),
            step_uuid=str(uuid4()),
            correlation_id=str(uuid4()),
            task_sequence_step={"step": {"name": "test_step"}},
        )
        assert event.event_id is not None
        assert event.task_sequence_step["step"]["name"] == "test_step"
        assert event.trace_id is None

    def test_ffi_step_event_immutable(self):
        """Test that FfiStepEvent is frozen (immutable)."""
        from pydantic import ValidationError

        from tasker_core import FfiStepEvent

        event = FfiStepEvent(
            event_id=str(uuid4()),
            task_uuid=str(uuid4()),
            step_uuid=str(uuid4()),
            correlation_id=str(uuid4()),
            task_sequence_step={},
        )
        # Pydantic frozen models raise ValidationError on modification
        with pytest.raises(ValidationError):
            event.event_id = "new-id"

    def test_ffi_dispatch_metrics_model(self):
        """Test FfiDispatchMetrics model."""
        from tasker_core import FfiDispatchMetrics

        metrics = FfiDispatchMetrics(
            pending_count=5,
            oldest_pending_age_ms=1000,
            newest_pending_age_ms=100,
            oldest_event_id=str(uuid4()),
            starvation_detected=False,
            starving_event_count=0,
        )
        assert metrics.pending_count == 5
        assert metrics.oldest_pending_age_ms == 1000
        assert metrics.starvation_detected is False

    def test_ffi_dispatch_metrics_defaults(self):
        """Test FfiDispatchMetrics with defaults."""
        from tasker_core import FfiDispatchMetrics

        metrics = FfiDispatchMetrics(pending_count=0)
        assert metrics.pending_count == 0
        assert metrics.oldest_pending_age_ms is None
        assert metrics.starvation_detected is False
        assert metrics.starving_event_count == 0

    def test_starvation_warning_model(self):
        """Test StarvationWarning model."""
        from tasker_core import StarvationWarning

        warning = StarvationWarning(
            event_id=uuid4(),
            step_uuid=uuid4(),
            pending_duration_ms=15000,
            threshold_ms=10000,
        )
        assert warning.pending_duration_ms == 15000
        assert warning.threshold_ms == 10000


class TestDomainEventModels:
    """Test domain event Pydantic models."""

    def test_domain_event_metadata(self):
        """Test DomainEventMetadata model."""
        from tasker_core import DomainEventMetadata

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
        from tasker_core import DomainEventMetadata

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
        from tasker_core import DomainEventMetadata, InProcessDomainEvent

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
        from tasker_core import ComponentHealth

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
        from tasker_core import ComponentHealth

        component = ComponentHealth(
            name="custom",
            status="healthy",
        )

        assert component.pool_size is None
        assert component.pool_idle is None

    def test_health_check(self):
        """Test HealthCheck model."""
        from tasker_core import ComponentHealth, HealthCheck

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
        from tasker_core import WorkerMetrics

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
        from tasker_core import WorkerMetrics

        metrics = WorkerMetrics()

        assert metrics.dispatch_channel_pending == 0
        assert metrics.oldest_pending_age_ms is None
        assert metrics.starvation_detected is False
        assert metrics.steps_processed == 0

    def test_worker_config(self):
        """Test WorkerConfig model."""
        from tasker_core import WorkerConfig

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
        from tasker_core import WorkerConfig

        config = WorkerConfig()

        assert config.environment is None
        assert config.supported_namespaces == []
        assert config.web_api_enabled is False
        assert config.polling_interval_ms == 10
        assert config.starvation_threshold_ms == 1000


class TestBatchProcessingModels:
    """Test batch processing types."""

    def test_cursor_config(self):
        """Test CursorConfig model."""
        from tasker_core import CursorConfig

        config = CursorConfig(
            start_cursor=0,
            end_cursor=100,
            step_size=10,
            metadata={"partition": "A"},
        )
        assert config.start_cursor == 0
        assert config.end_cursor == 100
        assert config.step_size == 10
        assert config.metadata["partition"] == "A"

    def test_cursor_config_defaults(self):
        """Test CursorConfig with defaults."""
        from tasker_core import CursorConfig

        config = CursorConfig(start_cursor=0, end_cursor=50)
        assert config.step_size == 1
        assert config.metadata == {}

    def test_batch_analyzer_outcome(self):
        """Test BatchAnalyzerOutcome model."""
        from tasker_core import BatchAnalyzerOutcome, CursorConfig

        configs = [
            CursorConfig(start_cursor=0, end_cursor=100),
            CursorConfig(start_cursor=100, end_cursor=200),
        ]
        outcome = BatchAnalyzerOutcome(
            cursor_configs=configs,
            total_items=200,
            batch_metadata={"source": "database"},
        )
        assert len(outcome.cursor_configs) == 2
        assert outcome.total_items == 200
        assert outcome.batch_metadata["source"] == "database"

    def test_batch_analyzer_outcome_from_ranges(self):
        """Test BatchAnalyzerOutcome.from_ranges factory."""
        from tasker_core import BatchAnalyzerOutcome

        outcome = BatchAnalyzerOutcome.from_ranges(
            ranges=[(0, 100), (100, 200), (200, 300)],
            step_size=10,
            total_items=300,
            batch_metadata={"version": "1"},
        )
        assert len(outcome.cursor_configs) == 3
        assert outcome.cursor_configs[0].start_cursor == 0
        assert outcome.cursor_configs[0].end_cursor == 100
        assert outcome.cursor_configs[0].step_size == 10
        assert outcome.total_items == 300

    def test_batch_worker_context(self):
        """Test BatchWorkerContext model."""
        from tasker_core import BatchWorkerContext, CursorConfig

        config = CursorConfig(start_cursor=0, end_cursor=100, step_size=5)
        context = BatchWorkerContext(
            batch_id="batch_001",
            cursor_config=config,
            batch_index=0,
            total_batches=10,
            batch_metadata={"source": "test"},
        )
        assert context.batch_id == "batch_001"
        assert context.start_cursor == 0
        assert context.end_cursor == 100
        assert context.step_size == 5
        assert context.batch_index == 0
        assert context.total_batches == 10

    def test_batch_worker_outcome(self):
        """Test BatchWorkerOutcome model."""
        from tasker_core import BatchWorkerOutcome

        outcome = BatchWorkerOutcome(
            items_processed=100,
            items_succeeded=95,
            items_failed=5,
            items_skipped=0,
            results=[{"id": i} for i in range(5)],
            errors=[{"id": i, "error": "failed"} for i in range(5)],
            last_cursor=99,
        )
        assert outcome.items_processed == 100
        assert outcome.items_succeeded == 95
        assert outcome.items_failed == 5
        assert len(outcome.results) == 5
        assert len(outcome.errors) == 5
        assert outcome.last_cursor == 99


class TestModelSerialization:
    """Test model serialization and deserialization."""

    def test_health_check_from_dict(self):
        """Test HealthCheck can be created from dict."""
        from tasker_core import HealthCheck

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
        from tasker_core import WorkerMetrics

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
        from tasker_core import WorkerConfig

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
        from tasker_core import InProcessDomainEvent

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
