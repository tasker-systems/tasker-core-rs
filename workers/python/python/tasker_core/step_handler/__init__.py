"""Step handler module.

This module provides the step handler base class and specialized handlers
for different execution patterns.

Classes:
    StepHandler: Abstract base class for all step handlers.
    ApiHandler: HTTP API handler with automatic error classification.
    DecisionHandler: Decision point handler for dynamic workflow routing.

Mixins (TAS-112 - Composition Pattern):
    APIMixin: HTTP API functionality mixin.
    DecisionMixin: Decision point functionality mixin.

Example:
    >>> from tasker_core.step_handler import StepHandler, ApiHandler
    >>>
    >>> class MyHandler(StepHandler):
    ...     handler_name = "my_handler"
    ...
    ...     def call(self, context):
    ...         return self.success({"processed": True})

    >>> from tasker_core.step_handler import ApiHandler
    >>>
    >>> class FetchDataHandler(ApiHandler):
    ...     handler_name = "fetch_data"
    ...     base_url = "https://api.example.com"
    ...
    ...     def call(self, context):
    ...         response = self.get("/data")
    ...         if response.ok:
    ...             return self.api_success(response)
    ...         return self.api_failure(response)

Mixin Pattern (Recommended for new code):
    >>> from tasker_core.step_handler import StepHandler
    >>> from tasker_core.step_handler.mixins import APIMixin
    >>>
    >>> class FetchDataHandler(APIMixin, StepHandler):
    ...     handler_name = "fetch_data"
    ...     base_url = "https://api.example.com"
    ...
    ...     def call(self, context):
    ...         response = self.get("/data")
    ...         if response.ok:
    ...             return self.api_success(response)
    ...         return self.api_failure(response)
"""

from __future__ import annotations

# TAS-112: Export mixins module for composition pattern
from tasker_core.step_handler import mixins
from tasker_core.step_handler.api import ApiHandler, ApiResponse
from tasker_core.step_handler.base import StepHandler
from tasker_core.step_handler.decision import DecisionHandler

__all__ = [
    "StepHandler",
    "ApiHandler",
    "ApiResponse",
    "DecisionHandler",
    "mixins",
]
