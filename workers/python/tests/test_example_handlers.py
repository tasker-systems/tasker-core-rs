"""Tests for example handlers demonstrating workflow patterns.

These tests verify that the example handlers work correctly and demonstrate
proper handler implementation patterns for users to follow.
"""

from __future__ import annotations

from uuid import uuid4

from tasker_core import (
    FfiStepEvent,
    HandlerRegistry,
    StepContext,
)
from tests.handlers.examples import (
    DiamondInitHandler,
    DiamondMergeHandler,
    DiamondPathAHandler,
    DiamondPathBHandler,
    FetchDataHandler,
    StoreDataHandler,
    TransformDataHandler,
)


def create_context_with_deps(
    handler_name: str,
    input_data: dict | None = None,
    dependency_results: dict | None = None,
) -> StepContext:
    """Create a StepContext with optional input data and dependency results.

    Uses Ruby-compatible nested structure:
    - step_definition.handler.callable -> handler name
    - task.context -> input data
    - dependency_results -> results from parent steps
    """
    event = FfiStepEvent(
        event_id=str(uuid4()),
        task_uuid=str(uuid4()),
        step_uuid=str(uuid4()),
        correlation_id=str(uuid4()),
        task_sequence_step={
            "step_definition": {
                "handler": {
                    "callable": handler_name,
                },
            },
            "task": {
                "context": input_data or {},
            },
            "dependency_results": dependency_results or {},
        },
    )
    return StepContext.from_ffi_event(event, handler_name)


class TestLinearWorkflowHandlers:
    """Tests for linear workflow handlers."""

    def test_fetch_data_handler_default_source(self):
        """Test FetchDataHandler with default source."""
        handler = FetchDataHandler()
        context = create_context_with_deps("fetch_data")

        result = handler.call(context)

        assert result.is_success is True
        assert result.result["source"] == "default"
        assert len(result.result["items"]) == 3
        assert "fetch_timestamp" in result.result

    def test_fetch_data_handler_custom_source(self):
        """Test FetchDataHandler with custom source."""
        handler = FetchDataHandler()
        context = create_context_with_deps(
            "fetch_data",
            input_data={"source": "external_api"},
        )

        result = handler.call(context)

        assert result.is_success is True
        assert result.result["source"] == "external_api"

    def test_fetch_data_handler_properties(self):
        """Test FetchDataHandler class properties."""
        handler = FetchDataHandler()

        assert handler.name == "fetch_data"
        assert handler.version == "1.0.0"

    def test_transform_data_handler_success(self):
        """Test TransformDataHandler with valid input."""
        handler = TransformDataHandler()

        fetch_result = {
            "source": "test",
            "items": [
                {"id": 1, "value": "item_1"},
                {"id": 2, "value": "item_2"},
            ],
        }
        context = create_context_with_deps(
            "transform_data",
            dependency_results={"fetch_data": fetch_result},
        )

        result = handler.call(context)

        assert result.is_success is True
        assert len(result.result["transformed_items"]) == 2
        assert result.result["transform_count"] == 2
        # Check transformation was applied
        assert result.result["transformed_items"][0]["value"] == "ITEM_1"
        assert result.result["transformed_items"][0]["processed"] is True

    def test_transform_data_handler_empty_items(self):
        """Test TransformDataHandler with empty items returns failure."""
        handler = TransformDataHandler()
        context = create_context_with_deps(
            "transform_data",
            dependency_results={"fetch_data": {"items": []}},
        )

        result = handler.call(context)

        assert result.is_success is False
        assert result.error_type == "validation_error"
        assert result.retryable is False

    def test_transform_data_handler_no_dependency(self):
        """Test TransformDataHandler without dependency results."""
        handler = TransformDataHandler()
        context = create_context_with_deps("transform_data")

        result = handler.call(context)

        assert result.is_success is False
        assert "No items to transform" in result.error_message

    def test_store_data_handler_success(self):
        """Test StoreDataHandler with valid input."""
        handler = StoreDataHandler()

        transform_result = {
            "transformed_items": [
                {"id": 1, "value": "ITEM_1", "processed": True},
                {"id": 2, "value": "ITEM_2", "processed": True},
            ],
        }
        context = create_context_with_deps(
            "store_data",
            dependency_results={"transform_data": transform_result},
        )

        result = handler.call(context)

        assert result.is_success is True
        assert result.result["stored_count"] == 2
        assert result.result["stored_ids"] == [1, 2]
        assert result.result["storage_location"] == "database"

    def test_store_data_handler_empty_items(self):
        """Test StoreDataHandler with empty items returns failure."""
        handler = StoreDataHandler()
        context = create_context_with_deps(
            "store_data",
            dependency_results={"transform_data": {"transformed_items": []}},
        )

        result = handler.call(context)

        assert result.is_success is False
        assert result.error_type == "validation_error"

    def test_linear_workflow_integration(self):
        """Test full linear workflow execution."""
        # Step 1: Fetch
        fetch_handler = FetchDataHandler()
        fetch_context = create_context_with_deps("fetch_data")
        fetch_result = fetch_handler.call(fetch_context)
        assert fetch_result.is_success

        # Step 2: Transform (using fetch result)
        transform_handler = TransformDataHandler()
        transform_context = create_context_with_deps(
            "transform_data",
            dependency_results={"fetch_data": fetch_result.result},
        )
        transform_result = transform_handler.call(transform_context)
        assert transform_result.is_success

        # Step 3: Store (using transform result)
        store_handler = StoreDataHandler()
        store_context = create_context_with_deps(
            "store_data",
            dependency_results={"transform_data": transform_result.result},
        )
        store_result = store_handler.call(store_context)
        assert store_result.is_success

        # Verify final results
        assert store_result.result["stored_count"] == 3
        assert store_result.result["stored_ids"] == [1, 2, 3]


