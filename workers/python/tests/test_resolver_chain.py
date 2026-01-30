"""TAS-93: Tests for resolver chain infrastructure.

Tests the resolver chain, built-in resolvers, method dispatch wrapper,
and handler definition types.
"""

from __future__ import annotations

from typing import TYPE_CHECKING

import pytest

from tasker_core.registry import (
    BaseResolver,
    ClassLookupResolver,
    ExplicitMappingResolver,
    HandlerDefinition,
    MethodDispatchWrapper,
    ResolverChain,
    ResolverNotFoundError,
)
from tasker_core.step_handler.base import StepHandler
from tasker_core.types import StepHandlerResult

if TYPE_CHECKING:
    from tasker_core.types import StepContext


# =============================================================================
# Test Handlers and Resolvers
# =============================================================================


class TestHandler(StepHandler):
    """Simple test handler."""

    handler_name = "test_handler"

    def call(self, _context: StepContext) -> StepHandlerResult:
        return StepHandlerResult.success({"from": "call"})


class MultiMethodHandler(StepHandler):
    """Handler with multiple callable methods."""

    handler_name = "multi_method_handler"

    def call(self, _context: StepContext) -> StepHandlerResult:
        return StepHandlerResult.success({"method": "call"})

    def process(self, _context: StepContext) -> StepHandlerResult:
        return StepHandlerResult.success({"method": "process"})

    def refund(self, _context: StepContext) -> StepHandlerResult:
        return StepHandlerResult.success({"method": "refund"})


class CustomResolver(BaseResolver):
    """Custom resolver for testing priority ordering."""

    def __init__(self, name: str = "custom", priority: int = 50) -> None:
        self._name = name
        self._priority = priority

    @property
    def name(self) -> str:
        return self._name

    @property
    def priority(self) -> int:
        return self._priority

    def can_resolve(self, definition: HandlerDefinition, _config=None) -> bool:
        return definition.callable.startswith("custom:")

    def resolve(self, definition: HandlerDefinition, _config=None):
        if not self.can_resolve(definition):
            return None
        return TestHandler()


# =============================================================================
# HandlerDefinition Tests
# =============================================================================


class TestHandlerDefinition:
    """Tests for HandlerDefinition dataclass."""

    def test_basic_creation(self):
        """Test basic HandlerDefinition creation."""
        definition = HandlerDefinition(callable="my_handler")

        assert definition.callable == "my_handler"
        assert definition.handler_method is None
        assert definition.resolver is None
        assert definition.initialization == {}

    def test_with_all_fields(self):
        """Test HandlerDefinition with all fields populated."""
        definition = HandlerDefinition(
            callable="payment.PaymentHandler",
            handler_method="refund",
            resolver="explicit_mapping",
            initialization={"api_key": "secret"},
        )

        assert definition.callable == "payment.PaymentHandler"
        assert definition.handler_method == "refund"
        assert definition.resolver == "explicit_mapping"
        assert definition.initialization == {"api_key": "secret"}

    def test_effective_method_default(self):
        """Test effective_method returns 'call' by default."""
        definition = HandlerDefinition(callable="handler")
        assert definition.effective_method() == "call"

    def test_effective_method_custom(self):
        """Test effective_method returns custom method."""
        definition = HandlerDefinition(callable="handler", handler_method="process")
        assert definition.effective_method() == "process"

    def test_uses_method_dispatch_false(self):
        """Test uses_method_dispatch is False without handler_method."""
        definition = HandlerDefinition(callable="handler")
        assert definition.uses_method_dispatch() is False

        # Also False if handler_method is 'call'
        definition2 = HandlerDefinition(callable="handler", handler_method="call")
        assert definition2.uses_method_dispatch() is False

    def test_uses_method_dispatch_true(self):
        """Test uses_method_dispatch is True with custom method."""
        definition = HandlerDefinition(callable="handler", handler_method="process")
        assert definition.uses_method_dispatch() is True

    def test_has_resolver_hint_false(self):
        """Test has_resolver_hint is False without resolver."""
        definition = HandlerDefinition(callable="handler")
        assert definition.has_resolver_hint() is False

        # Also False if resolver is empty string
        definition2 = HandlerDefinition(callable="handler", resolver="")
        assert definition2.has_resolver_hint() is False

    def test_has_resolver_hint_true(self):
        """Test has_resolver_hint is True with resolver."""
        definition = HandlerDefinition(callable="handler", resolver="explicit_mapping")
        assert definition.has_resolver_hint() is True

    def test_from_dict_with_method_field(self):
        """Test from_dict reads 'method' field from Rust FFI."""
        data = {
            "callable": "payment.PaymentHandler",
            "method": "refund",  # Rust uses 'method' not 'handler_method'
            "resolver": "explicit_mapping",
            "initialization": {"api_key": "secret"},
        }

        definition = HandlerDefinition.from_dict(data)

        assert definition.callable == "payment.PaymentHandler"
        assert definition.handler_method == "refund"  # Mapped correctly
        assert definition.resolver == "explicit_mapping"
        assert definition.initialization == {"api_key": "secret"}

    def test_from_dict_empty(self):
        """Test from_dict handles empty/None input."""
        definition = HandlerDefinition.from_dict(None)

        assert definition.callable == ""
        assert definition.handler_method is None
        assert definition.resolver is None
        assert definition.initialization == {}


