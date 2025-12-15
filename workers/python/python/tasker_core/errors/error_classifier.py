"""Error classifier for determining retry behavior.

This module provides the ErrorClassifier class that determines whether
an exception should trigger a retry based on its type.

This mirrors the Ruby implementation in:
workers/ruby/lib/tasker_core/errors/error_classifier.rb

Example:
    >>> from tasker_core.errors.error_classifier import ErrorClassifier
    >>>
    >>> classifier = ErrorClassifier()
    >>> classifier.retryable(NetworkError("timeout"))
    True
    >>> classifier.retryable(ValidationError("invalid"))
    False
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from . import (
    AuthenticationError,
    AuthorizationError,
    BusinessLogicError,
    ConfigurationError,
    NetworkError,
    NotFoundError,
    PermanentError,
    RateLimitError,
    ResourceContentionError,
    RetryableError,
    ServiceUnavailableError,
    TimeoutError,
    ValidationError,
)

if TYPE_CHECKING:
    pass


class ErrorClassifier:
    """Classifies exceptions for retry behavior.

    Determines whether an exception should trigger a retry based on:
    1. The exception's own `retryable` attribute (if it has one)
    2. Known permanent error classes (never retry)
    3. Known retryable error classes (always retry)
    4. Standard Python exception patterns
    5. Default behavior (configurable)

    The classifier is designed to be safe by default - unknown errors
    are classified as retryable to avoid data loss.

    Example:
        >>> classifier = ErrorClassifier()
        >>>
        >>> # Tasker errors respect their own retryable flag
        >>> classifier.retryable(RetryableError("temp failure"))
        True
        >>> classifier.retryable(PermanentError("bad data"))
        False
        >>>
        >>> # Standard Python exceptions are classified by type
        >>> classifier.retryable(ConnectionError("network down"))
        True
        >>> classifier.retryable(ValueError("invalid input"))
        False
    """

    # Exceptions that are ALWAYS permanent (never retry)
    PERMANENT_ERROR_CLASSES: tuple[type[Exception], ...] = (
        # Tasker permanent errors
        PermanentError,
        ValidationError,
        NotFoundError,
        AuthenticationError,
        AuthorizationError,
        ConfigurationError,
        BusinessLogicError,
        # Standard Python errors that indicate bad input/code
        ValueError,
        TypeError,
        KeyError,
        AttributeError,
        IndexError,
        AssertionError,
        SyntaxError,
        NameError,
        ImportError,
        NotImplementedError,
    )

    # Exceptions that are ALWAYS retryable
    RETRYABLE_ERROR_CLASSES: tuple[type[Exception], ...] = (
        # Tasker retryable errors
        RetryableError,
        TimeoutError,
        NetworkError,
        RateLimitError,
        ServiceUnavailableError,
        ResourceContentionError,
        # Standard Python errors that might be transient
        ConnectionError,
        ConnectionRefusedError,
        ConnectionResetError,
        ConnectionAbortedError,
        BrokenPipeError,
        TimeoutError,  # Built-in TimeoutError
        BlockingIOError,
        InterruptedError,
        # OS-level transient errors
        OSError,
        IOError,
    )

    def __init__(self, *, default_retryable: bool = True) -> None:
        """Initialize the classifier.

        Args:
            default_retryable: Default behavior for unknown exceptions.
                              True (default) is safer - unknown errors
                              get retried rather than lost.
        """
        self._default_retryable = default_retryable

    def retryable(self, exception: BaseException) -> bool:
        """Determine if an exception should trigger a retry.

        Classification order:
        1. If exception has `retryable` attribute, use that
        2. Check against PERMANENT_ERROR_CLASSES (return False)
        3. Check against RETRYABLE_ERROR_CLASSES (return True)
        4. Return default_retryable

        Args:
            exception: The exception to classify

        Returns:
            True if the error should be retried, False otherwise

        Example:
            >>> classifier = ErrorClassifier()
            >>> classifier.retryable(NetworkError("timeout"))
            True
            >>> classifier.retryable(ValidationError("bad input"))
            False
        """
        # Check for explicit retryable attribute first
        if hasattr(exception, "retryable"):
            return bool(exception.retryable)

        # Check permanent errors (order matters - check permanent first)
        if isinstance(exception, self.PERMANENT_ERROR_CLASSES):
            return False

        # Check retryable errors
        if isinstance(exception, self.RETRYABLE_ERROR_CLASSES):
            return True

        # Default behavior for unknown exceptions
        return self._default_retryable

    def permanent(self, exception: BaseException) -> bool:
        """Determine if an exception is permanent (should not retry).

        Convenience method that returns the opposite of retryable().

        Args:
            exception: The exception to classify

        Returns:
            True if the error should NOT be retried

        Example:
            >>> classifier = ErrorClassifier()
            >>> classifier.permanent(ValidationError("bad input"))
            True
        """
        return not self.retryable(exception)

    def classify(self, exception: BaseException) -> dict[str, bool | str]:
        """Classify an exception and return full details.

        Args:
            exception: The exception to classify

        Returns:
            Dictionary with classification details:
            - error_type: Exception class name
            - retryable: Whether to retry
            - classification: How the classification was determined

        Example:
            >>> classifier = ErrorClassifier()
            >>> classifier.classify(NetworkError("timeout"))
            {'error_type': 'NetworkError', 'retryable': True, 'classification': 'retryable_class'}
        """
        error_type = type(exception).__name__

        # Check for explicit retryable attribute
        if hasattr(exception, "retryable"):
            return {
                "error_type": error_type,
                "retryable": bool(exception.retryable),
                "classification": "explicit_attribute",
            }

        # Check permanent errors
        if isinstance(exception, self.PERMANENT_ERROR_CLASSES):
            return {
                "error_type": error_type,
                "retryable": False,
                "classification": "permanent_class",
            }

        # Check retryable errors
        if isinstance(exception, self.RETRYABLE_ERROR_CLASSES):
            return {
                "error_type": error_type,
                "retryable": True,
                "classification": "retryable_class",
            }

        # Default
        return {
            "error_type": error_type,
            "retryable": self._default_retryable,
            "classification": "default",
        }


# Singleton instance for convenience
_default_classifier: ErrorClassifier | None = None


def get_classifier() -> ErrorClassifier:
    """Get the default error classifier instance.

    Returns:
        The singleton ErrorClassifier instance

    Example:
        >>> classifier = get_classifier()
        >>> classifier.retryable(NetworkError("timeout"))
        True
    """
    global _default_classifier
    if _default_classifier is None:
        _default_classifier = ErrorClassifier()
    return _default_classifier


def is_retryable(exception: BaseException) -> bool:
    """Convenience function to check if an exception is retryable.

    Args:
        exception: The exception to check

    Returns:
        True if the exception should be retried

    Example:
        >>> from tasker_core.errors.error_classifier import is_retryable
        >>> is_retryable(NetworkError("timeout"))
        True
    """
    return get_classifier().retryable(exception)


def is_permanent(exception: BaseException) -> bool:
    """Convenience function to check if an exception is permanent.

    Args:
        exception: The exception to check

    Returns:
        True if the exception should NOT be retried

    Example:
        >>> from tasker_core.errors.error_classifier import is_permanent
        >>> is_permanent(ValidationError("bad input"))
        True
    """
    return get_classifier().permanent(exception)


__all__ = [
    "ErrorClassifier",
    "get_classifier",
    "is_retryable",
    "is_permanent",
]
