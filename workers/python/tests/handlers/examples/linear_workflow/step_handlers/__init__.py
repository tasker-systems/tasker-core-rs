"""Step handlers for linear workflow."""

from __future__ import annotations

from typing import TYPE_CHECKING

from tasker_core import StepHandler, StepHandlerResult

if TYPE_CHECKING:
    from tasker_core import StepContext


class LinearStep1Handler(StepHandler):
    """First step: square the initial even number.

    Input:
        even_number: int - The even number from input_data

    Output:
        result: int - The squared value (n**2)
        operation: str - "square"
    """

    handler_name = "linear_workflow.step_handlers.LinearStep1Handler"
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

        # Square the even number (first step operation)
        result = even_number * even_number

        return StepHandlerResult.success_handler_result({
            "result": result,
            "operation": "square",
            "step_type": "initial",
            "input_refs": {"even_number": "task.context.even_number"},
        })


class LinearStep2Handler(StepHandler):
    """Second step: add constant to squared result.

    Input (from dependency):
        linear_step_1_py.result: int - The squared value

    Output:
        result: int - The value after addition (n**2 + 10)
        operation: str - "add"
    """

    handler_name = "linear_workflow.step_handlers.LinearStep2Handler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Add 10 to the squared result."""
        # Get result from previous step using convenience method
        # This unwraps the nested {"result": value} structure
        step1_output = context.get_dependency_result("linear_step_1_py")

        if step1_output is None:
            return StepHandlerResult.failure_handler_result(
                message="Missing result from linear_step_1_py",
                error_type="dependency_error",
                retryable=True,
            )

        # Extract the actual computed value from handler output
        squared_value = step1_output.get("result") if isinstance(step1_output, dict) else step1_output

        if squared_value is None:
            return StepHandlerResult.failure_handler_result(
                message="Missing 'result' field in linear_step_1_py output",
                error_type="dependency_error",
                retryable=True,
            )

        # Add constant (second step operation)
        constant = 10
        result = squared_value + constant

        return StepHandlerResult.success_handler_result({
            "result": result,
            "operation": "add",
            "constant": constant,
            "input_value": squared_value,
        })


class LinearStep3Handler(StepHandler):
    """Third step: multiply by factor.

    Input (from dependency):
        linear_step_2_py.result: int - The value after addition

    Output:
        result: int - The value after multiplication ((n**2 + 10) * 3)
        operation: str - "multiply"
    """

    handler_name = "linear_workflow.step_handlers.LinearStep3Handler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Multiply by 3."""
        # Get result from previous step using convenience method
        step2_output = context.get_dependency_result("linear_step_2_py")

        if step2_output is None:
            return StepHandlerResult.failure_handler_result(
                message="Missing result from linear_step_2_py",
                error_type="dependency_error",
                retryable=True,
            )

        # Extract the actual computed value from handler output
        added_value = step2_output.get("result") if isinstance(step2_output, dict) else step2_output

        if added_value is None:
            return StepHandlerResult.failure_handler_result(
                message="Missing 'result' field in linear_step_2_py output",
                error_type="dependency_error",
                retryable=True,
            )

        # Multiply by factor (third step operation)
        factor = 3
        result = added_value * factor

        return StepHandlerResult.success_handler_result({
            "result": result,
            "operation": "multiply",
            "factor": factor,
            "input_value": added_value,
        })


class LinearStep4Handler(StepHandler):
    """Fourth step: divide for final result.

    Input (from dependency):
        linear_step_3_py.result: int - The value after multiplication

    Output:
        result: float - The final value (((n**2 + 10) * 3) / 2)
        operation: str - "divide"
    """

    handler_name = "linear_workflow.step_handlers.LinearStep4Handler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Divide by 2 for final result."""
        # Get result from previous step using convenience method
        step3_output = context.get_dependency_result("linear_step_3_py")

        if step3_output is None:
            return StepHandlerResult.failure_handler_result(
                message="Missing result from linear_step_3_py",
                error_type="dependency_error",
                retryable=True,
            )

        # Extract the actual computed value from handler output
        multiplied_value = step3_output.get("result") if isinstance(step3_output, dict) else step3_output

        if multiplied_value is None:
            return StepHandlerResult.failure_handler_result(
                message="Missing 'result' field in linear_step_3_py output",
                error_type="dependency_error",
                retryable=True,
            )

        # Divide by divisor (fourth step operation)
        divisor = 2
        result = multiplied_value / divisor

        return StepHandlerResult.success_handler_result({
            "result": result,
            "operation": "divide",
            "divisor": divisor,
            "input_value": multiplied_value,
        })


__all__ = [
    "LinearStep1Handler",
    "LinearStep2Handler",
    "LinearStep3Handler",
    "LinearStep4Handler",
]
