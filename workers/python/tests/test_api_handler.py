"""API handler tests.

These tests verify:
- ApiHandler class and capabilities
- ApiResponse wrapper and properties
- HTTP error classification (4xx vs 5xx)
- api_failure error typing
- HTTP methods (put, patch, delete, request)
- Error helper methods (connection_error, timeout_error)
- Error classification and message formatting edge cases
"""

from __future__ import annotations

from unittest.mock import Mock

import httpx


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


class TestApiHandlerFailureClassification:
    """Test ApiHandler.api_failure error classification."""

    def test_api_failure_classifies_4xx_as_non_retryable(self):
        """Test ApiHandler.api_failure correctly classifies 4xx HTTP errors as non-retryable."""
        from tasker_core import ApiHandler
        from tasker_core.step_handler.api import ApiResponse

        class TestHandler(ApiHandler):
            handler_name = "test_api"

            def call(self, _context):
                return self.success({})

        handler = TestHandler()

        # Test 400 Bad Request
        mock_response_400 = Mock()
        mock_response_400.status_code = 400
        mock_response_400.headers = {}
        mock_response_400.text = "Bad Request"
        api_response_400 = ApiResponse(mock_response_400)

        result_400 = handler.api_failure(api_response_400)
        assert result_400.is_success is False
        assert result_400.retryable is False
        assert result_400.error_type == "bad_request"

        # Test 401 Unauthorized
        mock_response_401 = Mock()
        mock_response_401.status_code = 401
        mock_response_401.headers = {}
        mock_response_401.text = "Unauthorized"
        api_response_401 = ApiResponse(mock_response_401)

        result_401 = handler.api_failure(api_response_401)
        assert result_401.is_success is False
        assert result_401.retryable is False
        assert result_401.error_type == "unauthorized"

        # Test 403 Forbidden
        mock_response_403 = Mock()
        mock_response_403.status_code = 403
        mock_response_403.headers = {}
        mock_response_403.text = "Forbidden"
        api_response_403 = ApiResponse(mock_response_403)

        result_403 = handler.api_failure(api_response_403)
        assert result_403.is_success is False
        assert result_403.retryable is False
        assert result_403.error_type == "forbidden"

        # Test 404 Not Found
        mock_response_404 = Mock()
        mock_response_404.status_code = 404
        mock_response_404.headers = {}
        mock_response_404.text = "Not Found"
        api_response_404 = ApiResponse(mock_response_404)

        result_404 = handler.api_failure(api_response_404)
        assert result_404.is_success is False
        assert result_404.retryable is False
        assert result_404.error_type == "not_found"

    def test_api_failure_4xx_error_types(self):
        """Test api_failure returns specific error types for different 4xx codes."""
        from tasker_core import ApiHandler
        from tasker_core.step_handler.api import ApiResponse

        class TestHandler(ApiHandler):
            handler_name = "test_api"

            def call(self, _context):
                return self.success({})

        handler = TestHandler()

        # Test 405 Method Not Allowed
        mock_405 = Mock()
        mock_405.status_code = 405
        mock_405.headers = {}
        mock_405.text = "Method Not Allowed"
        result_405 = handler.api_failure(ApiResponse(mock_405))
        assert result_405.error_type == "method_not_allowed"
        assert result_405.retryable is False

        # Test 409 Conflict
        mock_409 = Mock()
        mock_409.status_code = 409
        mock_409.headers = {}
        mock_409.text = "Conflict"
        result_409 = handler.api_failure(ApiResponse(mock_409))
        assert result_409.error_type == "conflict"
        assert result_409.retryable is False

        # Test 410 Gone
        mock_410 = Mock()
        mock_410.status_code = 410
        mock_410.headers = {}
        mock_410.text = "Gone"
        result_410 = handler.api_failure(ApiResponse(mock_410))
        assert result_410.error_type == "gone"
        assert result_410.retryable is False

        # Test 422 Unprocessable Entity
        mock_422 = Mock()
        mock_422.status_code = 422
        mock_422.headers = {}
        mock_422.text = "Unprocessable Entity"
        result_422 = handler.api_failure(ApiResponse(mock_422))
        assert result_422.error_type == "unprocessable_entity"
        assert result_422.retryable is False

    def test_api_failure_includes_status_code_in_metadata(self):
        """Test api_failure includes status code in metadata for 4xx errors."""
        from tasker_core import ApiHandler
        from tasker_core.step_handler.api import ApiResponse

        class TestHandler(ApiHandler):
            handler_name = "test_api"

            def call(self, _context):
                return self.success({})

        handler = TestHandler()

        mock_response = Mock()
        mock_response.status_code = 400
        mock_response.headers = {"content-type": "application/json"}
        mock_response.json.return_value = {"error": "Invalid input"}

        result = handler.api_failure(ApiResponse(mock_response))
        assert result.metadata["status_code"] == 400
        assert result.metadata["response_body"] == {"error": "Invalid input"}


