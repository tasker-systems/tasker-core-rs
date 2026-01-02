"""Step handler mixins.

TAS-112: Composition Pattern - Mixin Classes

This module provides mixin classes for step handler capabilities.
Use mixins via multiple inheritance with StepHandler base class.

Example:
    >>> from tasker_core.step_handler import StepHandler
    >>> from tasker_core.step_handler.mixins import APIMixin, DecisionMixin
    >>>
    >>> class MyApiHandler(APIMixin, StepHandler):
    ...     handler_name = "my_api_handler"
    ...     base_url = "https://api.example.com"
    ...
    ...     def call(self, context):
    ...         response = self.get("/users")
    ...         return self.api_success(response)
    >>>
    >>> class MyDecisionHandler(DecisionMixin, StepHandler):
    ...     handler_name = "my_decision_handler"
    ...
    ...     def call(self, context):
    ...         return self.decision_success(["next_step"])

Combining Mixins:
    >>> class ApiDecisionHandler(APIMixin, DecisionMixin, StepHandler):
    ...     handler_name = "api_decision_handler"
    ...
    ...     def call(self, context):
    ...         # Can use both API and Decision helpers
    ...         response = self.get("/check")
    ...         if response.ok:
    ...             return self.decision_success(["proceed"])
    ...         return self.skip_branches(reason="API check failed")
"""

from __future__ import annotations

from tasker_core.step_handler.mixins.api import APIMixin, ApiResponse
from tasker_core.step_handler.mixins.decision import DecisionMixin

__all__ = [
    "APIMixin",
    "ApiResponse",
    "DecisionMixin",
]