# =============================================================================
# MethodDispatchWrapper Tests
# =============================================================================


class TestMethodDispatchWrapper:
    """Tests for MethodDispatchWrapper."""

    def test_wraps_method(self, sample_step_context: StepContext):
        """Test wrapper redirects call to target method."""
        handler = MultiMethodHandler()
        wrapper = MethodDispatchWrapper(handler, "process")

        handler_result = wrapper.call(sample_step_context)

        assert handler_result.is_success is True
        assert handler_result.result == {"method": "process"}

    def test_unwrap_returns_original(self):
        """Test unwrap returns original handler."""
        handler = MultiMethodHandler()
        wrapper = MethodDispatchWrapper(handler, "process")

        unwrapped = wrapper.unwrap()
        assert unwrapped is handler

    def test_raises_on_missing_method(self):
        """Test raises AttributeError for missing method."""
        handler = MultiMethodHandler()

        with pytest.raises(AttributeError, match="does not have method 'nonexistent'"):
            MethodDispatchWrapper(handler, "nonexistent")

    def test_handler_property(self):
        """Test handler property access."""
        handler = MultiMethodHandler()
        wrapper = MethodDispatchWrapper(handler, "process")

        assert wrapper.handler is handler

    def test_target_method_property(self):
        """Test target_method property."""
        handler = MultiMethodHandler()
        wrapper = MethodDispatchWrapper(handler, "process")

        assert wrapper.target_method == "process"


# =============================================================================
# ExplicitMappingResolver Tests
# =============================================================================


class TestExplicitMappingResolver:
    """Tests for ExplicitMappingResolver."""

    def test_priority_and_name(self):
        """Test resolver priority and name."""
        resolver = ExplicitMappingResolver()

        assert resolver.name == "explicit_mapping"
        assert resolver.priority == 10

    def test_register_and_resolve_class(self):
        """Test registering and resolving a handler class."""
        resolver = ExplicitMappingResolver()
        resolver.register("test", TestHandler)

        definition = HandlerDefinition(callable="test")
        handler = resolver.resolve(definition)

        assert handler is not None
        assert isinstance(handler, TestHandler)

    def test_register_and_resolve_instance(self):
        """Test registering and resolving a handler instance."""
        resolver = ExplicitMappingResolver()
        instance = TestHandler()
        resolver.register("test_instance", instance)

        definition = HandlerDefinition(callable="test_instance")
        resolved = resolver.resolve(definition)

        assert resolved is instance

    def test_can_resolve_registered(self):
        """Test can_resolve returns True for registered handlers."""
        resolver = ExplicitMappingResolver()
        resolver.register("test", TestHandler)

        definition = HandlerDefinition(callable="test")
        assert resolver.can_resolve(definition) is True

    def test_can_resolve_unregistered(self):
        """Test can_resolve returns False for unregistered handlers."""
        resolver = ExplicitMappingResolver()

        definition = HandlerDefinition(callable="unknown")
        assert resolver.can_resolve(definition) is False

    def test_unregister(self):
        """Test unregistering a handler."""
        resolver = ExplicitMappingResolver()
        resolver.register("test", TestHandler)

        assert resolver.unregister("test") is True
        assert resolver.unregister("test") is False  # Already unregistered

        definition = HandlerDefinition(callable="test")
        assert resolver.can_resolve(definition) is False

    def test_registered_callables(self):
        """Test listing registered callables."""
        resolver = ExplicitMappingResolver()
        resolver.register("handler_a", TestHandler)
        resolver.register("handler_b", MultiMethodHandler)

        callables = resolver.registered_callables()
        assert "handler_a" in callables
        assert "handler_b" in callables


# =============================================================================
# ClassLookupResolver Tests
# =============================================================================


