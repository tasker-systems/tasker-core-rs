"""Comprehensive domain events lifecycle tests.

Tests for BasePublisher, BaseSubscriber, PublisherRegistry, and SubscriberRegistry
lifecycle hooks following Ruby's testing patterns (TAS-112).

This module provides comprehensive coverage of:
- Publisher lifecycle hooks (should_publish, transform_payload, before_publish, etc.)
- Subscriber lifecycle hooks (before_handle, handle, after_handle, on_handle_error)
- Registry operations (register, get, freeze, validate)
- Integration patterns

Test patterns adapted from:
- workers/ruby/spec/domain_events/base_publisher_spec.rb
- workers/ruby/spec/domain_events/publisher_registry_spec.rb
- workers/ruby/spec/domain_events/subscriber_registry_spec.rb
"""

from __future__ import annotations

from dataclasses import dataclass, field
from typing import Any
from unittest.mock import MagicMock
from uuid import uuid4

import pytest

from tasker_core.domain_events import (
    BasePublisher,
    BaseSubscriber,
    DefaultPublisher,
    EventDeclaration,
    InProcessDomainEventPoller,
    PublishContext,
    PublisherRegistry,
    StepEventContext,
    StepResult,
    SubscriberRegistry,
)

# =============================================================================
# Test Helpers - Matching Ruby's TestHelpers::EventFactory
# =============================================================================


@dataclass
class MockEventPublisher(BasePublisher):
    """Mock publisher that captures events for testing.

    Matches Ruby's TaskerCore::TestHelpers::MockEventPublisher.
    """

    _name: str = "MockEventPublisher"
    published_events: list[dict[str, Any]] = field(default_factory=list)
    should_fail: bool = False
    failure_error: Exception | None = None

    # Hook tracking
    should_publish_called: bool = False
    transform_payload_called: bool = False
    additional_metadata_called: bool = False
    before_publish_called: bool = False
    after_publish_called: bool = False
    on_publish_error_called: bool = False

    # Last call arguments
    last_event_name: str | None = None
    last_payload: dict[str, Any] | None = None
    last_metadata: dict[str, Any] | None = None

    @property
    def publisher_name(self) -> str:
        return self._name

    def should_publish(self, _ctx: StepEventContext) -> bool:
        self.should_publish_called = True
        return True

    def transform_payload(self, ctx: StepEventContext) -> dict[str, Any]:
        self.transform_payload_called = True
        return ctx.result or {}

    def additional_metadata(self, _ctx: StepEventContext) -> dict[str, Any]:
        self.additional_metadata_called = True
        return {"mock_publisher": True}

    def before_publish(
        self,
        event_name: str,
        payload: dict[str, Any],
        metadata: dict[str, Any],
    ) -> bool:
        self.before_publish_called = True
        self.last_event_name = event_name
        self.last_payload = payload
        self.last_metadata = metadata

        if self.should_fail:
            raise self.failure_error or Exception("Simulated failure")

        self.published_events.append(
            {
                "event_name": event_name,
                "payload": payload,
                "metadata": metadata,
            }
        )
        return True

    def after_publish(
        self,
        _event_name: str,
        _payload: dict[str, Any],
        _metadata: dict[str, Any],
    ) -> None:
        self.after_publish_called = True

    def on_publish_error(
        self,
        _event_name: str,
        _error: Exception,
        _payload: dict[str, Any],
    ) -> None:
        self.on_publish_error_called = True

    def simulate_failure(self, error: Exception | None = None) -> None:
        """Enable failure simulation."""
        self.should_fail = True
        self.failure_error = error or Exception("Simulated failure")

    def clear(self) -> None:
        """Clear all captured events and reset state."""
        self.published_events.clear()
        self.should_publish_called = False
        self.transform_payload_called = False
        self.additional_metadata_called = False
        self.before_publish_called = False
        self.after_publish_called = False
        self.on_publish_error_called = False
        self.should_fail = False
        self.failure_error = None

    def events_by_name(self, name: str) -> list[dict[str, Any]]:
        """Get all events with matching name."""
        return [e for e in self.published_events if e["event_name"] == name]


class SkippingPublisher(BasePublisher):
    """Publisher that always skips publishing."""

    @property
    def publisher_name(self) -> str:
        return "SkippingPublisher"

    def should_publish(self, _ctx: StepEventContext) -> bool:
        return False


class FailingBeforePublishPublisher(BasePublisher):
    """Publisher that fails in before_publish hook."""

    error_handled: bool = False

    @property
    def publisher_name(self) -> str:
        return "FailingBeforePublishPublisher"

    def before_publish(
        self,
        _event_name: str,
        _payload: dict[str, Any],
        _metadata: dict[str, Any],
    ) -> bool:
        raise ValueError("Pre-publish validation failed")

    def on_publish_error(
        self,
        _event_name: str,
        _error: Exception,
        _payload: dict[str, Any],
    ) -> None:
        self.error_handled = True


