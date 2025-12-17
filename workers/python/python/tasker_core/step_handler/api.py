"""API handler for HTTP interactions.

This module provides the ApiHandler base class for step handlers that
need to make HTTP API calls. It includes automatic error classification,
retry-after header handling, and convenient HTTP method wrappers.

Example:
    >>> from tasker_core.step_handler import ApiHandler
    >>> from tasker_core import StepContext, StepHandlerResult
    >>>
    >>> class FetchUserHandler(ApiHandler):
    ...     handler_name = "fetch_user"
    ...     base_url = "https://api.example.com"
    ...
    ...     def call(self, context: StepContext) -> StepHandlerResult:
    ...         user_id = context.input_data["user_id"]
    ...         response = self.get(f"/users/{user_id}")
    ...         return self.api_success(response)
"""

from __future__ import annotations

from typing import TYPE_CHECKING, Any

import httpx

from tasker_core.step_handler.base import StepHandler

if TYPE_CHECKING:
    from tasker_core.types import StepHandlerResult


# HTTP status code classifications
CLIENT_ERROR_CODES = range(400, 500)
SERVER_ERROR_CODES = range(500, 600)

# Status codes that should not be retried
NON_RETRYABLE_STATUS_CODES = {
    400,  # Bad Request - client error
    401,  # Unauthorized - auth required
    403,  # Forbidden - no permission
    404,  # Not Found - resource doesn't exist
    405,  # Method Not Allowed
    406,  # Not Acceptable
    410,  # Gone - resource permanently removed
    422,  # Unprocessable Entity - validation error
}

# Status codes that indicate temporary failures (should retry)
RETRYABLE_STATUS_CODES = {
    408,  # Request Timeout
    429,  # Too Many Requests (rate limit)
    500,  # Internal Server Error
    502,  # Bad Gateway
    503,  # Service Unavailable
    504,  # Gateway Timeout
}


class ApiResponse:
    """Response wrapper for API calls.

    Provides convenient access to response data and error classification.

    Attributes:
        status_code: HTTP status code.
        headers: Response headers.
        body: Response body (parsed JSON or raw text).
        raw_response: The underlying httpx.Response object.
    """

    def __init__(
        self,
        response: httpx.Response,
        body: dict[str, Any] | str | None = None,
    ) -> None:
        """Initialize the API response wrapper.

        Args:
            response: The httpx Response object.
            body: Parsed response body (optional).
        """
        self.status_code: int = response.status_code
        self.headers: dict[str, str] = dict(response.headers)
        self.raw_response: httpx.Response = response

        # Parse body if not provided
        if body is not None:
            self.body = body
        else:
            content_type = response.headers.get("content-type", "")
            if "application/json" in content_type:
                try:
                    self.body: dict[str, Any] | str | None = response.json()
                except ValueError:
                    self.body = response.text
            else:
                self.body = response.text

    @property
    def ok(self) -> bool:
        """Check if the response indicates success (2xx status)."""
        return 200 <= self.status_code < 300

    @property
    def is_client_error(self) -> bool:
        """Check if the response indicates a client error (4xx status)."""
        return self.status_code in CLIENT_ERROR_CODES

    @property
    def is_server_error(self) -> bool:
        """Check if the response indicates a server error (5xx status)."""
        return self.status_code in SERVER_ERROR_CODES

    @property
    def is_retryable(self) -> bool:
        """Check if the error should be retried."""
        return self.status_code in RETRYABLE_STATUS_CODES

    @property
    def retry_after(self) -> int | None:
        """Get the Retry-After header value in seconds, if present."""
        retry_after = self.headers.get("retry-after")
        if retry_after is None:
            return None
        try:
            return int(retry_after)
        except ValueError:
            # Could be a date format, return a default
            return 60

    def to_dict(self) -> dict[str, Any]:
        """Convert the response to a dictionary for result output."""
        return {
            "status_code": self.status_code,
            "headers": self.headers,
            "body": self.body,
        }