class TestClassLookupResolver:
    """Tests for ClassLookupResolver."""

    def test_priority_and_name(self):
        """Test resolver priority and name."""
        resolver = ClassLookupResolver()

        assert resolver.name == "class_lookup"
        assert resolver.priority == 100

    def test_can_resolve_class_path(self):
        """Test can_resolve identifies valid class paths."""
        resolver = ClassLookupResolver()

        # Valid class paths
        valid_definition = HandlerDefinition(callable="module.ClassName")
        assert resolver.can_resolve(valid_definition) is True

        valid_definition2 = HandlerDefinition(callable="package.submodule.MyHandler")
        assert resolver.can_resolve(valid_definition2) is True

    def test_cannot_resolve_simple_name(self):
        """Test can_resolve rejects simple handler names."""
        resolver = ClassLookupResolver()

        # Simple names (no dot, or doesn't end with capitalized identifier)
        simple_definition = HandlerDefinition(callable="my_handler")
        assert resolver.can_resolve(simple_definition) is False

        no_class = HandlerDefinition(callable="module.something")
        assert resolver.can_resolve(no_class) is False

    def test_resolve_actual_class(self):
        """Test resolving an actual importable class."""
        resolver = ClassLookupResolver()

        # Use a class from this test module
        definition = HandlerDefinition(callable="tests.test_resolver_chain.TestHandler")
        handler = resolver.resolve(definition)

        assert handler is not None
        assert isinstance(handler, TestHandler)

    def test_resolve_nonexistent_class(self):
        """Test resolving a non-existent class returns None."""
        resolver = ClassLookupResolver()

        definition = HandlerDefinition(callable="nonexistent.module.Handler")
        handler = resolver.resolve(definition)

        assert handler is None


# =============================================================================
# ResolverChain Tests
# =============================================================================


class TestResolverChain:
    """Tests for ResolverChain."""

    def test_default_chain(self):
        """Test default chain has expected resolvers."""
        chain = ResolverChain.default()
        resolvers = chain.list_resolvers()

        resolver_names = [name for name, _ in resolvers]
        assert "explicit_mapping" in resolver_names
        assert "class_lookup" in resolver_names

    def test_priority_ordering(self):
        """Test resolvers are ordered by priority."""
        chain = ResolverChain()
        chain.add_resolver(ClassLookupResolver())  # priority 100
        chain.add_resolver(ExplicitMappingResolver())  # priority 10
        chain.add_resolver(CustomResolver(name="mid", priority=50))

        resolvers = chain.list_resolvers()

        # Should be sorted by priority (lowest first)
        assert resolvers[0][0] == "explicit_mapping"  # priority 10
        assert resolvers[1][0] == "mid"  # priority 50
        assert resolvers[2][0] == "class_lookup"  # priority 100

    def test_resolve_with_chain(self):
        """Test resolution through the chain."""
        chain = ResolverChain.default()
        explicit = chain.get_resolver("explicit_mapping")
        explicit.register("test_handler", TestHandler)

        definition = HandlerDefinition(callable="test_handler")
        handler = chain.resolve(definition)

        assert handler is not None
        assert isinstance(handler, TestHandler)

    def test_resolve_with_resolver_hint(self):
        """Test resolution with resolver hint bypasses chain."""
        chain = ResolverChain()
        chain.add_resolver(ExplicitMappingResolver())
        chain.add_resolver(CustomResolver(name="custom"))

        explicit = chain.get_resolver("explicit_mapping")
        explicit.register("test_handler", TestHandler)

        # With resolver hint, should go directly to specified resolver
        definition = HandlerDefinition(
            callable="test_handler",
            resolver="explicit_mapping",
        )
        handler = chain.resolve(definition)

        assert handler is not None
        assert isinstance(handler, TestHandler)

    def test_resolver_hint_not_found(self):
        """Test resolver hint with non-existent resolver raises error."""
        chain = ResolverChain.default()

        definition = HandlerDefinition(
            callable="test_handler",
            resolver="nonexistent_resolver",
        )

        with pytest.raises(ResolverNotFoundError):
            chain.resolve(definition)

    def test_wrap_for_method_dispatch(self):
        """Test method dispatch wrapping."""
        chain = ResolverChain.default()

        handler = MultiMethodHandler()
        definition = HandlerDefinition(callable="test", handler_method="process")

        wrapped = chain.wrap_for_method_dispatch(handler, definition)

        assert isinstance(wrapped, MethodDispatchWrapper)
        assert wrapped.target_method == "process"

    def test_no_wrap_for_default_call(self):
        """Test no wrapping when using default call method."""
        chain = ResolverChain.default()

        handler = MultiMethodHandler()
        definition = HandlerDefinition(callable="test")

        result = chain.wrap_for_method_dispatch(handler, definition)

        # Should return unwrapped handler
        assert result is handler

    def test_get_resolver(self):
        """Test getting a resolver by name."""
        chain = ResolverChain.default()

        explicit = chain.get_resolver("explicit_mapping")
        assert explicit is not None
        assert explicit.name == "explicit_mapping"

        class_lookup = chain.get_resolver("class_lookup")
        assert class_lookup is not None
        assert class_lookup.name == "class_lookup"

        nonexistent = chain.get_resolver("nonexistent")
        assert nonexistent is None

    def test_add_resolver_sorted(self):
        """Test that adding resolvers maintains sort order."""
        chain = ResolverChain()

        # Add in random order
        chain.add_resolver(CustomResolver("high", 90))
        chain.add_resolver(CustomResolver("low", 5))
        chain.add_resolver(CustomResolver("mid", 50))

        resolvers = chain.list_resolvers()
        priorities = [p for _, p in resolvers]

        assert priorities == sorted(priorities)