class AbortingPublisher(BasePublisher):
    """Publisher that aborts in before_publish by returning False."""

    before_publish_called: bool = False
    after_publish_called: bool = False

    @property
    def publisher_name(self) -> str:
        return "AbortingPublisher"

    def before_publish(
        self,
        _event_name: str,
        _payload: dict[str, Any],
        _metadata: dict[str, Any],
    ) -> bool:
        self.before_publish_called = True
        return False  # Abort publishing

    def after_publish(
        self,
        _event_name: str,
        _payload: dict[str, Any],
        _metadata: dict[str, Any],
    ) -> None:
        self.after_publish_called = True


class TransformingPublisher(BasePublisher):
    """Publisher that transforms payloads."""

    @property
    def publisher_name(self) -> str:
        return "TransformingPublisher"

    def transform_payload(self, ctx: StepEventContext) -> dict[str, Any]:
        result = ctx.result or {}
        return {
            "transformed": True,
            "original_amount": result.get("amount"),
            "currency": result.get("currency", "USD"),
            "step_name": ctx.step_name,
        }

    def additional_metadata(self, ctx: StepEventContext) -> dict[str, Any]:
        return {
            "enriched_by": self.publisher_name,
            "namespace": ctx.namespace,
        }


class MockSubscriber(BaseSubscriber):
    """Mock subscriber that captures handled events."""

    def __init__(self, patterns: list[str] | None = None) -> None:
        super().__init__()
        self._patterns = patterns or ["*"]
        self.handled_events: list[dict[str, Any]] = []
        self.before_handle_called: bool = False
        self.after_handle_called: bool = False
        self.on_handle_error_called: bool = False
        self.should_skip: bool = False
        self.should_fail: bool = False

    @classmethod
    def subscribes_to(cls) -> list[str]:
        return ["*"]  # Default, overridden by instance

    def handle(self, event: dict[str, Any]) -> None:
        if self.should_fail:
            raise ValueError("Simulated handler error")
        self.handled_events.append(event)

    def before_handle(self, _event: dict[str, Any]) -> bool:
        self.before_handle_called = True
        return not self.should_skip

    def after_handle(self, _event: dict[str, Any]) -> None:
        self.after_handle_called = True

    def on_handle_error(self, _event: dict[str, Any], _error: Exception) -> None:
        self.on_handle_error_called = True

    def clear(self) -> None:
        """Reset state for next test."""
        self.handled_events.clear()
        self.before_handle_called = False
        self.after_handle_called = False
        self.on_handle_error_called = False
        self.should_skip = False
        self.should_fail = False


class PaymentSubscriber(BaseSubscriber):
    """Subscriber for payment events only."""

    @classmethod
    def subscribes_to(cls) -> list[str]:
        return ["payment.*"]

    def handle(self, event: dict[str, Any]) -> None:
        pass


class AllEventsSubscriber(BaseSubscriber):
    """Subscriber for all events."""

    @classmethod
    def subscribes_to(cls) -> list[str]:
        return ["*"]

    def handle(self, event: dict[str, Any]) -> None:
        pass


def create_step_event_context(
    task_uuid: str | None = None,
    step_uuid: str | None = None,
    step_name: str = "test_step",
    namespace: str = "test",
    result: dict[str, Any] | None = None,
    metadata: dict[str, Any] | None = None,
) -> StepEventContext:
    """Create a StepEventContext for testing."""
    return StepEventContext(
        task_uuid=task_uuid or str(uuid4()),
        step_uuid=step_uuid or str(uuid4()),
        step_name=step_name,
        namespace=namespace,
        correlation_id=str(uuid4()),
        result=result or {"amount": 100.00, "status": "success"},
        metadata=metadata or {},
    )


def create_step_result(
    success: bool = True,
    result: dict[str, Any] | None = None,
    metadata: dict[str, Any] | None = None,
) -> StepResult:
    """Create a StepResult for testing."""
    return StepResult(
        success=success,
        result=result or {"processed": True},
        metadata=metadata,
    )


def create_event_declaration(
    name: str = "test.event",
    condition: str | None = "success",
    delivery_mode: str | None = "fast",
    publisher: str | None = None,
) -> EventDeclaration:
    """Create an EventDeclaration for testing."""
    return EventDeclaration(
        name=name,
        condition=condition,
        delivery_mode=delivery_mode,
        publisher=publisher,
    )


def create_publish_context(
    event_name: str = "test.event",
    success: bool = True,
    result: dict[str, Any] | None = None,
) -> PublishContext:
    """Create a PublishContext for testing."""
    step_result = create_step_result(success=success, result=result)
    step_context = create_step_event_context(result=result)
    event_declaration = create_event_declaration(name=event_name)

    return PublishContext(
        event_name=event_name,
        step_result=step_result,
        event_declaration=event_declaration,
        step_context=step_context,
    )


# =============================================================================
# BasePublisher Tests
# =============================================================================


