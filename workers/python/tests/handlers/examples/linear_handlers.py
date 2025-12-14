"""Linear workflow example handlers.

These handlers demonstrate a sequential workflow pattern where each step
depends on the previous one in a chain:

    FetchData -> TransformData -> StoreData

Example usage:
    from tests.handlers.examples.linear_handlers import (
        FetchDataHandler,
        TransformDataHandler,
        StoreDataHandler,
    )
    from tasker_core import HandlerRegistry

    registry = HandlerRegistry.instance()
    registry.register("fetch_data", FetchDataHandler)
    registry.register("transform_data", TransformDataHandler)
    registry.register("store_data", StoreDataHandler)
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from tasker_core import StepHandler, StepHandlerResult

if TYPE_CHECKING:
    from tasker_core import StepContext


class FetchDataHandler(StepHandler):
    """Handler that fetches data (step 1 of linear workflow).

    This handler simulates fetching data from an external source.
    It uses input_data to determine the source and returns a list of items.

    Input:
        source: str - The data source to fetch from (default: "default")

    Output:
        source: str - The source that was used
        items: list - List of fetched items
    """

    handler_name = "fetch_data"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Fetch data from the configured source."""
        source = context.input_data.get("source", "default")

        # Simulate fetching data
        items = [
            {"id": 1, "value": "item_1"},
            {"id": 2, "value": "item_2"},
            {"id": 3, "value": "item_3"},
        ]

        return StepHandlerResult.success_handler_result({
            "source": source,
            "items": items,
            "fetch_timestamp": "2025-01-01T00:00:00Z",
        })


class TransformDataHandler(StepHandler):
    """Handler that transforms data (step 2 of linear workflow).

    This handler takes the output from FetchDataHandler and transforms
    each item by adding a 'processed' flag and uppercase value.

    Input (from dependency):
        fetch_data.items: list - List of items to transform

    Output:
        transformed_items: list - List of transformed items
        transform_count: int - Number of items transformed
    """

    handler_name = "transform_data"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Transform the fetched data."""
        # Get result from previous step
        fetch_result = context.dependency_results.get("fetch_data", {})
        items = fetch_result.get("items", [])

        if not items:
            return StepHandlerResult.failure_handler_result(
                message="No items to transform",
                error_type="validation_error",
                retryable=False,
            )

        # Transform each item
        transformed = []
        for item in items:
            transformed.append({
                "id": item.get("id"),
                "value": item.get("value", "").upper(),
                "processed": True,
            })

        return StepHandlerResult.success_handler_result({
            "transformed_items": transformed,
            "transform_count": len(transformed),
        })


class StoreDataHandler(StepHandler):
    """Handler that stores data (step 3 of linear workflow).

    This handler takes the transformed data and simulates storing it.
    It returns information about what was stored.

    Input (from dependency):
        transform_data.transformed_items: list - Items to store

    Output:
        stored_count: int - Number of items stored
        stored_ids: list - IDs of stored items
        storage_location: str - Where data was stored
    """

    handler_name = "store_data"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Store the transformed data."""
        # Get transformed data from previous step
        transform_result = context.dependency_results.get("transform_data", {})
        items = transform_result.get("transformed_items", [])

        if not items:
            return StepHandlerResult.failure_handler_result(
                message="No items to store",
                error_type="validation_error",
                retryable=False,
            )

        # Simulate storing data
        stored_ids = [item.get("id") for item in items if item.get("id")]

        return StepHandlerResult.success_handler_result({
            "stored_count": len(stored_ids),
            "stored_ids": stored_ids,
            "storage_location": "database",
            "store_timestamp": "2025-01-01T00:00:01Z",
        })
