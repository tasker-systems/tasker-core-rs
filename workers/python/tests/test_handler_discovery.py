"""Tests for handler discovery and advanced registry functionality.

These tests cover:
- Handler instantiation failure handling
- Package-based handler discovery
- Module scanning for StepHandler subclasses
"""

from __future__ import annotations

from tasker_core import HandlerRegistry, StepHandler, StepHandlerResult


class TestHandlerInstantiationErrors:
    """Tests for handler instantiation error handling."""

    def setup_method(self):
        """Reset registry before each test."""
        HandlerRegistry.reset_instance()

    def teardown_method(self):
        """Clean up after each test."""
        HandlerRegistry.reset_instance()

    def test_resolve_handler_that_fails_init(self):
        """Test resolving a handler that raises in __init__."""

        class FailingHandler(StepHandler):
            handler_name = "failing_handler"

            def __init__(self):
                raise RuntimeError("Handler initialization failed")

            def call(self, _context):
                return StepHandlerResult.success_handler_result({})

        registry = HandlerRegistry.instance()
        registry.register("failing_handler", FailingHandler)

        # resolve() should return None when instantiation fails
        handler = registry.resolve("failing_handler")
        assert handler is None


class TestHandlerDiscovery:
    """Tests for handler discovery from packages."""

    def setup_method(self):
        """Reset registry before each test."""
        HandlerRegistry.reset_instance()

    def teardown_method(self):
        """Clean up after each test."""
        HandlerRegistry.reset_instance()

    def test_discover_handlers_from_example_package(self):
        """Test discovering handlers from the examples package."""
        registry = HandlerRegistry.instance()

        # Discover handlers from our test handlers package
        count = registry.discover_handlers("tests.handlers.examples")

        # Should find all the example handlers
        assert count >= 6  # At minimum: 3 linear + 4 diamond handlers

        # Verify specific handlers were discovered
        assert registry.is_registered("fetch_data")
        assert registry.is_registered("transform_data")
        assert registry.is_registered("store_data")
        assert registry.is_registered("diamond_init")
        assert registry.is_registered("diamond_path_a")
        assert registry.is_registered("diamond_path_b")
        assert registry.is_registered("diamond_merge")

    def test_discover_handlers_from_nonexistent_package(self):
        """Test discovering from a nonexistent package returns 0."""
        registry = HandlerRegistry.instance()

        count = registry.discover_handlers("nonexistent.package.that.does.not.exist")
        assert count == 0

    def test_discover_handlers_from_module_without_path(self):
        """Test discovering from a module without __path__ attribute."""
        registry = HandlerRegistry.instance()

        # tasker_core.types is a module, not a package, so no __path__
        count = registry.discover_handlers("tasker_core.types")
        # Should return 0 since there are no handlers in types.py
        assert count == 0

    def test_discover_handlers_with_custom_base_class(self):
        """Test discovering handlers with a custom base class filter."""
        registry = HandlerRegistry.instance()

        # Create a custom base class
        class CustomBaseHandler(StepHandler):
            handler_name = "custom_base"

            def call(self, _context):
                return StepHandlerResult.success_handler_result({})

        # Discover with custom base - should find nothing
        # since our example handlers don't extend CustomBaseHandler
        count = registry.discover_handlers(
            "tests.handlers.examples",
            base_class=CustomBaseHandler,
        )
        assert count == 0


class TestModuleScanning:
    """Tests for module scanning functionality."""

    def setup_method(self):
        """Reset registry before each test."""
        HandlerRegistry.reset_instance()

    def teardown_method(self):
        """Clean up after each test."""
        HandlerRegistry.reset_instance()

    def test_scan_module_finds_handlers(self):
        """Test scanning a module finds StepHandler subclasses."""
        from tests.handlers.examples import linear_handlers

        registry = HandlerRegistry.instance()
        count = registry._scan_module_for_handlers(linear_handlers, StepHandler)

        assert count == 3
        assert registry.is_registered("fetch_data")
        assert registry.is_registered("transform_data")
        assert registry.is_registered("store_data")

    def test_scan_module_ignores_handler_without_name(self):
        """Test that handlers without handler_name are ignored."""

        # Create a module-like object with handlers
        class FakeModule:
            pass

        class HandlerWithName(StepHandler):
            handler_name = "named_handler"

            def call(self, _context):
                return StepHandlerResult.success_handler_result({})

        class HandlerWithoutName(StepHandler):
            # No handler_name set!

            def call(self, _context):
                return StepHandlerResult.success_handler_result({})

        FakeModule.HandlerWithName = HandlerWithName
        FakeModule.HandlerWithoutName = HandlerWithoutName

        registry = HandlerRegistry.instance()
        count = registry._scan_module_for_handlers(FakeModule, StepHandler)

        # Only HandlerWithName should be registered
        assert count == 1
        assert registry.is_registered("named_handler")

    def test_scan_module_ignores_base_class_itself(self):
        """Test that the base StepHandler class itself is not registered."""

        class FakeModule:
            pass

        FakeModule.StepHandler = StepHandler

        registry = HandlerRegistry.instance()
        count = registry._scan_module_for_handlers(FakeModule, StepHandler)

        assert count == 0

    def test_scan_module_ignores_non_classes(self):
        """Test that non-class objects are ignored during scanning."""

        class FakeModule:
            some_string = "not a class"
            some_number = 42
            some_list = [1, 2, 3]

            def some_function():
                pass

        registry = HandlerRegistry.instance()
        count = registry._scan_module_for_handlers(FakeModule, StepHandler)

        assert count == 0


class TestHandlerRegistryEdgeCases:
    """Tests for edge cases in HandlerRegistry."""

    def setup_method(self):
        """Reset registry before each test."""
        HandlerRegistry.reset_instance()

    def teardown_method(self):
        """Clean up after each test."""
        HandlerRegistry.reset_instance()

    def test_resolve_and_execute_discovered_handler(self):
        """Test that discovered handlers can be resolved and executed."""
        registry = HandlerRegistry.instance()
        registry.discover_handlers("tests.handlers.examples")

        handler = registry.resolve("fetch_data")
        assert handler is not None
        assert handler.name == "fetch_data"

        # Create a minimal context
        from uuid import uuid4

        from tasker_core import FfiStepEvent, StepContext

        event = FfiStepEvent(
            event_id=str(uuid4()),
            task_uuid=str(uuid4()),
            step_uuid=str(uuid4()),
            correlation_id=str(uuid4()),
            task_sequence_step={},
        )
        context = StepContext.from_ffi_event(event, "fetch_data")

        result = handler.call(context)
        assert result.success is True

    def test_get_handler_class_not_found(self):
        """Test get_handler_class returns None for unknown handler."""
        registry = HandlerRegistry.instance()
        handler_class = registry.get_handler_class("nonexistent_handler")
        assert handler_class is None

    def test_multiple_discovery_calls_accumulate(self):
        """Test that multiple discovery calls accumulate handlers."""
        registry = HandlerRegistry.instance()

        # First discovery
        count1 = registry.discover_handlers("tests.handlers.examples.linear_handlers")
        assert count1 > 0

        # Second discovery - should add more handlers
        count2 = registry.discover_handlers("tests.handlers.examples.diamond_handlers")
        assert count2 > 0

        # Total should be sum of both
        assert registry.handler_count() >= count1 + count2