class TestBasePublisher:
    """Tests for BasePublisher lifecycle hooks."""

    def test_publisher_name_is_abstract(self) -> None:
        """publisher_name must be implemented by subclasses."""
        # Cannot instantiate abstract class directly
        with pytest.raises(TypeError):
            BasePublisher()  # type: ignore[abstract]

    def test_default_publisher_has_name(self) -> None:
        """DefaultPublisher provides a name."""
        publisher = DefaultPublisher()
        assert publisher.publisher_name == "default"

    def test_default_should_publish_returns_true(self) -> None:
        """Default should_publish returns True."""
        publisher = DefaultPublisher()
        ctx = create_step_event_context()
        assert publisher.should_publish(ctx) is True

    def test_default_transform_payload_returns_result(self) -> None:
        """Default transform_payload returns step result."""
        publisher = DefaultPublisher()
        ctx = create_step_event_context(result={"amount": 100})
        payload = publisher.transform_payload(ctx)
        assert payload == {"amount": 100}

    def test_default_additional_metadata_returns_empty(self) -> None:
        """Default additional_metadata returns empty dict."""
        publisher = DefaultPublisher()
        ctx = create_step_event_context()
        metadata = publisher.additional_metadata(ctx)
        assert metadata == {}


class TestPublisherLifecycleHooks:
    """Tests for publisher lifecycle hook execution."""

    def test_should_publish_called_first(self) -> None:
        """should_publish is called before other hooks."""
        publisher = MockEventPublisher()
        ctx = create_step_event_context()
        publisher.publish(ctx)

        assert publisher.should_publish_called is True

    def test_transform_payload_called_when_publishing(self) -> None:
        """transform_payload is called when should_publish returns True."""
        publisher = MockEventPublisher()
        ctx = create_step_event_context()
        publisher.publish(ctx)

        assert publisher.transform_payload_called is True

    def test_additional_metadata_called_when_publishing(self) -> None:
        """additional_metadata is called when publishing."""
        publisher = MockEventPublisher()
        ctx = create_step_event_context()
        publisher.publish(ctx)

        assert publisher.additional_metadata_called is True

    def test_before_publish_called_with_correct_args(self) -> None:
        """before_publish receives event_name, payload, and metadata."""
        publisher = MockEventPublisher()
        ctx = create_step_event_context(
            step_name="process_payment",
            result={"amount": 100},
        )
        publisher.publish(ctx, event_name="payment.processed")

        assert publisher.before_publish_called is True
        assert publisher.last_event_name == "payment.processed"
        assert publisher.last_payload == {"amount": 100}
        assert "publisher" in publisher.last_metadata
        assert publisher.last_metadata["mock_publisher"] is True

    def test_after_publish_called_on_success(self) -> None:
        """after_publish is called after successful publishing."""
        publisher = MockEventPublisher()
        ctx = create_step_event_context()
        result = publisher.publish(ctx)

        assert result is True
        assert publisher.after_publish_called is True

    def test_after_publish_not_called_on_skip(self) -> None:
        """after_publish is not called when should_publish returns False."""
        publisher = SkippingPublisher()
        ctx = create_step_event_context()
        result = publisher.publish(ctx)

        assert result is False

    def test_lifecycle_hooks_execute_in_order(self) -> None:
        """Lifecycle hooks execute in correct order."""
        call_order: list[str] = []

        class OrderTrackingPublisher(BasePublisher):
            @property
            def publisher_name(self) -> str:
                return "OrderTrackingPublisher"

            def should_publish(self, _ctx: StepEventContext) -> bool:
                call_order.append("should_publish")
                return True

            def transform_payload(self, _ctx: StepEventContext) -> dict[str, Any]:
                call_order.append("transform_payload")
                return {}

            def additional_metadata(self, _ctx: StepEventContext) -> dict[str, Any]:
                call_order.append("additional_metadata")
                return {}

            def before_publish(self, *_args: Any) -> bool:
                call_order.append("before_publish")
                return True

            def after_publish(self, *_args: Any) -> None:
                call_order.append("after_publish")

        publisher = OrderTrackingPublisher()
        ctx = create_step_event_context()
        publisher.publish(ctx)

        assert call_order == [
            "should_publish",
            "transform_payload",
            "additional_metadata",
            "before_publish",
            "after_publish",
        ]


class TestPublisherConditionalPublishing:
    """Tests for conditional publishing based on should_publish."""

    def test_skips_when_should_publish_returns_false(self) -> None:
        """Publishing is skipped when should_publish returns False."""
        publisher = SkippingPublisher()
        ctx = create_step_event_context()
        result = publisher.publish(ctx)

        assert result is False

    def test_conditional_publishing_based_on_context(self) -> None:
        """Publishers can conditionally publish based on context."""

        class NamespaceFilteringPublisher(BasePublisher):
            @property
            def publisher_name(self) -> str:
                return "NamespaceFilteringPublisher"

            def should_publish(self, ctx: StepEventContext) -> bool:
                return ctx.namespace == "production"

        publisher = NamespaceFilteringPublisher()

        # Should skip for test namespace
        test_ctx = create_step_event_context(namespace="test")
        assert publisher.publish(test_ctx) is False

        # Should publish for production namespace
        prod_ctx = create_step_event_context(namespace="production")
        assert publisher.publish(prod_ctx) is True