class TestApiResponseAdvanced:
    """Advanced ApiResponse tests for retry_after, body parsing, and explicit body."""

    def test_retry_after_integer(self):
        """Test retry_after parses integer value."""
        from tasker_core.step_handler.api import ApiResponse

        mock_response = Mock()
        mock_response.status_code = 429
        mock_response.headers = {"retry-after": "120"}
        mock_response.text = "Too Many Requests"
        response = ApiResponse(mock_response)
        assert response.retry_after == 120

    def test_retry_after_date_format_returns_default(self):
        """Test retry_after returns 60 for non-integer (date) format."""
        from tasker_core.step_handler.api import ApiResponse

        mock_response = Mock()
        mock_response.status_code = 429
        mock_response.headers = {"retry-after": "Wed, 21 Oct 2025 07:28:00 GMT"}
        mock_response.text = "Rate limited"
        response = ApiResponse(mock_response)
        assert response.retry_after == 60

    def test_retry_after_missing(self):
        """Test retry_after returns None when header is absent."""
        from tasker_core.step_handler.api import ApiResponse

        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.headers = {}
        mock_response.text = "OK"
        response = ApiResponse(mock_response)
        assert response.retry_after is None

    def test_body_parsing_json_failure_fallback(self):
        """Test body falls back to text when JSON parsing fails."""
        from tasker_core.step_handler.api import ApiResponse

        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.headers = {"content-type": "application/json"}
        mock_response.json.side_effect = ValueError("Invalid JSON")
        mock_response.text = "not json"
        response = ApiResponse(mock_response)
        assert response.body == "not json"

    def test_explicit_body_parameter(self):
        """Test ApiResponse with explicit body parameter."""
        from tasker_core.step_handler.api import ApiResponse

        mock_response = Mock()
        mock_response.status_code = 200
        mock_response.headers = {}
        explicit_body = {"custom": "data"}
        response = ApiResponse(mock_response, body=explicit_body)
        assert response.body == {"custom": "data"}


class TestAPIMixinHTTPMethods:
    """Test APIMixin HTTP methods using httpx MockTransport."""

    def _create_handler(self, transport):
        """Create test handler with mock transport."""
        from tasker_core import ApiHandler

        class TestHTTPHandler(ApiHandler):
            handler_name = "test_http"
            base_url = "https://test.example.com"

            def call(self, _context):
                return self.success({})

        handler = TestHTTPHandler()
        handler._client = httpx.Client(base_url="https://test.example.com", transport=transport)
        return handler

    def _mock_transport(self, status_code=200, json_body=None):
        """Create a mock transport returning fixed responses."""

        def handler(_request: httpx.Request):
            body = json_body or {"ok": True}
            return httpx.Response(
                status_code=status_code,
                json=body,
            )

        return httpx.MockTransport(handler)

    def test_put_method(self):
        """Test put() makes PUT request."""
        transport = self._mock_transport(200, {"updated": True})
        handler = self._create_handler(transport)
        response = handler.put("/items/1", json={"name": "new"})
        assert response.ok is True
        assert response.body["updated"] is True
        handler.close()

    def test_patch_method(self):
        """Test patch() makes PATCH request."""
        transport = self._mock_transport(200, {"patched": True})
        handler = self._create_handler(transport)
        response = handler.patch("/items/1", json={"name": "patched"})
        assert response.ok is True
        assert response.body["patched"] is True
        handler.close()

    def test_delete_method(self):
        """Test delete() makes DELETE request."""
        transport = self._mock_transport(204)
        handler = self._create_handler(transport)
        response = handler.delete("/items/1")
        assert response.status_code == 204
        handler.close()

    def test_request_method(self):
        """Test request() makes arbitrary HTTP request."""
        transport = self._mock_transport(200, {"method": "OPTIONS"})
        handler = self._create_handler(transport)
        response = handler.request("OPTIONS", "/items")
        assert response.ok is True
        handler.close()