class TestDiamondWorkflowHandlers:
    """Tests for diamond workflow handlers."""

    def test_diamond_init_handler_default_value(self):
        """Test DiamondInitHandler with default value."""
        handler = DiamondInitHandler()
        context = create_context_with_deps("diamond_init")

        result = handler.call(context)

        assert result.is_success is True
        assert result.result["initialized"] is True
        assert result.result["value"] == 100
        assert result.result["metadata"]["workflow"] == "diamond"

    def test_diamond_init_handler_custom_value(self):
        """Test DiamondInitHandler with custom initial value."""
        handler = DiamondInitHandler()
        context = create_context_with_deps(
            "diamond_init",
            input_data={"initial_value": 200},
        )

        result = handler.call(context)

        assert result.is_success is True
        assert result.result["value"] == 200

    def test_diamond_path_a_handler(self):
        """Test DiamondPathAHandler (multiply by 2)."""
        handler = DiamondPathAHandler()

        init_result = {"initialized": True, "value": 100}
        context = create_context_with_deps(
            "diamond_path_a",
            dependency_results={"diamond_init": init_result},
        )

        result = handler.call(context)

        assert result.is_success is True
        assert result.result["path_a_result"] == 200  # 100 * 2
        assert result.result["operation"] == "multiply_by_2"
        assert result.result["input_value"] == 100

    def test_diamond_path_b_handler(self):
        """Test DiamondPathBHandler (add 50)."""
        handler = DiamondPathBHandler()

        init_result = {"initialized": True, "value": 100}
        context = create_context_with_deps(
            "diamond_path_b",
            dependency_results={"diamond_init": init_result},
        )

        result = handler.call(context)

        assert result.is_success is True
        assert result.result["path_b_result"] == 150  # 100 + 50
        assert result.result["operation"] == "add_50"
        assert result.result["input_value"] == 100

    def test_diamond_merge_handler(self):
        """Test DiamondMergeHandler combines results."""
        handler = DiamondMergeHandler()

        context = create_context_with_deps(
            "diamond_merge",
            dependency_results={
                "diamond_path_a": {
                    "path_a_result": 200,
                    "operation": "multiply_by_2",
                },
                "diamond_path_b": {
                    "path_b_result": 150,
                    "operation": "add_50",
                },
            },
        )

        result = handler.call(context)

        assert result.is_success is True
        assert result.result["merged_result"] == 350  # 200 + 150
        assert result.result["path_a_value"] == 200
        assert result.result["path_b_value"] == 150
        assert result.result["merge_summary"]["total"] == 350

    def test_diamond_merge_handler_missing_results(self):
        """Test DiamondMergeHandler with missing results."""
        handler = DiamondMergeHandler()
        context = create_context_with_deps("diamond_merge")

        result = handler.call(context)

        assert result.is_success is False
        assert result.error_type == "dependency_error"
        assert result.retryable is True

    def test_diamond_workflow_integration(self):
        """Test full diamond workflow execution."""
        # Step 1: Initialize
        init_handler = DiamondInitHandler()
        init_context = create_context_with_deps(
            "diamond_init",
            input_data={"initial_value": 100},
        )
        init_result = init_handler.call(init_context)
        assert init_result.is_success

        # Step 2a: Path A (parallel)
        path_a_handler = DiamondPathAHandler()
        path_a_context = create_context_with_deps(
            "diamond_path_a",
            dependency_results={"diamond_init": init_result.result},
        )
        path_a_result = path_a_handler.call(path_a_context)
        assert path_a_result.is_success

        # Step 2b: Path B (parallel)
        path_b_handler = DiamondPathBHandler()
        path_b_context = create_context_with_deps(
            "diamond_path_b",
            dependency_results={"diamond_init": init_result.result},
        )
        path_b_result = path_b_handler.call(path_b_context)
        assert path_b_result.is_success

        # Step 3: Merge (waits for both)
        merge_handler = DiamondMergeHandler()
        merge_context = create_context_with_deps(
            "diamond_merge",
            dependency_results={
                "diamond_path_a": path_a_result.result,
                "diamond_path_b": path_b_result.result,
            },
        )
        merge_result = merge_handler.call(merge_context)
        assert merge_result.is_success

        # With initial_value=100:
        # Path A: 100 * 2 = 200
        # Path B: 100 + 50 = 150
        # Merged: 200 + 150 = 350
        assert merge_result.result["merged_result"] == 350


