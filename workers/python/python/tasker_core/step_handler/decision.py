"""Decision handler for workflow routing.

TAS-112: Composition Pattern (DEPRECATED CLASS)

This module provides the DecisionHandler class for backward compatibility.
For new code, use the mixin pattern:

    >>> from tasker_core.step_handler import StepHandler
    >>> from tasker_core.step_handler.mixins import DecisionMixin
    >>>
    >>> class RouteOrderHandler(DecisionMixin, StepHandler):
    ...     handler_name = "route_order"
    ...
    ...     def call(self, context):
    ...         order_type = context.input_data.get("order_type")
    ...         if order_type == "premium":
    ...             return self.decision_success(
    ...                 ["validate_premium", "process_premium"],
    ...                 routing_context={"order_type": order_type}
    ...             )
    ...         return self.decision_success(["process_standard"])
"""

from __future__ import annotations

from tasker_core.step_handler.base import StepHandler
from tasker_core.step_handler.mixins.decision import DecisionMixin

__all__ = ["DecisionHandler"]


class DecisionHandler(DecisionMixin, StepHandler):
    """Base class for decision point step handlers.

    TAS-112: This class is provided for backward compatibility.
    For new code, prefer using DecisionMixin directly:

        class MyHandler(DecisionMixin, StepHandler):
            ...

    Decision handlers are used to make routing decisions in workflows.
    They evaluate conditions and determine which steps should execute next.

    Subclasses should implement the `call` method and use the helper methods
    to return outcomes:
    - `decision_success(steps, routing_context)` - simplified cross-language API
    - `decision_success_with_outcome(outcome)` - for complex DecisionPointOutcome
    - `skip_branches(reason)` - when no steps should execute

    Example:
        >>> class CustomerTierRouter(DecisionHandler):
        ...     handler_name = "route_by_tier"
        ...
        ...     def call(self, context):
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
