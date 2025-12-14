"""Diamond workflow example handlers.

These handlers demonstrate a diamond dependency pattern where execution
branches from a single node and merges back together:

           DiamondInit
              /    \\
         PathA      PathB
              \\    /
           DiamondMerge

This pattern is useful for parallel processing with a final aggregation step.

Example usage:
    from tests.handlers.examples.diamond_handlers import (
        DiamondInitHandler,
        DiamondPathAHandler,
        DiamondPathBHandler,
        DiamondMergeHandler,
    )
    from tasker_core import HandlerRegistry

    registry = HandlerRegistry.instance()
    registry.register("diamond_init", DiamondInitHandler)
    registry.register("diamond_path_a", DiamondPathAHandler)
    registry.register("diamond_path_b", DiamondPathBHandler)
    registry.register("diamond_merge", DiamondMergeHandler)
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from tasker_core import StepHandler, StepHandlerResult

if TYPE_CHECKING:
    from tasker_core import StepContext


class DiamondInitHandler(StepHandler):
    """Initial handler at the top of the diamond.

    This handler initializes the workflow and provides data that will
    be processed by both parallel branches.

    Input:
        initial_value: int - Starting value (default: 100)

    Output:
        initialized: bool - Always True
        value: int - The initial value
        metadata: dict - Additional metadata
    """

    handler_name = "diamond_init"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Initialize the diamond workflow."""
        initial_value = context.input_data.get("initial_value", 100)

        return StepHandlerResult.success_handler_result({
            "initialized": True,
            "value": initial_value,
            "metadata": {
                "workflow": "diamond",
                "init_timestamp": "2025-01-01T00:00:00Z",
            },
        })


class DiamondPathAHandler(StepHandler):
    """Path A handler (left branch of diamond).

    This handler processes the initial value using multiplication.
    It runs in parallel with PathB.

    Input (from dependency):
        diamond_init.value: int - Value to process

    Output:
        path_a_result: int - The processed value (value * 2)
        operation: str - Description of the operation
    """

    handler_name = "diamond_path_a"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Process data via path A (multiplication)."""
        init_result = context.dependency_results.get("diamond_init", {})
        value = init_result.get("value", 0)

        # Path A: multiply by 2
        result = value * 2

        return StepHandlerResult.success_handler_result({
            "path_a_result": result,
            "operation": "multiply_by_2",
            "input_value": value,
        })


class DiamondPathBHandler(StepHandler):
    """Path B handler (right branch of diamond).

    This handler processes the initial value using addition.
    It runs in parallel with PathA.

    Input (from dependency):
        diamond_init.value: int - Value to process

    Output:
        path_b_result: int - The processed value (value + 50)
        operation: str - Description of the operation
    """

    handler_name = "diamond_path_b"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Process data via path B (addition)."""
        init_result = context.dependency_results.get("diamond_init", {})
        value = init_result.get("value", 0)

        # Path B: add 50
        result = value + 50

        return StepHandlerResult.success_handler_result({
            "path_b_result": result,
            "operation": "add_50",
            "input_value": value,
        })


class DiamondMergeHandler(StepHandler):
    """Merge handler at the bottom of the diamond.

    This handler waits for both PathA and PathB to complete,
    then combines their results.

    Input (from dependencies):
        diamond_path_a.path_a_result: int - Result from path A
        diamond_path_b.path_b_result: int - Result from path B

    Output:
        merged_result: int - Sum of both path results
        path_a_value: int - The path A result
        path_b_value: int - The path B result
        merge_summary: dict - Summary of the merge operation
    """

    handler_name = "diamond_merge"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Merge results from both paths."""
        path_a = context.dependency_results.get("diamond_path_a", {})
        path_b = context.dependency_results.get("diamond_path_b", {})

        a_result = path_a.get("path_a_result", 0)
        b_result = path_b.get("path_b_result", 0)

        # Validate we have results from both paths
        if a_result == 0 and b_result == 0:
            return StepHandlerResult.failure_handler_result(
                message="Missing results from both paths",
                error_type="dependency_error",
                retryable=True,
            )

        merged = a_result + b_result

        return StepHandlerResult.success_handler_result({
            "merged_result": merged,
            "path_a_value": a_result,
            "path_b_value": b_result,
            "merge_summary": {
                "total": merged,
                "path_a_operation": path_a.get("operation", "unknown"),
                "path_b_operation": path_b.get("operation", "unknown"),
            },
        })
