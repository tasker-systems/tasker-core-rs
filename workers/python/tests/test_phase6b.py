"""Phase 6b Tests: Specialized Handlers and Batch Processing.

These tests verify:
- DecisionPointOutcome and related types
- BatchProcessingOutcome and related types
- ApiHandler base class
- DecisionHandler base class
- Batchable mixin
"""

from __future__ import annotations

from unittest.mock import Mock


class TestDecisionTypes:
    """Test decision point outcome types."""

    def test_decision_type_enum(self):
        """Test DecisionType enum values."""
        from tasker_core import DecisionType

        assert DecisionType.CREATE_STEPS == "create_steps"
        assert DecisionType.NO_BRANCHES == "no_branches"

    def test_decision_point_outcome_create_steps(self):
        """Test creating a create_steps outcome."""
        from tasker_core import DecisionPointOutcome, DecisionType

        outcome = DecisionPointOutcome.create_steps(
            ["step_a", "step_b"],
            routing_context={"reason": "test"},
        )
        assert outcome.decision_type == DecisionType.CREATE_STEPS
        assert outcome.next_step_names == ["step_a", "step_b"]
        assert outcome.routing_context == {"reason": "test"}
        assert outcome.reason is None

    def test_decision_point_outcome_no_branches(self):
        """Test creating a no_branches outcome."""
        from tasker_core import DecisionPointOutcome, DecisionType

        outcome = DecisionPointOutcome.no_branches(
            reason="No items match criteria",
            routing_context={"filter": "active"},
        )
        assert outcome.decision_type == DecisionType.NO_BRANCHES
        assert outcome.next_step_names == []
        assert outcome.reason == "No items match criteria"
        assert outcome.routing_context == {"filter": "active"}

    def test_decision_point_outcome_with_dynamic_steps(self):
        """Test creating outcome with dynamic steps."""
        from tasker_core import DecisionPointOutcome

        dynamic_steps = [
            {"name": "dynamic_step_1", "handler": "process_dynamic"},
            {"name": "dynamic_step_2", "handler": "process_dynamic"},
        ]
        outcome = DecisionPointOutcome.create_steps(
            ["static_step"],
            dynamic_steps=dynamic_steps,
        )
        assert outcome.next_step_names == ["static_step"]
        assert outcome.dynamic_steps == dynamic_steps


class TestBatchTypes:
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


class TestApiHandler:
    """Test ApiHandler class."""

    def test_api_handler_subclass(self):
        """Test creating an ApiHandler subclass."""
        from tasker_core import ApiHandler

        class TestApiHandler(ApiHandler):
            handler_name = "test_api_handler"
            base_url = "https://api.example.com"

            def call(self, _context):
                return self.success({"data": "test"})

        handler = TestApiHandler()
        assert handler.handler_name == "test_api_handler"
        assert handler.base_url == "https://api.example.com"
        assert "api" in handler.capabilities
        assert "http" in handler.capabilities

    def test_api_handler_has_client(self):
        """Test ApiHandler creates httpx client."""
        from tasker_core import ApiHandler

        class TestHandler(ApiHandler):
            handler_name = "test"
            base_url = "https://test.com"

            def call(self, _context):
                return self.success({})

        handler = TestHandler()
        # Client is lazily created
        assert handler._client is None
        # Accessing .client creates it
        client = handler.client
        assert client is not None
        handler.close()
        assert handler._client is None


