"""Decision handler tests.

These tests verify:
- DecisionType enum values
- DecisionPointOutcome factory methods
- DecisionHandler class and helper methods
"""

from __future__ import annotations


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


class TestDecisionHandler:
    """Test DecisionHandler class."""

    def test_decision_handler_subclass(self):
        """Test creating a DecisionHandler subclass."""
        from tasker_core import DecisionHandler, DecisionPointOutcome

        class TestDecisionHandler(DecisionHandler):
            handler_name = "test_decision"

            def call(self, _context):
                outcome = DecisionPointOutcome.create_steps(["next_step"])
                return self.decision_success_with_outcome(outcome)

        handler = TestDecisionHandler()
        assert handler.handler_name == "test_decision"
        assert "decision" in handler.capabilities
        assert "routing" in handler.capabilities

    def test_decision_handler_simplified_api(self):
        """Test DecisionHandler with simplified decision_success API."""
        from tasker_core import DecisionHandler

        class TestDecisionHandler(DecisionHandler):
            handler_name = "test_decision"

            def call(self, _context):
                # Use simplified cross-language API
                return self.decision_success(
                    ["step_a", "step_b"],
                    routing_context={"reason": "test"},
                )

        handler = TestDecisionHandler()
        result = handler.decision_success(["next_step"], routing_context={"x": 1})
        assert result.is_success is True
        assert result.result["decision_point_outcome"]["step_names"] == ["next_step"]
        assert result.result["decision_point_outcome"]["type"] == "create_steps"

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
        assert result.is_success is True
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
        assert result.is_success is True
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
        assert result.is_success is False
        assert result.error_message == "Invalid input"
        assert result.error_type == "invalid_input"
        assert result.retryable is False  # Decision failures default to not retryable
