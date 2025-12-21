"""Decision handler for workflow routing.

This module provides the DecisionHandler base class for step handlers that
make routing decisions in workflows. Decision handlers determine which
branches of a workflow to execute based on input data or conditions.

Example:
    >>> from tasker_core.step_handler import DecisionHandler
    >>> from tasker_core import StepContext, StepHandlerResult
    >>>
    >>> class RouteOrderHandler(DecisionHandler):
    ...     handler_name = "route_order"
    ...
    ...     def call(self, context: StepContext) -> StepHandlerResult:
    ...         order_type = context.input_data.get("order_type")
    ...         if order_type == "premium":
    ...             # Simplified cross-language API
    ...             return self.decision_success(
    ...                 ["validate_premium", "process_premium"],
    ...                 routing_context={"order_type": order_type}
    ...             )
    ...         elif order_type == "standard":
    ...             return self.decision_success(["process_standard"])
    ...         else:
    ...             return self.skip_branches(reason="Unknown order type")
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

from tasker_core.step_handler.base import StepHandler
from tasker_core.types import DecisionPointOutcome

if TYPE_CHECKING:
    from tasker_core.types import StepHandlerResult


class DecisionHandler(StepHandler):
    """Base class for decision point step handlers.

    Decision handlers are used to make routing decisions in workflows.
    They evaluate conditions and determine which steps should execute next.

    Subclasses should implement the `call` method and use the helper methods
    to return outcomes:
    - `decision_success(steps, routing_context)` - simplified cross-language API
    - `decision_success_with_outcome(outcome)` - for complex DecisionPointOutcome
    - `skip_branches(reason)` - when no steps should execute

    Class Attributes:
        handler_name: Unique identifier for this handler.
        handler_version: Version string for the handler.

    Example:
        >>> class CustomerTierRouter(DecisionHandler):
        ...     handler_name = "route_by_tier"
        ...
        ...     def call(self, context: StepContext) -> StepHandlerResult:
        ...         tier = context.input_data.get("customer_tier")
        ...         if tier == "enterprise":
        ...             return self.decision_success(
        ...                 ["enterprise_validation", "enterprise_processing"],
        ...                 routing_context={"tier": tier}
        ...             )
        ...         elif tier == "premium":
        ...             return self.decision_success(["premium_processing"])
        ...         else:
        ...             return self.decision_success(["standard_processing"])
    """

    @property
    def capabilities(self) -> list[str]:
        """Return handler capabilities."""
        return ["process", "decision", "routing"]

    def decision_success(
        self,
        steps: list[str],
        routing_context: dict[str, Any] | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> StepHandlerResult:
        """Simplified decision success helper (cross-language standard API).

        Use this when routing to one or more steps based on a decision.
        This is the recommended method for most decision handlers.

        Args:
            steps: List of step names to activate.
            routing_context: Optional context for routing decisions.
            metadata: Optional additional metadata.

        Returns:
            A success StepHandlerResult with the decision outcome.

        Example:
            >>> # Simple routing
            >>> return self.decision_success(["process_order"])
            >>>
            >>> # With routing context
            >>> return self.decision_success(
            ...     ["validate_premium", "process_premium"],
            ...     routing_context={"tier": "premium"}
            ... )
        """
        outcome = DecisionPointOutcome.create_steps(steps, routing_context=routing_context)
        return self.decision_success_with_outcome(outcome, metadata=metadata)

    def decision_success_with_outcome(
        self,
        outcome: DecisionPointOutcome,
        metadata: dict[str, Any] | None = None,
    ) -> StepHandlerResult:
        """Create a success result with a DecisionPointOutcome.

        Use this for complex decision outcomes that require dynamic steps
        or advanced routing. For simple step routing, use `decision_success()`.

        Args:
            outcome: The decision point outcome specifying next steps.
            metadata: Optional additional metadata.

        Returns:
            A success StepHandlerResult with the decision outcome.

        Example:
            >>> outcome = DecisionPointOutcome.create_steps(
            ...     ["process_premium"],
            ...     dynamic_steps=[{"name": "custom_step", ...}],
            ... )
            >>> return self.decision_success_with_outcome(outcome)
        """
        # Build decision_point_outcome in format Rust expects
        # (matches Ruby's DecisionPointOutcome.to_h structure)
        decision_point_outcome: dict[str, Any] = {
            "type": outcome.decision_type.value,  # "create_steps" or "no_branches"
            "step_names": outcome.next_step_names,
        }

        result: dict[str, Any] = {
            "decision_point_outcome": decision_point_outcome,
        }

        if outcome.dynamic_steps:
            result["dynamic_steps"] = outcome.dynamic_steps

        if outcome.routing_context:
            result["routing_context"] = outcome.routing_context

        combined_metadata = metadata or {}
        combined_metadata["decision_handler"] = self.name
        combined_metadata["decision_version"] = self.version

        return self.success(result, metadata=combined_metadata)

    def decision_no_branches(
        self,
        outcome: DecisionPointOutcome,
        metadata: dict[str, Any] | None = None,
    ) -> StepHandlerResult:
        """Create a success result for a decision with no branches.

        Use this when the decision results in no additional steps being executed.
        This is still a successful outcome - the decision was made correctly,
        it just doesn't require any follow-up steps.

        Args:
            outcome: The decision point outcome with reason for no branches.
            metadata: Optional additional metadata.

        Returns:
            A success StepHandlerResult indicating no branches.

        Example:
            >>> outcome = DecisionPointOutcome.no_branches(
            ...     reason="No items match processing criteria"
            ... )
            >>> return self.decision_no_branches(outcome)
        """
        # Build decision_point_outcome in format Rust expects
        # (matches Ruby's DecisionPointOutcome.to_h structure)
        decision_point_outcome: dict[str, Any] = {
            "type": outcome.decision_type.value,  # "no_branches"
        }

        result: dict[str, Any] = {
            "decision_point_outcome": decision_point_outcome,
            "reason": outcome.reason,
        }

        if outcome.routing_context:
            result["routing_context"] = outcome.routing_context

        combined_metadata = metadata or {}
        combined_metadata["decision_handler"] = self.name
        combined_metadata["decision_version"] = self.version

        return self.success(result, metadata=combined_metadata)

    def route_to_steps(
        self,
        step_names: list[str],
        routing_context: dict[str, Any] | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> StepHandlerResult:
        """Alias for decision_success(). Use decision_success() instead.

        This method is kept for backward compatibility. Prefer using
        `decision_success(steps, routing_context)` for cross-language consistency.

        Args:
            step_names: Names of the steps to execute.
            routing_context: Optional context data for routing.
            metadata: Optional additional metadata.

        Returns:
            A success StepHandlerResult routing to the specified steps.
        """
        return self.decision_success(step_names, routing_context, metadata)

    def skip_branches(
        self,
        reason: str,
        routing_context: dict[str, Any] | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> StepHandlerResult:
        """Convenience method to skip all branches.

        A shorthand for creating a no-branches DecisionPointOutcome.

        Args:
            reason: Human-readable reason for skipping branches.
            routing_context: Optional context data.
            metadata: Optional additional metadata.

        Returns:
            A success StepHandlerResult indicating no branches.

        Example:
            >>> if not items_to_process:
            ...     return self.skip_branches(
            ...         reason="No items require processing"
            ...     )
        """
        outcome = DecisionPointOutcome.no_branches(
            reason=reason,
            routing_context=routing_context,
        )
        return self.decision_no_branches(outcome, metadata=metadata)

    def decision_failure(
        self,
        message: str,
        error_type: str = "decision_error",
        retryable: bool = False,
        metadata: dict[str, Any] | None = None,
    ) -> StepHandlerResult:
        """Create a failure result for a decision that could not be made.

        Use this when the handler cannot determine the appropriate routing,
        typically due to invalid input data or missing required information.

        Decision failures are usually NOT retryable because they typically
        indicate data issues rather than transient errors.

        Args:
            message: Human-readable error message.
            error_type: Error type classification.
            retryable: Whether the error is retryable (default: False).
            metadata: Optional additional metadata.

        Returns:
            A failure StepHandlerResult.

        Example:
            >>> if "order_type" not in context.input_data:
            ...     return self.decision_failure(
            ...         message="Missing required field: order_type",
            ...         error_type="missing_field",
            ...     )
        """
        combined_metadata = metadata or {}
        combined_metadata["decision_handler"] = self.name
        combined_metadata["decision_version"] = self.version

        return self.failure(
            message=message,
            error_type=error_type,
            retryable=retryable,
            metadata=combined_metadata,
        )


__all__ = ["DecisionHandler"]