class TestApiResponse:
    """Test ApiResponse class."""

    def test_api_response_properties(self):
        """Test ApiResponse properties."""
        from tasker_core.step_handler.api import ApiResponse

        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.headers = {"content-type": "application/json"}
        mock_response.json.return_value = {"data": "test"}

        response = ApiResponse(mock_response)
        assert response.status_code == 200
        assert response.ok is True
        assert response.is_client_error is False
        assert response.is_server_error is False
        assert response.body == {"data": "test"}

    def test_api_response_client_error(self):
        """Test ApiResponse with client error."""
        from tasker_core.step_handler.api import ApiResponse

        mock_response = Mock()
        mock_response.status_code = 404
        mock_response.headers = {"content-type": "text/plain"}
        mock_response.text = "Not Found"

        response = ApiResponse(mock_response)
        assert response.ok is False
        assert response.is_client_error is True
        assert response.is_retryable is False

    def test_api_response_server_error_retryable(self):
        """Test ApiResponse with retryable server error."""
        from tasker_core.step_handler.api import ApiResponse

        mock_response = Mock()
        mock_response.status_code = 503
        mock_response.headers = {"content-type": "text/plain", "retry-after": "30"}
        mock_response.text = "Service Unavailable"

        response = ApiResponse(mock_response)
        assert response.ok is False
        assert response.is_server_error is True
        assert response.is_retryable is True
        assert response.retry_after == 30

    def test_api_response_to_dict(self):
        """Test ApiResponse.to_dict()."""
        from tasker_core.step_handler.api import ApiResponse

        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.headers = {"content-type": "application/json"}
        mock_response.json.return_value = {"key": "value"}

        response = ApiResponse(mock_response)
        result = response.to_dict()
        assert result["status_code"] == 200
        assert result["body"] == {"key": "value"}


class TestDecisionHandler:
    """Test DecisionHandler class."""

    def test_decision_handler_subclass(self):
        """Test creating a DecisionHandler subclass."""
        from tasker_core import DecisionHandler, DecisionPointOutcome

        class TestDecisionHandler(DecisionHandler):
            handler_name = "test_decision"

            def call(self, _context):
                outcome = DecisionPointOutcome.create_steps(["next_step"])
                return self.decision_success(outcome)

        handler = TestDecisionHandler()
        assert handler.handler_name == "test_decision"
        assert "decision" in handler.capabilities
        assert "routing" in handler.capabilities

    def test_decision_handler_route_to_steps(self):
        """Test DecisionHandler.route_to_steps helper."""
        from tasker_core import DecisionHandler

        class TestHandler(DecisionHandler):
            handler_name = "test"

            def call(self, _context):
                return self.route_to_steps(
                    ["step_a", "step_b"],
                    routing_context={"reason": "test"},
                )

        handler = TestHandler()
        # Mock a context just enough to call the method
        result = handler.route_to_steps(["step_a"], routing_context={"x": 1})
        assert result.success is True
        # Result now uses decision_point_outcome wrapper (matches Rust expectations)
        assert result.result["decision_point_outcome"]["step_names"] == ["step_a"]
        assert result.result["decision_point_outcome"]["type"] == "create_steps"

    def test_decision_handler_skip_branches(self):
        """Test DecisionHandler.skip_branches helper."""
        from tasker_core import DecisionHandler

        class TestHandler(DecisionHandler):
            handler_name = "test"

            def call(self, _context):
                return self.skip_branches(reason="No items")

        handler = TestHandler()
        result = handler.skip_branches(reason="Nothing to do")
        assert result.success is True
        # Result now uses decision_point_outcome wrapper (matches Rust expectations)
        assert result.result["decision_point_outcome"]["type"] == "no_branches"
        assert result.result["reason"] == "Nothing to do"

    def test_decision_handler_failure(self):
        """Test DecisionHandler.decision_failure helper."""
        from tasker_core import DecisionHandler

        class TestHandler(DecisionHandler):
            handler_name = "test"

            def call(self, _context):
                return self.decision_failure(
                    message="Missing required field",
                    error_type="validation_error",
                )

        handler = TestHandler()
        result = handler.decision_failure(
            message="Invalid input",
            error_type="invalid_input",
        )
        assert result.success is False
        assert result.error_message == "Invalid input"
        assert result.error_type == "invalid_input"
        assert result.retryable is False  # Decision failures default to not retryable