class ApiHandler(StepHandler):
    """Base class for HTTP API step handlers.

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
        ...     def call(self, context: StepContext) -> StepHandlerResult:
        ...         payment_data = context.input_data["payment"]
        ...         response = self.post("/payments", json=payment_data)
        ...         if response.ok:
        ...             return self.api_success(response)
        ...         return self.api_failure(response)
    """

    # Class attributes - can be overridden by subclasses
    base_url: str = ""
    default_timeout: float = 30.0
    default_headers: dict[str, str] = {}

    def __init__(self) -> None:
        """Initialize the ApiHandler."""
        super().__init__() if hasattr(super(), "__init__") else None
        self._client: httpx.Client | None = None

    @property
    def capabilities(self) -> list[str]:
        """Return handler capabilities."""
        return ["process", "http", "api"]

    @property
    def client(self) -> httpx.Client:
        """Get or create the HTTP client.

        Returns:
            A configured httpx Client instance.
        """
        if self._client is None:
            self._client = httpx.Client(
                base_url=self.base_url,
                timeout=self.default_timeout,
                headers=self.default_headers,
            )
        return self._client

    def close(self) -> None:
        """Close the HTTP client."""
        if self._client is not None:
            self._client.close()
            self._client = None

    def __del__(self) -> None:
        """Clean up resources on deletion."""
        self.close()

    # =========================================================================
    # HTTP Methods
    # =========================================================================

    def get(
        self,
        path: str,
        params: dict[str, Any] | None = None,
        headers: dict[str, str] | None = None,
        **kwargs: Any,
    ) -> ApiResponse:
        """Make a GET request.

        Args:
            path: URL path (appended to base_url).
            params: Query parameters.
            headers: Additional headers.
            **kwargs: Additional arguments passed to httpx.get().

        Returns:
            ApiResponse wrapping the response.

        Example:
            >>> response = self.get("/users", params={"page": 1})
            >>> if response.ok:
            ...     users = response.body["data"]
        """
        response = self.client.get(path, params=params, headers=headers, **kwargs)
        return ApiResponse(response)

    def post(
        self,
        path: str,
        json: dict[str, Any] | None = None,
        data: dict[str, Any] | None = None,
        headers: dict[str, str] | None = None,
        **kwargs: Any,
    ) -> ApiResponse:
        """Make a POST request.

        Args:
            path: URL path (appended to base_url).
            json: JSON body data.
            data: Form data.
            headers: Additional headers.
            **kwargs: Additional arguments passed to httpx.post().

        Returns:
            ApiResponse wrapping the response.

        Example:
            >>> response = self.post("/users", json={"name": "Alice"})
            >>> if response.ok:
            ...     user_id = response.body["id"]
        """
        response = self.client.post(path, json=json, data=data, headers=headers, **kwargs)
        return ApiResponse(response)

    def put(
        self,
        path: str,
        json: dict[str, Any] | None = None,
        data: dict[str, Any] | None = None,
        headers: dict[str, str] | None = None,
        **kwargs: Any,
    ) -> ApiResponse:
        """Make a PUT request.

        Args:
            path: URL path (appended to base_url).
            json: JSON body data.
            data: Form data.
            headers: Additional headers.
            **kwargs: Additional arguments passed to httpx.put().

        Returns:
            ApiResponse wrapping the response.

        Example:
            >>> response = self.put("/users/123", json={"name": "Bob"})
        """
        response = self.client.put(path, json=json, data=data, headers=headers, **kwargs)
        return ApiResponse(response)

    def patch(
        self,
        path: str,
        json: dict[str, Any] | None = None,
        data: dict[str, Any] | None = None,
        headers: dict[str, str] | None = None,
        **kwargs: Any,
    ) -> ApiResponse:
        """Make a PATCH request.

        Args:
            path: URL path (appended to base_url).
            json: JSON body data.
            data: Form data.
            headers: Additional headers.
            **kwargs: Additional arguments passed to httpx.patch().

        Returns:
            ApiResponse wrapping the response.
        """
        response = self.client.patch(path, json=json, data=data, headers=headers, **kwargs)
        return ApiResponse(response)

    def delete(
        self,
        path: str,
        headers: dict[str, str] | None = None,
        **kwargs: Any,
    ) -> ApiResponse:
        """Make a DELETE request.

        Args:
            path: URL path (appended to base_url).
            headers: Additional headers.
            **kwargs: Additional arguments passed to httpx.delete().

        Returns:
            ApiResponse wrapping the response.

        Example:
            >>> response = self.delete("/users/123")
            >>> if response.status_code == 204:
            ...     print("Deleted successfully")
        """
        response = self.client.delete(path, headers=headers, **kwargs)
        return ApiResponse(response)

    def request(
        self,
        method: str,
        path: str,
        **kwargs: Any,
    ) -> ApiResponse:
        """Make an arbitrary HTTP request.

        Args:
            method: HTTP method (GET, POST, PUT, etc.).
            path: URL path (appended to base_url).
            **kwargs: Arguments passed to httpx.request().

        Returns:
            ApiResponse wrapping the response.
        """
        response = self.client.request(method, path, **kwargs)
        return ApiResponse(response)

    # =========================================================================
    # Result Helpers
    # =========================================================================

    def api_success(
        self,
        response: ApiResponse,
        result: dict[str, Any] | None = None,
        include_response: bool = True,
    ) -> StepHandlerResult:
        """Create a success result from an API response.

        Args:
            response: The API response.
            result: Optional custom result data. If not provided, uses response body.
            include_response: Whether to include response metadata.

        Returns:
            A success StepHandlerResult.

        Example:
            >>> response = self.get("/data")
            >>> if response.ok:
            ...     return self.api_success(response)
        """
        if result is None:
            result = response.body if isinstance(response.body, dict) else {"data": response.body}

        metadata: dict[str, Any] = {}
        if include_response:
            metadata["status_code"] = response.status_code
            metadata["headers"] = response.headers

        return self.success(result, metadata=metadata)

    def api_failure(
        self,
        response: ApiResponse,
        message: str | None = None,
    ) -> StepHandlerResult:
        """Create a failure result from an API response.

        Automatically classifies the error type based on the HTTP status code
        and determines if the error is retryable.

        Args:
            response: The API response.
            message: Optional custom error message.

        Returns:
            A failure StepHandlerResult with appropriate error classification.

        Example:
            >>> response = self.post("/payments", json=data)
            >>> if not response.ok:
            ...     return self.api_failure(response)
        """
        # Determine error type from status code
        error_type = self._classify_error(response)

        # Generate error message if not provided
        if message is None:
            message = self._format_error_message(response)

        # Determine if retryable
        retryable = response.is_retryable

        # Build metadata
        metadata: dict[str, Any] = {
            "status_code": response.status_code,
            "headers": response.headers,
        }

        # Include retry-after if present
        if response.retry_after is not None:
            metadata["retry_after_seconds"] = response.retry_after

        # Include response body for debugging
        if response.body:
            metadata["response_body"] = response.body

        return self.failure(
            message=message,
            error_type=error_type,
            retryable=retryable,
            metadata=metadata,
        )

    def _classify_error(self, response: ApiResponse) -> str:
        """Classify an error based on HTTP status code.

        Args:
            response: The API response.

        Returns:
            Error type string for classification.
        """
        status_code = response.status_code

        # Map specific status codes to error types
        error_type_map = {
            400: "bad_request",
            401: "unauthorized",
            403: "forbidden",
            404: "not_found",
            405: "method_not_allowed",
            408: "request_timeout",
            409: "conflict",
            410: "gone",
            422: "unprocessable_entity",
            429: "rate_limited",
            500: "internal_server_error",
            502: "bad_gateway",
            503: "service_unavailable",
            504: "gateway_timeout",
        }

        if status_code in error_type_map:
            return error_type_map[status_code]

        if status_code in CLIENT_ERROR_CODES:
            return "client_error"

        if status_code in SERVER_ERROR_CODES:
            return "server_error"

        return "http_error"

    def _format_error_message(self, response: ApiResponse) -> str:
        """Format an error message from an API response.

        Args:
            response: The API response.

        Returns:
            Human-readable error message.
        """
        status_code = response.status_code

        # Try to extract error message from response body
        if isinstance(response.body, dict):
            # Common error response formats
            for key in ["error", "message", "detail", "error_message", "msg"]:
                if key in response.body:
                    error_detail = response.body[key]
                    if isinstance(error_detail, str):
                        return f"HTTP {status_code}: {error_detail}"
                    elif isinstance(error_detail, dict) and "message" in error_detail:
                        return f"HTTP {status_code}: {error_detail['message']}"

        # Generic message based on status code
        status_messages = {
            400: "Bad Request",
            401: "Unauthorized",
            403: "Forbidden",
            404: "Not Found",
            405: "Method Not Allowed",
            408: "Request Timeout",
            409: "Conflict",
            410: "Gone",
            422: "Unprocessable Entity",
            429: "Too Many Requests",
            500: "Internal Server Error",
            502: "Bad Gateway",
            503: "Service Unavailable",
            504: "Gateway Timeout",
        }

        message = status_messages.get(status_code, "HTTP Error")
        return f"HTTP {status_code}: {message}"

    def connection_error(
        self,
        error: Exception,
        context: str | None = None,
    ) -> StepHandlerResult:
        """Create a failure result from a connection error.

        Connection errors are typically retryable.

        Args:
            error: The connection exception.
            context: Optional context about the request.

        Returns:
            A retryable failure StepHandlerResult.

        Example:
            >>> try:
            ...     response = self.get("/endpoint")
            ... except httpx.ConnectError as e:
            ...     return self.connection_error(e, "fetching user data")
        """
        message = f"Connection error: {error}"
        if context:
            message = f"Connection error while {context}: {error}"

        return self.failure(
            message=message,
            error_type="connection_error",
            retryable=True,
            metadata={"exception_type": type(error).__name__},
        )

    def timeout_error(
        self,
        error: Exception,
        context: str | None = None,
    ) -> StepHandlerResult:
        """Create a failure result from a timeout error.

        Timeout errors are typically retryable.

        Args:
            error: The timeout exception.
            context: Optional context about the request.

        Returns:
            A retryable failure StepHandlerResult.
        """
        message = f"Request timeout: {error}"
        if context:
            message = f"Request timeout while {context}: {error}"

        return self.failure(
            message=message,
            error_type="timeout",
            retryable=True,
            metadata={"exception_type": type(error).__name__},
        )


__all__ = ["ApiHandler", "ApiResponse"]
