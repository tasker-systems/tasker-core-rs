"""Module export tests.

These tests verify:
- All expected symbols are exported from tasker_core
- Submodule exports are available
- __all__ list is comprehensive
"""

from __future__ import annotations

import tasker_core


class TestPhase3Exports:
    """Test that Phase 3 exports are available."""

    def test_phase3_ffi_functions_exported(self):
        """Test Phase 3 FFI functions are exported."""
        from tasker_core import (
            check_starvation_warnings,
            cleanup_timeouts,
            complete_step_event,
            get_ffi_dispatch_metrics,
            poll_step_events,
        )

        assert callable(poll_step_events)
        assert callable(complete_step_event)
        assert callable(get_ffi_dispatch_metrics)
        assert callable(check_starvation_warnings)
        assert callable(cleanup_timeouts)

    def test_phase3_types_exported(self):
        """Test Phase 3 types are exported."""
        from tasker_core import (
            FfiDispatchMetrics,
            FfiStepEvent,
            ResultStatus,
            StarvationWarning,
            StepError,
            StepExecutionResult,
        )

        assert FfiStepEvent is not None
        assert StepExecutionResult is not None
        assert StepError is not None
        assert ResultStatus is not None
        assert FfiDispatchMetrics is not None
        assert StarvationWarning is not None

    def test_event_poller_exported(self):
        """Test EventPoller is exported."""
        from tasker_core import EventPoller

        assert EventPoller is not None
        assert callable(EventPoller)

    def test_all_exports_includes_phase3(self):
        """Test __all__ includes Phase 3 exports."""
        phase3_exports = {
            "poll_step_events",
            "complete_step_event",
            "get_ffi_dispatch_metrics",
            "check_starvation_warnings",
            "cleanup_timeouts",
            "FfiStepEvent",
            "StepExecutionResult",
            "StepError",
            "ResultStatus",
            "FfiDispatchMetrics",
            "StarvationWarning",
            "EventPoller",
        }

        assert phase3_exports.issubset(set(tasker_core.__all__))


class TestPhase5Exports:
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


class TestTAS95Exports:
    """Test that TAS-95 API alignment exports are available."""

    def test_error_type_enum_exported(self):
        """Test ErrorType enum is exported from tasker_core."""
        from tasker_core import ErrorType

        assert ErrorType is not None
        assert ErrorType.PERMANENT_ERROR == "permanent_error"
        assert ErrorType.RETRYABLE_ERROR == "retryable_error"
        assert ErrorType.VALIDATION_ERROR == "validation_error"
        assert ErrorType.TIMEOUT == "timeout"
        assert ErrorType.HANDLER_ERROR == "handler_error"

    def test_error_type_helper_methods(self):
        """Test ErrorType helper methods."""
        from tasker_core import ErrorType

        assert ErrorType.is_standard("permanent_error") is True
        assert ErrorType.is_standard("custom_error") is False
        assert ErrorType.is_typically_retryable("retryable_error") is True
        assert ErrorType.is_typically_retryable("permanent_error") is False

    def test_base_publisher_exported(self):
        """Test BasePublisher is exported from tasker_core."""
        from tasker_core import BasePublisher

        assert BasePublisher is not None
        # Verify it's an abstract class
        from abc import ABC

        assert issubclass(BasePublisher, ABC)

    def test_base_subscriber_exported(self):
        """Test BaseSubscriber is exported from tasker_core."""
        from tasker_core import BaseSubscriber

        assert BaseSubscriber is not None
        # Verify it's an abstract class
        from abc import ABC

        assert issubclass(BaseSubscriber, ABC)

    def test_step_event_context_exported(self):
        """Test StepEventContext is exported from tasker_core."""
        from tasker_core import StepEventContext

        assert StepEventContext is not None
        # Verify it can be instantiated
        ctx = StepEventContext(
            task_uuid="task-123",
            step_uuid="step-456",
            step_name="test_step",
            namespace="test",
            correlation_id="corr-789",
        )
        assert ctx.task_uuid == "task-123"
        assert ctx.step_name == "test_step"

    def test_all_exports_includes_tas95(self):
        """Test __all__ includes TAS-95 exports."""
        tas95_exports = {
            "ErrorType",
            "BasePublisher",
            "BaseSubscriber",
            "StepEventContext",
        }
        assert tas95_exports.issubset(set(tasker_core.__all__))


class TestPhase6bExports:
    """Test that Phase 6b exports are available."""

    def test_decision_types_exported(self):
        """Test decision types are exported from tasker_core."""
        from tasker_core import DecisionPointOutcome, DecisionType

        assert DecisionType is not None
        assert DecisionPointOutcome is not None

    def test_batch_types_exported(self):
        """Test batch types are exported from tasker_core."""
        from tasker_core import (
            BatchAnalyzerOutcome,
            BatchWorkerContext,
            BatchWorkerOutcome,
            CursorConfig,
        )

        assert CursorConfig is not None
        assert BatchAnalyzerOutcome is not None
        assert BatchWorkerContext is not None
        assert BatchWorkerOutcome is not None

    def test_handlers_exported(self):
        """Test specialized handlers are exported from tasker_core."""
        from tasker_core import ApiHandler, ApiResponse, DecisionHandler

        assert ApiHandler is not None
        assert ApiResponse is not None
        assert DecisionHandler is not None

    def test_batchable_exported(self):
        """Test Batchable mixin is exported from tasker_core."""
        from tasker_core import Batchable

        assert Batchable is not None

    def test_step_handler_submodule_exports(self):
        """Test step_handler submodule exports."""
        from tasker_core.step_handler import (
            ApiHandler,
            ApiResponse,
            DecisionHandler,
            StepHandler,
        )

        assert StepHandler is not None
        assert ApiHandler is not None
        assert ApiResponse is not None
        assert DecisionHandler is not None

    def test_batch_processing_submodule_exports(self):
        """Test batch_processing submodule exports."""
        from tasker_core.batch_processing import Batchable

        assert Batchable is not None

    def test_all_exports_includes_phase6b(self):
        """Test __all__ includes Phase 6b exports."""
        phase6b_exports = {
            "ApiHandler",
            "ApiResponse",
            "DecisionHandler",
            "Batchable",
            "DecisionType",
            "DecisionPointOutcome",
            "CursorConfig",
            "BatchAnalyzerOutcome",
            "BatchWorkerContext",
            "BatchWorkerOutcome",
        }

        assert phase6b_exports.issubset(set(tasker_core.__all__))
