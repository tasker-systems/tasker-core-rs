"""Step handlers for diamond workflow.

These handlers demonstrate a diamond/parallel workflow pattern where:
1. Start step squares the input
2. Two branches run in parallel (add and multiply)
3. End step averages the results from both branches

    DiamondStart (square)
         /          \\
   BranchB (+25)  BranchC (*2)
         \\          /
     DiamondEnd (average)

For input even_number=4:
    Start: 4^2 = 16
    BranchB: 16 + 25 = 41
    BranchC: 16 * 2 = 32
    End: (41 + 32) / 2 = 36.5
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from tasker_core import StepHandler, StepHandlerResult

if TYPE_CHECKING:
    from tasker_core import StepContext


class DiamondStartHandler(StepHandler):
    """Starting step: square the initial even number.

    Input:
        even_number: int - The even number from input_data

    Output:
        result: int - The squared value (n**2)
        operation: str - "square"
    """

    handler_name = "diamond_workflow.step_handlers.DiamondStartHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Square the even number."""
        even_number = context.input_data.get("even_number")

        if even_number is None:
            return StepHandlerResult.failure_handler_result(
                message="Task context must contain an even_number",
                error_type="validation_error",
                retryable=False,
            )

        if not isinstance(even_number, int) or even_number % 2 != 0:
            return StepHandlerResult.failure_handler_result(
                message=f"even_number must be an even integer, got: {even_number}",
                error_type="validation_error",
                retryable=False,
            )

        # Square the even number (initial step operation)
        result = even_number * even_number

        return StepHandlerResult.success_handler_result(
            {
                "result": result,
                "operation": "square",
                "step_type": "initial",
            }
        )


class DiamondBranchBHandler(StepHandler):
    """Branch B: add constant (left parallel branch).

    Input (from dependency):
        diamond_start_py.result: int - The squared value

    Output:
        result: int - The value after addition (n**2 + 25)
        operation: str - "add"
    """

    handler_name = "diamond_workflow.step_handlers.DiamondBranchBHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Add 25 to the squared result."""
        # Get result from start step using convenience method
        start_output = context.get_dependency_result("diamond_start_py")

        if start_output is None:
            return StepHandlerResult.failure_handler_result(
                message="Missing result from diamond_start_py",
                error_type="dependency_error",
                retryable=True,
            )

        # Extract the actual computed value from handler output
        squared_value = (
            start_output.get("result") if isinstance(start_output, dict) else start_output
        )

        if squared_value is None:
            return StepHandlerResult.failure_handler_result(
                message="Missing 'result' field in diamond_start_py output",
                error_type="dependency_error",
                retryable=True,
            )

        # Add constant (branch B operation)
        constant = 25
        result = squared_value + constant

        return StepHandlerResult.success_handler_result(
            {
                "result": result,
                "operation": "add",
                "constant": constant,
                "input_value": squared_value,
                "branch": "B",
            }
        )


class DiamondBranchCHandler(StepHandler):
    """Branch C: multiply by factor (right parallel branch).

    Input (from dependency):
        diamond_start_py.result: int - The squared value

    Output:
        result: int - The value after multiplication (n**2 * 2)
        operation: str - "multiply"
    """

    handler_name = "diamond_workflow.step_handlers.DiamondBranchCHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Multiply the squared result by 2."""
        # Get result from start step using convenience method
        start_output = context.get_dependency_result("diamond_start_py")

        if start_output is None:
            return StepHandlerResult.failure_handler_result(
                message="Missing result from diamond_start_py",
                error_type="dependency_error",
                retryable=True,
            )

        # Extract the actual computed value from handler output
        squared_value = (
            start_output.get("result") if isinstance(start_output, dict) else start_output
        )

        if squared_value is None:
            return StepHandlerResult.failure_handler_result(
                message="Missing 'result' field in diamond_start_py output",
                error_type="dependency_error",
                retryable=True,
            )

        # Multiply by factor (branch C operation)
        factor = 2
        result = squared_value * factor

        return StepHandlerResult.success_handler_result(
            {
                "result": result,
                "operation": "multiply",
                "factor": factor,
                "input_value": squared_value,
                "branch": "C",
            }
        )


class DiamondEndHandler(StepHandler):
    """End step: average results from both branches (convergence).

    Input (from dependencies):
        diamond_branch_b_py.result: int - Result from branch B
        diamond_branch_c_py.result: int - Result from branch C

    Output:
        result: float - The average of both branch results
        operation: str - "average"
    """

    handler_name = "diamond_workflow.step_handlers.DiamondEndHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Calculate average of both branch results."""
        # Get results from both branches using convenience method
        branch_b_output = context.get_dependency_result("diamond_branch_b_py")
        branch_c_output = context.get_dependency_result("diamond_branch_c_py")

        # Extract the actual computed values from handler outputs
        b_value = None
        c_value = None

        if branch_b_output is not None:
            b_value = (
                branch_b_output.get("result")
                if isinstance(branch_b_output, dict)
                else branch_b_output
            )

        if branch_c_output is not None:
            c_value = (
                branch_c_output.get("result")
                if isinstance(branch_c_output, dict)
                else branch_c_output
            )

        if b_value is None or c_value is None:
            missing = []
            if b_value is None:
                missing.append("diamond_branch_b_py")
            if c_value is None:
                missing.append("diamond_branch_c_py")
            return StepHandlerResult.failure_handler_result(
                message=f"Missing results from: {', '.join(missing)}",
                error_type="dependency_error",
                retryable=True,
            )

        # Average both results (convergence operation)
        result = (b_value + c_value) / 2

        return StepHandlerResult.success_handler_result(
            {
                "result": result,
                "operation": "average",
                "branch_b_value": b_value,
                "branch_c_value": c_value,
                "step_type": "convergence",
            }
        )


__all__ = [
    "DiamondStartHandler",
    "DiamondBranchBHandler",
    "DiamondBranchCHandler",
    "DiamondEndHandler",
]