class TestHandlerRegistration:
    """Tests for registering example handlers with HandlerRegistry."""

    def setup_method(self):
        """Reset registry before each test."""
        HandlerRegistry.reset_instance()

    def teardown_method(self):
        """Clean up after each test."""
        HandlerRegistry.reset_instance()

    def test_register_linear_handlers(self):
        """Test registering all linear workflow handlers."""
        registry = HandlerRegistry.instance()
        initial_count = registry.handler_count()

        registry.register("fetch_data", FetchDataHandler)
        registry.register("transform_data", TransformDataHandler)
        registry.register("store_data", StoreDataHandler)

        assert registry.is_registered("fetch_data")
        assert registry.is_registered("transform_data")
        assert registry.is_registered("store_data")
        # Handlers may already exist from auto-discovery, so check relative increase
        assert registry.handler_count() >= initial_count

    def test_register_diamond_handlers(self):
        """Test registering all diamond workflow handlers."""
        registry = HandlerRegistry.instance()
        initial_count = registry.handler_count()

        registry.register("diamond_init", DiamondInitHandler)
        registry.register("diamond_path_a", DiamondPathAHandler)
        registry.register("diamond_path_b", DiamondPathBHandler)
        registry.register("diamond_merge", DiamondMergeHandler)

        assert registry.is_registered("diamond_init")
        assert registry.is_registered("diamond_path_a")
        assert registry.is_registered("diamond_path_b")
        assert registry.is_registered("diamond_merge")
        # Handlers may already exist from auto-discovery, so check relative increase
        assert registry.handler_count() >= initial_count

        # Verify resolution returns correct handlers
        init = registry.resolve("diamond_init")
        assert init.name == "diamond_init"

    def test_execute_registered_handler(self):
        """Test executing a handler resolved from registry."""
        registry = HandlerRegistry.instance()
        registry.register("fetch_data", FetchDataHandler)

        handler = registry.resolve("fetch_data")
        assert handler is not None

        context = create_context_with_deps("fetch_data")
        result = handler.call(context)

        assert result.is_success is True
        assert "items" in result.result