class TestAPIMixinErrorHelpers:
    """Test APIMixin error helper methods."""

    def _create_handler(self):
        """Create test handler for error helper testing."""
        from tasker_core import ApiHandler

        class TestErrorHandler(ApiHandler):
            handler_name = "test_errors"

            def call(self, _context):
                return self.success({})

        return TestErrorHandler()

    def test_connection_error_without_context(self):
        """Test connection_error() without context string."""
        handler = self._create_handler()
        result = handler.connection_error(ConnectionError("refused"))
        assert result.is_success is False
        assert result.retryable is True
        assert result.error_type == "connection_error"
        assert "Connection error: refused" in result.error_message

    def test_connection_error_with_context(self):
        """Test connection_error() with context string."""
        handler = self._create_handler()
        result = handler.connection_error(ConnectionError("refused"), context="fetching users")
        assert "while fetching users" in result.error_message

    def test_timeout_error_without_context(self):
        """Test timeout_error() without context string."""
        handler = self._create_handler()
        result = handler.timeout_error(TimeoutError("timed out"))
        assert result.is_success is False
        assert result.retryable is True
        assert result.error_type == "timeout"
        assert "Request timeout: timed out" in result.error_message

    def test_timeout_error_with_context(self):
        """Test timeout_error() with context string."""
        handler = self._create_handler()
        result = handler.timeout_error(TimeoutError("timed out"), context="processing data")
        assert "while processing data" in result.error_message

    def test_classify_error_unknown_4xx(self):
        """Test _classify_error for unknown 4xx returns 'client_error'."""
        from tasker_core.step_handler.api import ApiResponse

        handler = self._create_handler()
        mock_response = Mock()
        mock_response.status_code = 418  # I'm a teapot
        mock_response.headers = {}
        mock_response.text = "I'm a teapot"
        response = ApiResponse(mock_response)
        assert handler._classify_error(response) == "client_error"

    def test_classify_error_unknown_5xx(self):
        """Test _classify_error for unknown 5xx returns 'server_error'."""
        from tasker_core.step_handler.api import ApiResponse

        handler = self._create_handler()
        mock_response = Mock()
        mock_response.status_code = 599
        mock_response.headers = {}
        mock_response.text = "Custom Server Error"
        response = ApiResponse(mock_response)
        assert handler._classify_error(response) == "server_error"

    def test_classify_error_non_http(self):
        """Test _classify_error for non-HTTP status returns 'http_error'."""
        from tasker_core.step_handler.api import ApiResponse

        handler = self._create_handler()
        mock_response = Mock()
        mock_response.status_code = 302
        mock_response.headers = {}
        mock_response.text = "Redirect"
        response = ApiResponse(mock_response)
        assert handler._classify_error(response) == "http_error"

    def test_format_error_message_with_error_key(self):
        """Test _format_error_message extracts 'error' from body."""
        from tasker_core.step_handler.api import ApiResponse

        handler = self._create_handler()
        mock_response = Mock()
        mock_response.status_code = 400
        mock_response.headers = {"content-type": "application/json"}
        mock_response.json.return_value = {"error": "Invalid email format"}
        response = ApiResponse(mock_response)
        message = handler._format_error_message(response)
        assert "Invalid email format" in message

    def test_format_error_message_nested_error(self):
        """Test _format_error_message extracts nested error message."""
        from tasker_core.step_handler.api import ApiResponse

        handler = self._create_handler()
        mock_response = Mock()
        mock_response.status_code = 422
        mock_response.headers = {"content-type": "application/json"}
        mock_response.json.return_value = {"error": {"message": "Validation failed"}}
        response = ApiResponse(mock_response)
        message = handler._format_error_message(response)
        assert "Validation failed" in message

    def test_format_error_message_fallback(self):
        """Test _format_error_message uses fallback for unknown body."""
        from tasker_core.step_handler.api import ApiResponse

        handler = self._create_handler()
        mock_response = Mock()
        mock_response.status_code = 500
        mock_response.headers = {"content-type": "text/plain"}
        mock_response.text = "Internal Error"
        response = ApiResponse(mock_response)
        message = handler._format_error_message(response)
        assert "HTTP 500" in message
        assert "Internal Server Error" in message

    def test_api_failure_with_retry_after(self):
        """Test api_failure includes retry_after_seconds in metadata."""
        from tasker_core.step_handler.api import ApiResponse

        handler = self._create_handler()
        mock_response = Mock()
        mock_response.status_code = 429
        mock_response.headers = {"retry-after": "60"}
        mock_response.text = "Rate limited"
        response = ApiResponse(mock_response)
        result = handler.api_failure(response)
        assert result.metadata["retry_after_seconds"] == 60
        assert result.retryable is True

    def test_api_failure_5xx_is_retryable(self):
        """Test api_failure marks 5xx errors as retryable."""
        from tasker_core.step_handler.api import ApiResponse

        handler = self._create_handler()
        mock_response = Mock()
        mock_response.status_code = 503
        mock_response.headers = {}
        mock_response.text = "Service Unavailable"
        response = ApiResponse(mock_response)
        result = handler.api_failure(response)
        assert result.retryable is True
        assert result.error_type == "service_unavailable"
