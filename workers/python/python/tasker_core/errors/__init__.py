"""Error classes for Tasker Core Python worker.

This module provides a hierarchy of error classes that mirror the Ruby
implementation in workers/ruby/lib/tasker_core/errors/common.rb.

The error hierarchy supports automatic retry classification:
- RetryableError: Transient failures that should be retried
- PermanentError: Failures that will not succeed on retry

Example:
    >>> from tasker_core.errors import RetryableError, PermanentError
    >>>
    >>> # Raise a retryable error
    >>> raise NetworkError("Connection timeout")
    >>>
    >>> # Raise a permanent error
    >>> raise ValidationError("Invalid input format")
"""

from __future__ import annotations

from typing import Any


class TaskerError(Exception):
    """Base class for all Tasker errors.

    Attributes:
        message: Human-readable error message
        retryable: Whether this error should trigger a retry
        metadata: Additional error context
    """

    retryable: bool = True

    def __init__(
        self,
        message: str,
        *,
        retryable: bool | None = None,
        metadata: dict[str, Any] | None = None,
    ) -> None:
        """Initialize the error.

        Args:
            message: Error message
            retryable: Override default retryability
            metadata: Additional context
        """
        super().__init__(message)
        self.message = message
        if retryable is not None:
            self.retryable = retryable
        self.metadata = metadata or {}

    def to_dict(self) -> dict[str, Any]:
        """Convert to dictionary for FFI serialization.

        Returns:
            Dictionary with error details
        """
        return {
            "error_type": self.__class__.__name__,
            "message": self.message,
            "retryable": self.retryable,
            "metadata": self.metadata,
        }


class RetryableError(TaskerError):
    """Base class for errors that should be retried.

    Use this for transient failures where a retry might succeed:
    - Network timeouts
    - Service temporarily unavailable
    - Resource contention
    - Rate limiting

    Example:
        >>> raise RetryableError("Service temporarily unavailable")
    """

    retryable: bool = True


class PermanentError(TaskerError):
    """Base class for errors that should NOT be retried.

    Use this for failures that will not succeed on retry:
    - Validation errors
    - Missing required data
    - Authentication failures
    - Business logic violations

    Example:
        >>> raise PermanentError("Invalid account number")
    """

    retryable: bool = False


# Retryable error subclasses


class TimeoutError(RetryableError):
    """Operation timed out.

    The operation took too long and was terminated. A retry might
    succeed if the system is less loaded.

    Example:
        >>> raise TimeoutError("Database query exceeded 30s limit")
    """

    pass


class NetworkError(RetryableError):
    """Network-related failure.

    Connection failed, DNS resolution failed, etc. A retry might
    succeed once network conditions improve.

    Example:
        >>> raise NetworkError("Failed to connect to payment gateway")
    """

    pass


class RateLimitError(RetryableError):
    """Rate limit exceeded.

    Too many requests to an external service. A retry after backoff
    should succeed.

    Example:
        >>> raise RateLimitError("API rate limit exceeded, retry after 60s")
    """

    pass


class ServiceUnavailableError(RetryableError):
    """External service is temporarily unavailable.

    The service is down or overloaded. A retry after some time
    should succeed.

    Example:
        >>> raise ServiceUnavailableError("Payment service in maintenance mode")
    """

    pass


class ResourceContentionError(RetryableError):
    """Resource contention or lock conflict.

    Could not acquire a required lock or resource. A retry might
    succeed when the resource is available.

    Example:
        >>> raise ResourceContentionError("Could not acquire order lock")
    """

    pass


# Permanent error subclasses


class ValidationError(PermanentError):
    """Input validation failed.

    The input data does not meet requirements. Retrying with the
    same data will always fail.

    Example:
        >>> raise ValidationError("Email address format invalid")
    """

    pass


class NotFoundError(PermanentError):
    """Required resource not found.

    A required entity does not exist. Retrying will not create it.

    Example:
        >>> raise NotFoundError("Order 12345 does not exist")
    """

    pass


class AuthenticationError(PermanentError):
    """Authentication failed.

    Invalid credentials or token. Retrying with the same credentials
    will always fail.

    Example:
        >>> raise AuthenticationError("Invalid API key")
    """

    pass


class AuthorizationError(PermanentError):
    """Authorization failed.

    User does not have permission. Retrying will not grant permission.

    Example:
        >>> raise AuthorizationError("User lacks admin privileges")
    """

    pass


class ConfigurationError(PermanentError):
    """Configuration error.

    System is misconfigured. Retrying without fixing config will fail.

    Example:
        >>> raise ConfigurationError("Missing required DATABASE_URL")
    """

    pass


class BusinessLogicError(PermanentError):
    """Business rule violation.

    The operation violates business rules. Retrying will always fail.

    Example:
        >>> raise BusinessLogicError("Cannot refund more than original amount")
    """

    pass


class FFIError(TaskerError):
    """Error in FFI layer communication.

    The retryability depends on the nature of the FFI failure.
    Serialization errors are typically permanent, while communication
    errors might be retryable.

    Example:
        >>> raise FFIError("Failed to serialize result", retryable=False)
    """

    retryable: bool = True  # Default to retryable, can be overridden


__all__ = [
    # Base classes
    "TaskerError",
    "RetryableError",
    "PermanentError",
    # Retryable errors
    "TimeoutError",
    "NetworkError",
    "RateLimitError",
    "ServiceUnavailableError",
    "ResourceContentionError",
    # Permanent errors
    "ValidationError",
    "NotFoundError",
    "AuthenticationError",
    "AuthorizationError",
    "ConfigurationError",
    "BusinessLogicError",
    # Special
    "FFIError",
]