class TestPublisherPayloadTransformation:
    """Tests for payload transformation."""

    def test_transform_payload_modifies_result(self) -> None:
        """transform_payload can modify the event payload."""
        publisher = TransformingPublisher()
        ctx = create_step_event_context(
            step_name="process_order",
            result={"amount": 99.99, "currency": "EUR"},
        )

        # Access transform_payload directly to verify
        payload = publisher.transform_payload(ctx)

        assert payload["transformed"] is True
        assert payload["original_amount"] == 99.99
        assert payload["currency"] == "EUR"
        assert payload["step_name"] == "process_order"

    def test_additional_metadata_enriches_event(self) -> None:
        """additional_metadata adds custom fields."""
        publisher = TransformingPublisher()
        ctx = create_step_event_context(namespace="payments")

        metadata = publisher.additional_metadata(ctx)

        assert metadata["enriched_by"] == "TransformingPublisher"
        assert metadata["namespace"] == "payments"


class TestPublisherErrorHandling:
    """Tests for publisher error handling."""

    def test_on_publish_error_called_on_exception(self) -> None:
        """on_publish_error is called when before_publish raises."""
        publisher = FailingBeforePublishPublisher()
        ctx = create_step_event_context()
        result = publisher.publish(ctx)

        assert result is False
        assert publisher.error_handled is True

    def test_publish_returns_false_on_error(self) -> None:
        """publish returns False when an error occurs."""
        publisher = MockEventPublisher()
        publisher.simulate_failure(ValueError("Test error"))
        ctx = create_step_event_context()

        result = publisher.publish(ctx)

        assert result is False
        assert publisher.on_publish_error_called is True

    def test_before_publish_abort_prevents_after_publish(self) -> None:
        """When before_publish returns False, after_publish is not called."""
        publisher = AbortingPublisher()
        ctx = create_step_event_context()

        result = publisher.publish(ctx)

        assert result is False
        assert publisher.before_publish_called is True
        assert publisher.after_publish_called is False


class TestPublishContext:
    """Tests for PublishContext usage."""

    def test_publish_context_contains_all_fields(self) -> None:
        """PublishContext includes all required fields."""
        ctx = create_publish_context(
            event_name="order.created",
            success=True,
            result={"order_id": "123"},
        )

        assert ctx.event_name == "order.created"
        assert ctx.step_result.success is True
        assert ctx.step_result.result == {"order_id": "123"}
        assert ctx.event_declaration is not None
        assert ctx.event_declaration.name == "order.created"
        assert ctx.step_context is not None


# =============================================================================
# BaseSubscriber Tests
# =============================================================================


class TestBaseSubscriber:
    """Tests for BaseSubscriber base functionality."""

    def test_subscribes_to_is_abstract(self) -> None:
        """subscribes_to must be implemented by subclasses."""
        # MockSubscriber implements it
        subscriber = MockSubscriber()
        assert subscriber.subscribes_to() == ["*"]

    def test_handle_is_abstract(self) -> None:
        """handle must be implemented by subclasses."""
        subscriber = MockSubscriber()
        # Should not raise - MockSubscriber implements it
        subscriber.handle({"event_name": "test"})

    def test_subscriber_starts_inactive(self) -> None:
        """New subscribers start in inactive state."""
        subscriber = MockSubscriber()
        assert subscriber.active is False

    def test_subscriptions_starts_empty(self) -> None:
        """New subscribers have no subscriptions."""
        subscriber = MockSubscriber()
        assert subscriber.subscriptions == []


class TestSubscriberPatternMatching:
    """Tests for subscriber event pattern matching."""

    def test_matches_exact_event_name(self) -> None:
        """Exact event names match correctly."""

        class ExactSubscriber(BaseSubscriber):
            @classmethod
            def subscribes_to(cls) -> list[str]:
                return ["order.created"]

            def handle(self, event: dict[str, Any]) -> None:
                pass

        subscriber = ExactSubscriber()
        assert subscriber.matches("order.created") is True
        assert subscriber.matches("order.updated") is False

    def test_matches_wildcard_pattern(self) -> None:
        """Wildcard patterns match correctly."""

        class WildcardSubscriber(BaseSubscriber):
            @classmethod
            def subscribes_to(cls) -> list[str]:
                return ["payment.*"]

            def handle(self, event: dict[str, Any]) -> None:
                pass

        subscriber = WildcardSubscriber()
        assert subscriber.matches("payment.processed") is True
        assert subscriber.matches("payment.failed") is True
        assert subscriber.matches("order.created") is False

    def test_matches_catch_all(self) -> None:
        """Catch-all pattern matches everything."""
        subscriber = AllEventsSubscriber()
        assert subscriber.matches("anything") is True
        assert subscriber.matches("payment.processed") is True
        assert subscriber.matches("") is True

    def test_matches_multiple_patterns(self) -> None:
        """Multiple patterns are checked."""

        class MultiPatternSubscriber(BaseSubscriber):
            @classmethod
            def subscribes_to(cls) -> list[str]:
                return ["order.created", "payment.*"]

            def handle(self, event: dict[str, Any]) -> None:
                pass

        subscriber = MultiPatternSubscriber()
        assert subscriber.matches("order.created") is True
        assert subscriber.matches("payment.processed") is True
        assert subscriber.matches("inventory.updated") is False