class TestBatchable:
    """Test Batchable mixin."""

    def test_batchable_create_cursor_config(self):
        """Test Batchable.create_cursor_config."""
        from tasker_core import Batchable

        class TestHandler(Batchable):
            pass

        handler = TestHandler()
        config = handler.create_cursor_config(0, 100, step_size=5)
        assert config.start_cursor == 0
        assert config.end_cursor == 100
        assert config.step_size == 5

    def test_batchable_create_cursor_ranges(self):
        """Test Batchable.create_cursor_ranges."""
        from tasker_core import Batchable

        class TestHandler(Batchable):
            pass

        handler = TestHandler()
        configs = handler.create_cursor_ranges(total_items=250, batch_size=100)
        assert len(configs) == 3
        assert configs[0].start_cursor == 0
        assert configs[0].end_cursor == 100
        assert configs[1].start_cursor == 100
        assert configs[1].end_cursor == 200
        assert configs[2].start_cursor == 200
        assert configs[2].end_cursor == 250

    def test_batchable_create_batch_outcome(self):
        """Test Batchable.create_batch_outcome."""
        from tasker_core import Batchable

        class TestHandler(Batchable):
            pass

        handler = TestHandler()
        outcome = handler.create_batch_outcome(
            total_items=500,
            batch_size=100,
            batch_metadata={"source": "test"},
        )
        assert len(outcome.cursor_configs) == 5
        assert outcome.total_items == 500
        assert outcome.batch_metadata["source"] == "test"

    def test_batchable_create_worker_outcome(self):
        """Test Batchable.create_worker_outcome."""
        from tasker_core import Batchable

        class TestHandler(Batchable):
            pass

        handler = TestHandler()
        outcome = handler.create_worker_outcome(
            items_processed=100,
            items_succeeded=95,
            items_failed=5,
            last_cursor=99,
        )
        assert outcome.items_processed == 100
        assert outcome.items_succeeded == 95
        assert outcome.items_failed == 5
        assert outcome.last_cursor == 99

    def test_batchable_aggregate_worker_results(self):
        """Test Batchable.aggregate_worker_results."""
        from tasker_core import Batchable

        results = [
            {
                "items_processed": 100,
                "items_succeeded": 95,
                "items_failed": 5,
                "errors": [{"id": 1, "error": "fail"}],
            },
            {
                "items_processed": 100,
                "items_succeeded": 98,
                "items_failed": 2,
                "errors": [{"id": 2, "error": "fail"}],
            },
            {
                "items_processed": 50,
                "items_succeeded": 50,
                "items_failed": 0,
            },
        ]
        summary = Batchable.aggregate_worker_results(results)
        assert summary["total_processed"] == 250
        assert summary["total_succeeded"] == 243
        assert summary["total_failed"] == 7
        assert summary["batch_count"] == 3
        assert len(summary["errors"]) == 2


class TestBatchableWithStepHandler:
    """Test Batchable mixin with StepHandler."""

    def test_batchable_analyzer_success(self):
        """Test batch_analyzer_success method."""
        from tasker_core import Batchable, StepHandler

        class TestAnalyzer(StepHandler, Batchable):
            handler_name = "test_analyzer"

            def call(self, _context):
                outcome = self.create_batch_outcome(total_items=100, batch_size=25)
                return self.batch_analyzer_success(outcome)

        handler = TestAnalyzer()
        outcome = handler.create_batch_outcome(total_items=100, batch_size=25)
        result = handler.batch_analyzer_success(outcome)

        assert result.success is True
        # Result now uses batch_processing_outcome wrapper (matches Rust expectations)
        assert result.result["worker_count"] == 4
        assert result.result["total_items"] == 100
        assert len(result.result["batch_processing_outcome"]["cursor_configs"]) == 4
        assert result.result["batch_processing_outcome"]["type"] == "create_batches"
        assert result.metadata.get("batch_analyzer") is True

    def test_batchable_worker_success(self):
        """Test batch_worker_success method."""
        from tasker_core import Batchable, StepHandler

        class TestWorker(StepHandler, Batchable):
            handler_name = "test_worker"

            def call(self, _context):
                outcome = self.create_worker_outcome(
                    items_processed=25,
                    items_succeeded=24,
                    items_failed=1,
                )
                return self.batch_worker_success(outcome)

        handler = TestWorker()
        outcome = handler.create_worker_outcome(
            items_processed=25,
            items_succeeded=24,
            items_failed=1,
        )
        result = handler.batch_worker_success(outcome)

        assert result.success is True
        assert result.result["items_processed"] == 25
        assert result.result["items_succeeded"] == 24
        assert result.result["items_failed"] == 1
        assert result.metadata.get("batch_worker") is True


class TestModuleExportsPhase6b:
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
        import tasker_core

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