# =============================================================================
# Integration Tests
# =============================================================================


class TestResolverChainIntegration:
    """Integration tests for resolver chain with HandlerRegistry."""

    def test_full_resolution_flow(self, sample_step_context: StepContext):
        """Test complete resolution flow with method dispatch."""
        chain = ResolverChain.default()

        # Register handler
        explicit = chain.get_resolver("explicit_mapping")
        explicit.register("payment_handler", MultiMethodHandler)

        # Create definition with method dispatch
        definition = HandlerDefinition(
            callable="payment_handler",
            handler_method="refund",
        )

        # Resolve through chain
        handler = chain.resolve(definition)
        assert handler is not None

        # Apply method dispatch wrapper
        wrapped = chain.wrap_for_method_dispatch(handler, definition)
        assert isinstance(wrapped, MethodDispatchWrapper)

        # Execute and verify correct method was called
        handler_result = wrapped.call(sample_step_context)

        assert handler_result.is_success is True
        assert handler_result.result == {"method": "refund"}

    def test_fallback_through_chain(self):
        """Test that resolution falls through to next resolver."""
        chain = ResolverChain.default()

        # Don't register in explicit mapping - should fall through to class_lookup
        # Class lookup will find TestHandler if path is correct
        definition = HandlerDefinition(callable="tests.test_resolver_chain.TestHandler")

        handler = chain.resolve(definition)
        assert handler is not None
        assert isinstance(handler, TestHandler)


# =============================================================================
# Extended ResolverChain Tests (Phase 1.3)
# =============================================================================