class TestSubscriberLifecycleHooks:
    """Tests for subscriber lifecycle hook execution."""

    def _activate_subscriber(self, subscriber: MockSubscriber) -> None:
        """Activate a subscriber for testing (simulates start())."""
        subscriber._active = True

    def test_before_handle_called_first(self) -> None:
        """before_handle is called before handle."""
        subscriber = MockSubscriber()
        self._activate_subscriber(subscriber)
        subscriber.handle_event_safely({"event_name": "test"})

        assert subscriber.before_handle_called is True

    def test_after_handle_called_on_success(self) -> None:
        """after_handle is called after successful handling."""
        subscriber = MockSubscriber()
        self._activate_subscriber(subscriber)
        subscriber.handle_event_safely({"event_name": "test"})

        assert subscriber.after_handle_called is True
        assert len(subscriber.handled_events) == 1

    def test_before_handle_can_skip_handling(self) -> None:
        """Returning False from before_handle skips handling."""
        subscriber = MockSubscriber()
        self._activate_subscriber(subscriber)
        subscriber.should_skip = True
        subscriber.handle_event_safely({"event_name": "test"})

        assert subscriber.before_handle_called is True
        assert subscriber.after_handle_called is False
        assert len(subscriber.handled_events) == 0

    def test_on_handle_error_called_on_exception(self) -> None:
        """on_handle_error is called when handle raises."""
        subscriber = MockSubscriber()
        self._activate_subscriber(subscriber)
        subscriber.should_fail = True
        subscriber.handle_event_safely({"event_name": "test"})

        assert subscriber.on_handle_error_called is True
        assert subscriber.after_handle_called is False

    def test_handle_event_safely_catches_errors(self) -> None:
        """handle_event_safely doesn't propagate exceptions."""
        subscriber = MockSubscriber()
        self._activate_subscriber(subscriber)
        subscriber.should_fail = True

        # Should not raise
        subscriber.handle_event_safely({"event_name": "test"})

    def test_inactive_subscriber_skips_handling(self) -> None:
        """Inactive subscribers don't process events."""
        subscriber = MockSubscriber()
        # Don't activate - should skip
        subscriber.handle_event_safely({"event_name": "test"})

        assert subscriber.before_handle_called is False
        assert len(subscriber.handled_events) == 0


# =============================================================================
# PublisherRegistry Tests
# =============================================================================


class TestPublisherRegistry:
    """Tests for PublisherRegistry operations."""

    @pytest.fixture(autouse=True)
    def reset_registry(self) -> None:
        """Reset registry before each test."""
        PublisherRegistry.reset()
        yield
        PublisherRegistry.reset()

    def test_singleton_instance(self) -> None:
        """instance() returns singleton."""
        registry1 = PublisherRegistry.instance()
        registry2 = PublisherRegistry.instance()
        assert registry1 is registry2

    def test_register_publisher(self) -> None:
        """Publishers can be registered."""
        registry = PublisherRegistry.instance()
        publisher = MockEventPublisher()

        registry.register(publisher)

        assert registry.get("MockEventPublisher") is publisher

    def test_register_with_custom_name(self) -> None:
        """Publishers can be registered with custom name."""
        registry = PublisherRegistry.instance()
        publisher = MockEventPublisher()

        registry.register(publisher, name="custom_name")

        assert registry.get("custom_name") is publisher
        assert registry.get("MockEventPublisher") is None

    def test_get_returns_none_for_unknown(self) -> None:
        """get() returns None for unregistered publishers."""
        registry = PublisherRegistry.instance()
        assert registry.get("unknown") is None

    def test_get_or_default_returns_default(self) -> None:
        """get_or_default() returns default publisher for unknown names."""
        registry = PublisherRegistry.instance()
        publisher = registry.get_or_default("unknown")
        assert publisher.publisher_name == "default"

    def test_get_or_default_returns_registered(self) -> None:
        """get_or_default() returns registered publisher if exists."""
        registry = PublisherRegistry.instance()
        custom_publisher = MockEventPublisher()
        registry.register(custom_publisher)

        publisher = registry.get_or_default("MockEventPublisher")
        assert publisher is custom_publisher

    def test_validate_required_finds_missing(self) -> None:
        """validate_required() returns list of missing publishers."""
        registry = PublisherRegistry.instance()
        registry.register(MockEventPublisher())

        missing = registry.validate_required(["MockEventPublisher", "OtherPublisher"])

        assert missing == ["OtherPublisher"]

    def test_validate_required_empty_when_all_present(self) -> None:
        """validate_required() returns empty list when all present."""
        registry = PublisherRegistry.instance()
        registry.register(MockEventPublisher())

        missing = registry.validate_required(["MockEventPublisher"])

        assert missing == []

    def test_freeze_prevents_registration(self) -> None:
        """freeze() prevents further registration."""
        registry = PublisherRegistry.instance()
        registry.freeze()

        with pytest.raises(RuntimeError, match="frozen"):
            registry.register(MockEventPublisher())

    def test_unfreeze_allows_registration(self) -> None:
        """unfreeze() allows registration again (for testing)."""
        registry = PublisherRegistry.instance()
        registry.freeze()
        registry.unfreeze()

        # Should not raise
        registry.register(MockEventPublisher())

    def test_list_publishers(self) -> None:
        """list_publishers() returns all registered names."""
        registry = PublisherRegistry.instance()
        registry.register(MockEventPublisher())

        names = registry.list_publishers()

        assert "MockEventPublisher" in names

    def test_clear_removes_all(self) -> None:
        """clear() removes all registered publishers."""
        registry = PublisherRegistry.instance()
        registry.register(MockEventPublisher())
        registry.clear()

        assert registry.list_publishers() == []


