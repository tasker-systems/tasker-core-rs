"""Handler registry tests.

These tests verify:
- HandlerRegistry singleton pattern
- Handler registration and resolution
- Handler listing and management
- Handler class retrieval
"""

from __future__ import annotations

import pytest

from tasker_core import HandlerRegistry, StepHandler, StepHandlerResult


class TestHandlerRegistry:
    """Tests for HandlerRegistry."""

    def setup_method(self):
        """Reset singleton before each test."""
        HandlerRegistry.reset_instance()

    def teardown_method(self):
        """Clean up after each test."""
        HandlerRegistry.reset_instance()

    def test_singleton_instance(self):
        """Test HandlerRegistry singleton pattern."""
        reg1 = HandlerRegistry.instance()
        reg2 = HandlerRegistry.instance()
        assert reg1 is reg2

    def test_reset_instance(self):
        """Test singleton reset creates new instance."""
        reg1 = HandlerRegistry.instance()
        HandlerRegistry.reset_instance()
        reg2 = HandlerRegistry.instance()
        assert reg1 is not reg2

    def test_register_and_resolve(self):
        """Test handler registration and resolution."""
        registry = HandlerRegistry()

        class TestHandler(StepHandler):
            handler_name = "test_handler"

            def call(self, _context):
                return StepHandlerResult.success({})

        registry.register("test_handler", TestHandler)
        handler = registry.resolve("test_handler")

        assert handler is not None
        assert handler.name == "test_handler"

    def test_resolve_not_found(self):
        """Test resolving non-existent handler returns None."""
        registry = HandlerRegistry()
        handler = registry.resolve("non_existent")
        assert handler is None

    def test_is_registered(self):
        """Test is_registered method."""
        registry = HandlerRegistry()

        class TestHandler(StepHandler):
            handler_name = "test_handler"

            def call(self, _context):
                return StepHandlerResult.success({})

        assert not registry.is_registered("test_handler")
        registry.register("test_handler", TestHandler)
        assert registry.is_registered("test_handler")

    def test_list_handlers(self):
        """Test list_handlers method."""
        registry = HandlerRegistry()

        class Handler1(StepHandler):
            handler_name = "handler1"

            def call(self, _context):
                return StepHandlerResult.success({})

        class Handler2(StepHandler):
            handler_name = "handler2"

            def call(self, _context):
                return StepHandlerResult.success({})

        registry.register("handler1", Handler1)
        registry.register("handler2", Handler2)

        handlers = registry.list_handlers()
        assert set(handlers) == {"handler1", "handler2"}

    def test_unregister(self):
        """Test unregister method."""
        registry = HandlerRegistry()

        class TestHandler(StepHandler):
            handler_name = "test_handler"

            def call(self, _context):
                return StepHandlerResult.success({})

        registry.register("test_handler", TestHandler)
        assert registry.is_registered("test_handler")

        result = registry.unregister("test_handler")
        assert result is True
        assert not registry.is_registered("test_handler")

    def test_unregister_not_found(self):
        """Test unregister returns False for non-existent handler."""
        registry = HandlerRegistry()
        result = registry.unregister("non_existent")
        assert result is False

    def test_handler_count(self):
        """Test handler_count method."""
        registry = HandlerRegistry()
        assert registry.handler_count() == 0

        class TestHandler(StepHandler):
            handler_name = "test"

            def call(self, _context):
                return StepHandlerResult.success({})

        registry.register("test", TestHandler)
        assert registry.handler_count() == 1

    def test_clear(self):
        """Test clear method."""
        registry = HandlerRegistry()

        class TestHandler(StepHandler):
            handler_name = "test"

            def call(self, _context):
                return StepHandlerResult.success({})

        registry.register("test", TestHandler)
        assert registry.handler_count() == 1

        registry.clear()
        assert registry.handler_count() == 0

    def test_get_handler_class(self):
        """Test get_handler_class without instantiation."""
        registry = HandlerRegistry()

        class TestHandler(StepHandler):
            handler_name = "test"
            handler_version = "2.0.0"

            def call(self, _context):
                return StepHandlerResult.success({})

        registry.register("test", TestHandler)

        handler_class = registry.get_handler_class("test")
        assert handler_class is TestHandler
        assert handler_class.handler_version == "2.0.0"

    def test_register_overwrites_existing(self):
        """Test registering same name overwrites previous handler."""
        registry = HandlerRegistry()

        class Handler1(StepHandler):
            handler_name = "test"
            handler_version = "1.0.0"

            def call(self, _context):
                return StepHandlerResult.success({})

        class Handler2(StepHandler):
            handler_name = "test"
            handler_version = "2.0.0"

            def call(self, _context):
                return StepHandlerResult.success({})

        registry.register("test", Handler1)
        registry.register("test", Handler2)

        handler = registry.resolve("test")
        assert handler.version == "2.0.0"

    def test_register_invalid_class_raises(self):
        """Test registering non-StepHandler raises ValueError."""
        registry = HandlerRegistry()

        class NotAHandler:
            pass

        with pytest.raises(ValueError):
            registry.register("test", NotAHandler)