class TestResolverChainExtended:
    """Extended tests for ResolverChain methods."""

    def test_resolve_with_hint_success(self):
        """Test _resolve_with_hint returns handler from named resolver."""
        chain = ResolverChain.default()
        explicit = chain.get_resolver("explicit_mapping")
        explicit.register("test_handler", TestHandler)

        definition = HandlerDefinition(callable="test_handler", resolver="explicit_mapping")
        handler = chain.resolve(definition)
        assert handler is not None
        assert isinstance(handler, TestHandler)

    def test_resolve_with_hint_resolver_returns_none(self):
        """Test _resolve_with_hint returns None when resolver can't resolve."""
        chain = ResolverChain.default()

        definition = HandlerDefinition(callable="nonexistent", resolver="explicit_mapping")
        handler = chain.resolve(definition)
        assert handler is None

    def test_can_resolve_chain_wide(self):
        """Test can_resolve checks all resolvers in chain."""
        chain = ResolverChain.default()
        explicit = chain.get_resolver("explicit_mapping")
        explicit.register("test_handler", TestHandler)

        assert chain.can_resolve(HandlerDefinition(callable="test_handler")) is True
        assert chain.can_resolve(HandlerDefinition(callable="nonexistent_handler")) is False

    def test_can_resolve_with_hint(self):
        """Test can_resolve with resolver hint delegates to named resolver."""
        chain = ResolverChain.default()
        explicit = chain.get_resolver("explicit_mapping")
        explicit.register("test_handler", TestHandler)

        # With hint pointing to explicit_mapping
        assert (
            chain.can_resolve(
                HandlerDefinition(callable="test_handler", resolver="explicit_mapping")
            )
            is True
        )
        assert (
            chain.can_resolve(HandlerDefinition(callable="missing", resolver="explicit_mapping"))
            is False
        )

    def test_can_resolve_with_unknown_hint(self):
        """Test can_resolve returns False for unknown resolver hint."""
        chain = ResolverChain.default()
        assert (
            chain.can_resolve(HandlerDefinition(callable="x", resolver="nonexistent_resolver"))
            is False
        )

    def test_register_convenience_method(self):
        """Test register() adds to explicit_mapping resolver."""
        chain = ResolverChain.default()
        chain.register("direct_handler", TestHandler)

        handler = chain.resolve(HandlerDefinition(callable="direct_handler"))
        assert handler is not None
        assert isinstance(handler, TestHandler)

    def test_register_raises_without_explicit_resolver(self):
        """Test register() raises RuntimeError without ExplicitMappingResolver."""
        chain = ResolverChain()
        chain.add_resolver(ClassLookupResolver())

        with pytest.raises(RuntimeError, match="No ExplicitMappingResolver"):
            chain.register("key", TestHandler)

    def test_wrap_for_method_dispatch_missing_method(self):
        """Test wrap_for_method_dispatch returns None for missing method."""
        chain = ResolverChain.default()
        handler = TestHandler()
        definition = HandlerDefinition(callable="test", handler_method="nonexistent_method")

        result = chain.wrap_for_method_dispatch(handler, definition)
        assert result is None

    def test_chain_info(self):
        """Test chain_info returns resolver metadata."""
        chain = ResolverChain.default()
        info = chain.chain_info()

        assert len(info) == 2
        names = [entry["name"] for entry in info]
        assert "explicit_mapping" in names
        assert "class_lookup" in names
        for entry in info:
            assert "priority" in entry
            assert "callables" in entry

    def test_remove_resolver(self):
        """Test remove_resolver removes by name."""
        chain = ResolverChain.default()
        assert len(chain) == 2

        removed = chain.remove_resolver("class_lookup")
        assert removed is not None
        assert removed.name == "class_lookup"
        assert len(chain) == 1

    def test_remove_resolver_nonexistent(self):
        """Test remove_resolver returns None for unknown name."""
        chain = ResolverChain.default()
        removed = chain.remove_resolver("nonexistent")
        assert removed is None

    def test_resolver_names_property(self):
        """Test resolver_names returns names in priority order."""
        chain = ResolverChain.default()
        names = chain.resolver_names
        assert names == ["explicit_mapping", "class_lookup"]

    def test_registered_callables(self):
        """Test registered_callables aggregates across resolvers."""
        chain = ResolverChain.default()
        explicit = chain.get_resolver("explicit_mapping")
        explicit.register("handler_a", TestHandler)
        explicit.register("handler_b", MultiMethodHandler)

        callables = chain.registered_callables()
        assert "handler_a" in callables
        assert "handler_b" in callables


# =============================================================================
# ClassLookupResolver Extended Tests (Phase 1.5)
# =============================================================================


class TestClassLookupResolverExtended:
    """Extended tests for ClassLookupResolver error paths."""

    def test_import_class_import_error(self):
        """Test _import_class returns None on ImportError."""
        resolver = ClassLookupResolver()
        result = resolver._import_class("nonexistent_module_xyz.Handler")
        assert result is None

    def test_import_class_non_type_attribute(self):
        """Test _import_class returns None when attribute is not a type."""
        resolver = ClassLookupResolver()
        # os.path is a module, not a type
        result = resolver._import_class("os.path")
        assert result is None

    def test_import_class_no_dot(self):
        """Test _import_class returns None for no-dot strings."""
        resolver = ClassLookupResolver()
        result = resolver._import_class("NoDotHere")
        assert result is None

    def test_instantiate_handler_config_fallback(self):
        """Test _instantiate_handler tries config kwarg on TypeError."""
        resolver = ClassLookupResolver()

        class NeedsConfig:
            def __init__(self, config):
                self.config = config

        definition = HandlerDefinition(
            callable="test",
            initialization={"key": "value"},
        )
        result = resolver._instantiate_handler(NeedsConfig, definition, {})
        assert result is not None
        assert result.config == {"key": "value"}

    def test_instantiate_handler_failure(self):
        """Test _instantiate_handler returns None when both attempts fail."""
        resolver = ClassLookupResolver()

        class BadInit:
            def __init__(self, required_arg, another_arg):
                pass

        definition = HandlerDefinition(callable="test")
        result = resolver._instantiate_handler(BadInit, definition, {})
        assert result is None


# =============================================================================
# BaseResolver Extended Tests (Phase 1.6)
# =============================================================================


class TestBaseResolverExtended:
    """Extended tests for BaseResolver default implementations."""

    def test_registered_callables_returns_empty_list(self):
        """Test registered_callables() default returns empty list."""
        resolver = CustomResolver(name="test", priority=50)
        callables = resolver.registered_callables()
        assert callables == []