# =============================================================================
# SubscriberRegistry Tests
# =============================================================================


class TestSubscriberRegistry:
    """Tests for SubscriberRegistry operations."""

    @pytest.fixture(autouse=True)
    def reset_registry(self) -> None:
        """Reset registry before each test."""
        SubscriberRegistry.reset()
        yield
        SubscriberRegistry.reset()

    def test_singleton_instance(self) -> None:
        """instance() returns singleton."""
        registry1 = SubscriberRegistry.instance()
        registry2 = SubscriberRegistry.instance()
        assert registry1 is registry2

    def test_register_class(self) -> None:
        """Subscriber classes can be registered."""
        registry = SubscriberRegistry.instance()

        registry.register(PaymentSubscriber)

        registered = registry.list_registered()
        assert "PaymentSubscriber" in registered["classes"]

    def test_register_instance(self) -> None:
        """Subscriber instances can be registered."""
        registry = SubscriberRegistry.instance()
        subscriber = MockSubscriber()

        registry.register_instance(subscriber)

        registered = registry.list_registered()
        assert "MockSubscriber" in registered["instances"]

    def test_register_instance_with_custom_name(self) -> None:
        """Instances can be registered with custom names."""
        registry = SubscriberRegistry.instance()
        subscriber = MockSubscriber()

        registry.register_instance(subscriber, name="custom_subscriber")

        registered = registry.list_registered()
        assert "custom_subscriber" in registered["instances"]

    def test_start_all_instantiates_classes(self) -> None:
        """start_all() instantiates registered classes."""
        registry = SubscriberRegistry.instance()
        registry.register(MockSubscriber)
        poller = MagicMock(spec=InProcessDomainEventPoller)

        count = registry.start_all(poller)

        assert count == 1
        assert registry.active_count == 1

    def test_start_all_starts_instances(self) -> None:
        """start_all() starts pre-registered instances."""
        registry = SubscriberRegistry.instance()
        subscriber = MockSubscriber()
        registry.register_instance(subscriber)
        poller = MagicMock(spec=InProcessDomainEventPoller)

        count = registry.start_all(poller)

        assert count == 1
        # Note: subscriber.active would require poller.on_event to work

    def test_stop_all_stops_subscribers(self) -> None:
        """stop_all() stops all active subscribers."""
        registry = SubscriberRegistry.instance()
        registry.register(MockSubscriber)
        poller = MagicMock(spec=InProcessDomainEventPoller)

        registry.start_all(poller)
        stopped = registry.stop_all()

        assert stopped == 1
        assert registry.active_count == 0

    def test_stats_returns_subscriber_info(self) -> None:
        """stats() returns subscriber statistics."""
        registry = SubscriberRegistry.instance()
        registry.register(PaymentSubscriber)

        stats = registry.stats()

        assert stats["registered_classes"] == 1
        assert stats["registered_instances"] == 0
        assert stats["active_subscribers"] == 0

    def test_clear_removes_all(self) -> None:
        """clear() removes all registrations."""
        registry = SubscriberRegistry.instance()
        registry.register(PaymentSubscriber)
        registry.register_instance(MockSubscriber())

        registry.clear()

        registered = registry.list_registered()
        assert registered["classes"] == []
        assert registered["instances"] == []


# =============================================================================
# Integration Tests
# =============================================================================


class TestPublisherSubscriberIntegration:
    """Integration tests for publisher-subscriber flows."""

    @pytest.fixture(autouse=True)
    def reset_registries(self) -> None:
        """Reset registries before each test."""
        PublisherRegistry.reset()
        SubscriberRegistry.reset()
        yield
        PublisherRegistry.reset()
        SubscriberRegistry.reset()

    def test_full_publisher_lifecycle_flow(self) -> None:
        """Test complete publisher lifecycle from context to event."""
        publisher = MockEventPublisher()
        ctx = create_step_event_context(
            step_name="process_payment",
            namespace="payments",
            result={"amount": 100, "currency": "USD"},
        )

        result = publisher.publish(ctx, event_name="payment.processed")

        assert result is True
        assert publisher.should_publish_called is True
        assert publisher.transform_payload_called is True
        assert publisher.additional_metadata_called is True
        assert publisher.before_publish_called is True
        assert publisher.after_publish_called is True
        assert len(publisher.published_events) == 1

        event = publisher.published_events[0]
        assert event["event_name"] == "payment.processed"
        assert event["payload"]["amount"] == 100

    def test_registry_based_publisher_lookup(self) -> None:
        """Test registry-based publisher lookup and usage."""
        registry = PublisherRegistry.instance()
        custom_publisher = TransformingPublisher()
        registry.register(custom_publisher)

        # Lookup and use
        publisher = registry.get("TransformingPublisher")
        assert publisher is not None

        ctx = create_step_event_context(result={"amount": 50})
        payload = publisher.transform_payload(ctx)

        assert payload["transformed"] is True
        assert payload["original_amount"] == 50

    def test_subscriber_handles_matching_events(self) -> None:
        """Test subscriber event handling with pattern matching."""
        subscriber = MockSubscriber()
        subscriber._active = True  # Activate for handling

        # Simulate event handling
        event = {
            "event_name": "payment.processed",
            "payload": {"amount": 100},
        }

        if subscriber.matches(event["event_name"]):
            subscriber.handle_event_safely(event)

        assert len(subscriber.handled_events) == 1
        assert subscriber.handled_events[0]["event_name"] == "payment.processed"

    def test_subscriber_skips_non_matching_events(self) -> None:
        """Test subscriber skips non-matching events."""
        subscriber = PaymentSubscriber()

        # This shouldn't match
        assert subscriber.matches("order.created") is False


