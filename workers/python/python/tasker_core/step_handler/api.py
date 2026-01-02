"""API handler for HTTP interactions.

TAS-112: Composition Pattern (DEPRECATED CLASS)

This module provides the ApiHandler class for backward compatibility.
For new code, use the mixin pattern:

    >>> from tasker_core.step_handler import StepHandler
    >>> from tasker_core.step_handler.mixins import APIMixin
    >>>
    >>> class FetchUserHandler(APIMixin, StepHandler):
    ...     handler_name = "fetch_user"
    ...     base_url = "https://api.example.com"
    ...
    ...     def call(self, context):
    ...         user_id = context.input_data["user_id"]
    ...         response = self.get(f"/users/{user_id}")
    ...         return self.api_success(response)
"""

from __future__ import annotations

from tasker_core.step_handler.base import StepHandler
from tasker_core.step_handler.mixins.api import APIMixin, ApiResponse

# Re-export ApiResponse for convenience
__all__ = ["ApiHandler", "ApiResponse"]


class ApiHandler(APIMixin, StepHandler):
    """Base class for HTTP API step handlers.

    TAS-112: This class is provided for backward compatibility.
    For new code, prefer using APIMixin directly:

        class MyHandler(APIMixin, StepHandler):
            ...

    Provides HTTP client functionality with automatic error classification,
    retry handling, and convenient methods for common HTTP operations.

    Class Attributes:
        base_url: Base URL for API calls. Can be overridden per-instance.
        default_timeout: Default request timeout in seconds.
        default_headers: Default headers to include in all requests.

    Example:
        >>> class PaymentApiHandler(ApiHandler):
        ...     handler_name = "process_payment"
        ...     base_url = "https://payments.example.com/api/v1"
        ...     default_headers = {"X-API-Key": "secret"}
        ...
        ...     def call(self, context):
        ...         payment_data = context.input_data["payment"]
        ...         response = self.post("/payments", json=payment_data)
        ...         if response.ok:
        ...             return self.api_success(response)
        ...         return self.api_failure(response)
    """

    @property
    def capabilities(self) -> list[str]:
        """Return handler capabilities."""
        return ["process", "http", "api"]
