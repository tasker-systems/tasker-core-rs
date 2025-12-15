"""Step handlers for test scenarios."""

from __future__ import annotations

from datetime import datetime, timezone
from typing import TYPE_CHECKING

from tasker_core import StepHandler, StepHandlerResult

if TYPE_CHECKING:
    from tasker_core import StepContext


class SuccessStepHandler(StepHandler):
    """Handler that always succeeds.

    Useful for testing successful workflow execution.

    Input (optional):
        message: str - Optional message to include in result

    Output:
        status: str - "success"
        message: str - Success message
        timestamp: str - ISO 8601 timestamp
    """

    handler_name = "test_scenarios.step_handlers.SuccessStepHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Execute successfully."""
        message = context.input_data.get("message", "Step completed successfully")

        return StepHandlerResult.success_handler_result({
            "status": "success",
            "message": message,
            "timestamp": datetime.now(timezone.utc).isoformat(),
            "handler": "SuccessStepHandler",
        })


class RetryableErrorStepHandler(StepHandler):
    """Handler that returns retryable errors.

    Useful for testing retry behavior in workflows.

    Input (optional):
        error_message: str - Custom error message

    Output:
        Failure result with retryable=True
    """

    handler_name = "test_scenarios.step_handlers.RetryableErrorStepHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Return a retryable error."""
        error_message = context.input_data.get(
            "error_message",
            "Temporary failure - please retry"
        )

        return StepHandlerResult.failure_handler_result(
            message=error_message,
            error_type="temporary_error",
            retryable=True,
            metadata={
                "handler": "RetryableErrorStepHandler",
                "timestamp": datetime.now(timezone.utc).isoformat(),
            },
        )


class PermanentErrorStepHandler(StepHandler):
    """Handler that returns permanent (non-retryable) errors.

    Useful for testing permanent failure scenarios in workflows.

    Input (optional):
        error_message: str - Custom error message

    Output:
        Failure result with retryable=False
    """

    handler_name = "test_scenarios.step_handlers.PermanentErrorStepHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Return a permanent error."""
        error_message = context.input_data.get(
            "error_message",
            "Permanent failure - do not retry"
        )

        return StepHandlerResult.failure_handler_result(
            message=error_message,
            error_type="permanent_error",
            retryable=False,
            metadata={
                "handler": "PermanentErrorStepHandler",
                "timestamp": datetime.now(timezone.utc).isoformat(),
            },
        )


__all__ = [
    "SuccessStepHandler",
    "RetryableErrorStepHandler",
    "PermanentErrorStepHandler",
]
