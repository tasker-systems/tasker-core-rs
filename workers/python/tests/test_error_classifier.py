"""Tests for error classifier module.

These tests verify:
- ErrorClassifier retryable/permanent classification
- classify() return structure
- Singleton get_classifier() and convenience functions
- default_retryable constructor override
- Explicit retryable attribute on exceptions
"""

from __future__ import annotations

import pytest

from tasker_core.errors import (
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
    ValidationError,
)
from tasker_core.errors import TimeoutError as TaskerTimeoutError
from tasker_core.errors.error_classifier import (
    ErrorClassifier,
    get_classifier,
    is_permanent,
    is_retryable,
)


class TestErrorClassifierRetryable:
    """Test retryable classification for known error types."""

    @pytest.mark.parametrize(
        "exception",
        [
            RetryableError("transient"),
            TaskerTimeoutError("timed out"),
            NetworkError("network down"),
            RateLimitError("too many requests"),
            ServiceUnavailableError("service down"),
            ResourceContentionError("lock conflict"),
        ],
    )
    def test_tasker_retryable_errors(self, exception):
        """Tasker retryable error subclasses are classified as retryable."""
        classifier = ErrorClassifier()
        assert classifier.retryable(exception) is True

    @pytest.mark.parametrize(
        "exception",
        [
            ConnectionError("refused"),
            ConnectionRefusedError("refused"),
            ConnectionResetError("reset"),
            ConnectionAbortedError("aborted"),
            BrokenPipeError("broken pipe"),
            TimeoutError("builtin timeout"),
            BlockingIOError("blocking"),
            InterruptedError("interrupted"),
            OSError("os error"),
            OSError("io error"),
        ],
    )
    def test_stdlib_retryable_errors(self, exception):
        """Standard library transient errors are classified as retryable."""
        classifier = ErrorClassifier()
        assert classifier.retryable(exception) is True


class TestErrorClassifierPermanent:
    """Test permanent classification for known error types."""

    @pytest.mark.parametrize(
        "exception",
        [
            PermanentError("permanent"),
            ValidationError("invalid"),
            NotFoundError("missing"),
            AuthenticationError("auth failed"),
            AuthorizationError("forbidden"),
            ConfigurationError("bad config"),
            BusinessLogicError("business rule"),
        ],
    )
    def test_tasker_permanent_errors(self, exception):
        """Tasker permanent error subclasses are classified as permanent."""
        classifier = ErrorClassifier()
        assert classifier.retryable(exception) is False
        assert classifier.permanent(exception) is True

    @pytest.mark.parametrize(
        "exception",
        [
            ValueError("bad value"),
            TypeError("bad type"),
            KeyError("missing key"),
            AttributeError("no attr"),
            IndexError("out of range"),
            AssertionError("assert failed"),
            SyntaxError("syntax"),
            NameError("undefined"),
            ImportError("no module"),
            NotImplementedError("not impl"),
        ],
    )
    def test_stdlib_permanent_errors(self, exception):
        """Standard library permanent errors are classified as permanent."""
        classifier = ErrorClassifier()
        assert classifier.retryable(exception) is False

    def test_permanent_is_inverse_of_retryable(self):
        """permanent() returns the inverse of retryable()."""
        classifier = ErrorClassifier()
        retryable_err = NetworkError("network down")
        permanent_err = ValidationError("invalid")

        assert classifier.permanent(retryable_err) is not classifier.retryable(retryable_err)
        assert classifier.permanent(permanent_err) is not classifier.retryable(permanent_err)


class TestErrorClassifierClassify:
    """Test classify() return structure."""

    def test_classify_retryable_class(self):
        """classify() returns retryable_class for known retryable errors."""
        classifier = ErrorClassifier()
        result = classifier.classify(NetworkError("network"))
        assert result["error_type"] == "NetworkError"
        assert result["retryable"] is True
        assert result["classification"] == "explicit_attribute"

    def test_classify_permanent_class(self):
        """classify() returns permanent_class for known permanent errors."""
        classifier = ErrorClassifier()
        result = classifier.classify(ValueError("bad"))
        assert result["error_type"] == "ValueError"
        assert result["retryable"] is False
        assert result["classification"] == "permanent_class"

    def test_classify_explicit_attribute(self):
        """classify() returns explicit_attribute when exception has retryable attr."""
        classifier = ErrorClassifier()
        err = RetryableError("has attr")
        result = classifier.classify(err)
        assert result["classification"] == "explicit_attribute"
        assert result["retryable"] is True

    def test_classify_default(self):
        """classify() returns default for unknown exceptions."""
        classifier = ErrorClassifier()

        class CustomError(Exception):
            pass

        result = classifier.classify(CustomError("custom"))
        assert result["error_type"] == "CustomError"
        assert result["retryable"] is True
        assert result["classification"] == "default"


class TestErrorClassifierSingleton:
    """Test singleton and convenience functions."""

    def teardown_method(self):
        """Reset module-level singleton after each test."""
        import tasker_core.errors.error_classifier as mod

        mod._default_classifier = None

    def test_get_classifier_returns_instance(self):
        """get_classifier() returns an ErrorClassifier instance."""
        classifier = get_classifier()
        assert isinstance(classifier, ErrorClassifier)

    def test_get_classifier_is_singleton(self):
        """get_classifier() returns the same instance on repeated calls."""
        c1 = get_classifier()
        c2 = get_classifier()
        assert c1 is c2

    def test_is_retryable_convenience(self):
        """is_retryable() delegates to singleton classifier."""
        assert is_retryable(NetworkError("net")) is True
        assert is_retryable(ValueError("val")) is False

    def test_is_permanent_convenience(self):
        """is_permanent() delegates to singleton classifier."""
        assert is_permanent(ValidationError("val")) is True
        assert is_permanent(ConnectionError("conn")) is False


class TestErrorClassifierDefaultOverride:
    """Test default_retryable constructor override."""

    def test_default_retryable_true(self):
        """Unknown exceptions are retryable when default_retryable=True."""
        classifier = ErrorClassifier(default_retryable=True)

        class UnknownError(Exception):
            pass

        assert classifier.retryable(UnknownError("unknown")) is True

    def test_default_retryable_false(self):
        """Unknown exceptions are permanent when default_retryable=False."""
        classifier = ErrorClassifier(default_retryable=False)

        class UnknownError(Exception):
            pass

        assert classifier.retryable(UnknownError("unknown")) is False

    def test_explicit_retryable_attribute_overrides_classification(self):
        """Exception with explicit retryable attribute overrides type-based classification."""
        classifier = ErrorClassifier()

        class CustomException(Exception):
            retryable = False

        err = CustomException("custom")
        assert classifier.retryable(err) is False

        class CustomRetryable(Exception):
            retryable = True

        err2 = CustomRetryable("custom")
        assert classifier.retryable(err2) is True