class TestBaseSubscriberStartStop:
    """Tests for BaseSubscriber start() and stop() lifecycle."""

    def test_start_registers_with_poller(self):
        """start() registers event handlers with the poller."""
        subscriber = MockSubscriber()
        poller = MagicMock(spec=InProcessDomainEventPoller)

        subscriber.start(poller)

        assert subscriber.active is True
        assert len(subscriber.subscriptions) > 0
        assert poller.on_event.called

    def test_start_is_idempotent(self):
        """Calling start() twice doesn't double-register."""
        subscriber = MockSubscriber()
        poller = MagicMock(spec=InProcessDomainEventPoller)

        subscriber.start(poller)
        initial_sub_count = len(subscriber.subscriptions)
        initial_call_count = poller.on_event.call_count

        subscriber.start(poller)

        assert len(subscriber.subscriptions) == initial_sub_count
        assert poller.on_event.call_count == initial_call_count

    def test_stop_deactivates_subscriber(self):
        """stop() sets active to False and clears subscriptions."""
        subscriber = MockSubscriber()
        poller = MagicMock(spec=InProcessDomainEventPoller)

        subscriber.start(poller)
        assert subscriber.active is True

        subscriber.stop()
        assert subscriber.active is False
        assert subscriber.subscriptions == []

    def test_stop_is_idempotent(self):
        """Calling stop() on inactive subscriber is safe."""
        subscriber = MockSubscriber()
        # Don't start - already inactive
        subscriber.stop()
        assert subscriber.active is False
        assert subscriber.subscriptions == []

    def test_start_registers_multiple_patterns(self):
        """start() registers a handler for each subscribed pattern."""

        class MultiPatternSubscriber(BaseSubscriber):
            @classmethod
            def subscribes_to(cls) -> list[str]:
                return ["order.*", "payment.*", "inventory.*"]

            def handle(self, event: dict[str, Any]) -> None:
                pass

        subscriber = MultiPatternSubscriber()
        poller = MagicMock(spec=InProcessDomainEventPoller)

        subscriber.start(poller)

        assert len(subscriber.subscriptions) == 3
        assert poller.on_event.call_count == 3


class TestSubscriberHandleIfMatches:
    """Tests for BaseSubscriber._handle_if_matches()."""

    def test_handle_if_matches_matching_pattern(self):
        """_handle_if_matches calls handle for matching events."""
        subscriber = MockSubscriber()
        subscriber._active = True

        event = MagicMock()
        event.event_name = "payment.processed"
        event.model_dump.return_value = {
            "event_name": "payment.processed",
            "payload": {"amount": 100},
        }

        subscriber._handle_if_matches(event, "*")

        assert len(subscriber.handled_events) == 1
        assert subscriber.handled_events[0]["event_name"] == "payment.processed"

    def test_handle_if_matches_non_matching_pattern(self):
        """_handle_if_matches skips non-matching events."""
        subscriber = MockSubscriber()
        subscriber._active = True

        event = MagicMock()
        event.event_name = "order.created"

        subscriber._handle_if_matches(event, "payment.*")

        assert len(subscriber.handled_events) == 0


class TestPublisherRegistryEdgeCases:
    """Tests for PublisherRegistry edge cases."""

    @pytest.fixture(autouse=True)
    def reset_registry(self) -> None:
        """Reset registry before each test."""
        PublisherRegistry.reset()
        yield
        PublisherRegistry.reset()

    def test_freeze_then_unfreeze_then_register(self):
        """freeze/unfreeze cycle allows registration again."""
        registry = PublisherRegistry.instance()
        registry.freeze()
        assert registry.is_frozen is True

        registry.unfreeze()
        assert registry.is_frozen is False

        # Should work after unfreeze
        registry.register(MockEventPublisher())
        assert len(registry.list_publishers()) == 1

    def test_register_non_publisher_raises_value_error(self):
        """Registering a non-BasePublisher raises ValueError."""
        registry = PublisherRegistry.instance()

        with pytest.raises(ValueError, match="Expected BasePublisher"):
            registry.register("not_a_publisher")  # type: ignore[arg-type]

    def test_clear_when_frozen_raises_runtime_error(self):
        """clear() raises RuntimeError when registry is frozen."""
        registry = PublisherRegistry.instance()
        registry.register(MockEventPublisher())
        registry.freeze()

        with pytest.raises(RuntimeError, match="frozen"):
            registry.clear()

    def test_register_overwrites_existing_publisher(self):
        """Registering a publisher with same name overwrites."""
        registry = PublisherRegistry.instance()
        pub1 = MockEventPublisher()
        pub2 = MockEventPublisher()

        registry.register(pub1)
        registry.register(pub2)

        assert registry.get("MockEventPublisher") is pub2


