"""Tests for RegistryResolver base class.

These tests verify:
- Pattern matching with regex
- Prefix matching
- can_resolve() with matching/non-matching inputs
- resolve() delegates to resolve_handler()
- resolve_handler() raises NotImplementedError when not overridden
- _match_pattern() with and without compiled pattern
- name and priority properties from class variables
"""

from __future__ import annotations

import pytest

from tasker_core.registry.handler_definition import HandlerDefinition
from tasker_core.registry.registry_resolver import RegistryResolver


class PatternResolver(RegistryResolver):
    """Test resolver using regex pattern matching."""

    _name = "pattern_resolver"
    _priority = 20
    pattern = r"^payments:(?P<provider>\w+):(?P<action>\w+)$"

    def resolve_handler(self, definition, config):  # noqa: ARG002
        match = self._match_pattern(definition.callable)
        if not match:
            return None
        return {"provider": match.group("provider"), "action": match.group("action")}


class PrefixResolver(RegistryResolver):
    """Test resolver using prefix matching."""

    _name = "prefix_resolver"
    _priority = 30
    prefix = "myapp."


class BareResolver(RegistryResolver):
    """Test resolver with no pattern or prefix (returns False for can_resolve)."""

    _name = "bare_resolver"
    _priority = 50


class TestRegistryResolverProperties:
    """Test name and priority properties."""

    def test_name_from_class_variable(self):
        """name property returns _name class variable."""
        resolver = PatternResolver()
        assert resolver.name == "pattern_resolver"

    def test_priority_from_class_variable(self):
        """priority property returns _priority class variable."""
        resolver = PatternResolver()
        assert resolver.priority == 20

    def test_prefix_resolver_name_and_priority(self):
        """PrefixResolver has correct name and priority."""
        resolver = PrefixResolver()
        assert resolver.name == "prefix_resolver"
        assert resolver.priority == 30


class TestRegistryResolverCanResolve:
    """Test can_resolve() with different matching strategies."""

    def test_pattern_matching_success(self):
        """can_resolve returns True for matching regex pattern."""
        resolver = PatternResolver()
        definition = HandlerDefinition(callable="payments:stripe:charge")
        assert resolver.can_resolve(definition) is True

    def test_pattern_matching_failure(self):
        """can_resolve returns False for non-matching regex pattern."""
        resolver = PatternResolver()
        definition = HandlerDefinition(callable="orders:create")
        assert resolver.can_resolve(definition) is False

    def test_prefix_matching_success(self):
        """can_resolve returns True for matching prefix."""
        resolver = PrefixResolver()
        definition = HandlerDefinition(callable="myapp.handlers.PaymentHandler")
        assert resolver.can_resolve(definition) is True

    def test_prefix_matching_failure(self):
        """can_resolve returns False for non-matching prefix."""
        resolver = PrefixResolver()
        definition = HandlerDefinition(callable="other.handlers.PaymentHandler")
        assert resolver.can_resolve(definition) is False

    def test_no_pattern_or_prefix_returns_false(self):
        """can_resolve returns False when neither pattern nor prefix is set."""
        resolver = BareResolver()
        definition = HandlerDefinition(callable="anything")
        assert resolver.can_resolve(definition) is False


class TestRegistryResolverResolve:
    """Test resolve() and resolve_handler() delegation."""

    def test_resolve_delegates_to_resolve_handler(self):
        """resolve() delegates to resolve_handler()."""
        resolver = PatternResolver()
        definition = HandlerDefinition(callable="payments:stripe:charge")
        result = resolver.resolve(definition)
        assert result == {"provider": "stripe", "action": "charge"}

    def test_resolve_passes_empty_config_when_none(self):
        """resolve() passes empty dict when config is None."""
        resolver = PatternResolver()
        definition = HandlerDefinition(callable="payments:paypal:refund")
        result = resolver.resolve(definition, config=None)
        assert result == {"provider": "paypal", "action": "refund"}

    def test_resolve_handler_raises_not_implemented(self):
        """resolve_handler() raises NotImplementedError when not overridden."""
        resolver = PrefixResolver()
        definition = HandlerDefinition(callable="myapp.handler")
        with pytest.raises(NotImplementedError, match="resolve_handler.*must be implemented"):
            resolver.resolve(definition)


class TestRegistryResolverMatchPattern:
    """Test _match_pattern() helper."""

    def test_match_pattern_with_compiled_pattern(self):
        """_match_pattern returns match object for matching callable."""
        resolver = PatternResolver()
        match = resolver._match_pattern("payments:stripe:charge")
        assert match is not None
        assert match.group("provider") == "stripe"
        assert match.group("action") == "charge"

    def test_match_pattern_no_match(self):
        """_match_pattern returns None for non-matching callable."""
        resolver = PatternResolver()
        match = resolver._match_pattern("orders:create")
        assert match is None

    def test_match_pattern_without_compiled_pattern(self):
        """_match_pattern returns None when no pattern is compiled."""
        resolver = BareResolver()
        match = resolver._match_pattern("anything")
        assert match is None
