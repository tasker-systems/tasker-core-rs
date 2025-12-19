"""Step handler module.

This module provides the step handler base class and specialized handlers
for different execution patterns.

Classes:
    StepHandler: Abstract base class for all step handlers.
    ApiHandler: HTTP API handler with automatic error classification.
    DecisionHandler: Decision point handler for dynamic workflow routing.

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
"""

from __future__ import annotations

from tasker_core.step_handler.api import ApiHandler, ApiResponse
from tasker_core.step_handler.base import StepHandler
from tasker_core.step_handler.decision import DecisionHandler

__all__ = [
    "StepHandler",
    "ApiHandler",
    "ApiResponse",
    "DecisionHandler",
]