class TestSubscriberRegistryEdgeCases:
    """Tests for SubscriberRegistry edge cases."""

    @pytest.fixture(autouse=True)
    def reset_registry(self) -> None:
        """Reset registry before each test."""
        SubscriberRegistry.reset()
        yield
        SubscriberRegistry.reset()

    def test_register_non_subscriber_class_raises_value_error(self):
        """Registering a non-BaseSubscriber class raises ValueError."""
        registry = SubscriberRegistry.instance()

        with pytest.raises(ValueError, match="Expected BaseSubscriber"):
            registry.register(str)  # type: ignore[arg-type]

    def test_register_instance_non_subscriber_raises_value_error(self):
        """Registering a non-BaseSubscriber instance raises ValueError."""
        registry = SubscriberRegistry.instance()

        with pytest.raises(ValueError, match="Expected BaseSubscriber"):
            registry.register_instance("not_a_subscriber")  # type: ignore[arg-type]

    def test_start_all_with_failing_class(self):
        """start_all continues when a subscriber class fails to instantiate."""

        class FailingSubscriber(BaseSubscriber):
            def __init__(self):
                raise RuntimeError("Initialization failure")

            @classmethod
            def subscribes_to(cls) -> list[str]:
                return ["*"]

            def handle(self, event: dict[str, Any]) -> None:
                pass

        registry = SubscriberRegistry.instance()
        registry.register(FailingSubscriber)
        registry.register(MockSubscriber)
        poller = MagicMock(spec=InProcessDomainEventPoller)

        started = registry.start_all(poller)

        # Only MockSubscriber should start successfully
        assert started == 1
        assert registry.active_count == 1

    def test_active_count_tracks_started_subscribers(self):
        """active_count reflects number of started subscribers."""
        registry = SubscriberRegistry.instance()
        assert registry.active_count == 0

        registry.register(MockSubscriber)
        registry.register(PaymentSubscriber)
        poller = MagicMock(spec=InProcessDomainEventPoller)

        registry.start_all(poller)
        assert registry.active_count == 2

        registry.stop_all()
        assert registry.active_count == 0

    def test_stats_reflects_active_subscribers(self):
        """stats() includes active subscriber information."""
        registry = SubscriberRegistry.instance()
        registry.register(PaymentSubscriber)
        poller = MagicMock(spec=InProcessDomainEventPoller)

        registry.start_all(poller)
        stats = registry.stats()

        assert stats["registered_classes"] == 1
        assert stats["active_subscribers"] == 1
        assert len(stats["subscriptions"]) == 1
        assert stats["subscriptions"][0]["class"] == "PaymentSubscriber"


class TestEventDeclarationStepResult:
    """Tests for EventDeclaration and StepResult types (TAS-112)."""

    def test_event_declaration_creation(self) -> None:
        """EventDeclaration can be created with all fields."""
        decl = EventDeclaration(
            name="order.completed",
            condition="success",
            delivery_mode="broadcast",
            publisher="CustomPublisher",
            schema={"type": "object"},
        )

        assert decl.name == "order.completed"
        assert decl.condition == "success"
        assert decl.delivery_mode == "broadcast"
        assert decl.publisher == "CustomPublisher"
        assert decl.schema == {"type": "object"}

    def test_event_declaration_defaults(self) -> None:
        """EventDeclaration has sensible defaults."""
        decl = EventDeclaration(name="test.event")

        assert decl.name == "test.event"
        assert decl.condition is None
        assert decl.delivery_mode is None
        assert decl.publisher is None
        assert decl.schema is None

    def test_step_result_success(self) -> None:
        """StepResult can represent success."""
        result = StepResult(
            success=True,
            result={"processed": 100},
            metadata={"duration_ms": 150},
        )

        assert result.success is True
        assert result.result == {"processed": 100}
        assert result.metadata == {"duration_ms": 150}

    def test_step_result_failure(self) -> None:
        """StepResult can represent failure."""
        result = StepResult(
            success=False,
            result=None,
            metadata={"error": "Timeout"},
        )

        assert result.success is False
        assert result.result is None

    def test_publish_context_with_all_components(self) -> None:
        """PublishContext combines all components."""
        step_result = StepResult(success=True, result={"id": 123})
        event_decl = EventDeclaration(name="item.created", condition="success")
        step_ctx = create_step_event_context()

        ctx = PublishContext(
            event_name="item.created",
            step_result=step_result,
            event_declaration=event_decl,
            step_context=step_ctx,
        )

        assert ctx.event_name == "item.created"
        assert ctx.step_result.success is True
        assert ctx.event_declaration.name == "item.created"
        assert ctx.step_context is not None
