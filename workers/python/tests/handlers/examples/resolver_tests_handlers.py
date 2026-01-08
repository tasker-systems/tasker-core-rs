"""TAS-93 Phase 5: Multi-Method Handlers for Resolver Chain E2E Testing.

This module demonstrates the method dispatch feature of the resolver chain.
Handlers have multiple entry points beyond the default `call` method, allowing
YAML templates to specify `method: "validate"` or `method: "process"` etc.

Example YAML configuration:
    handler:
      callable: resolver_tests.step_handlers.MultiMethodHandler
      method: validate  # Invokes validate() instead of call()

@see docs/ticket-specs/TAS-93/implementation-plan.md
"""

from __future__ import annotations

from typing import TYPE_CHECKING

from tasker_core import StepHandler, StepHandlerResult

if TYPE_CHECKING:
    from tasker_core import StepContext


class MultiMethodHandler(StepHandler):
    """Multi-method handler demonstrating method dispatch.

    Available methods:
    - call: Default entry point (standard processing)
    - validate: Validation-only path
    - process: Processing-specific path
    - refund: Refund-specific path (payment domain example)

    Each method returns a result with `invoked_method` so tests can verify
    which method was actually called.
    """

    handler_name = "resolver_tests.step_handlers.MultiMethodHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Default entry point - standard processing."""
        input_data = context.input_data.get("data", {})

        return StepHandlerResult.success(
            {
                "invoked_method": "call",
                "handler": "MultiMethodHandler",
                "message": "Default call method invoked",
                "input_received": input_data,
                "step_uuid": str(context.step_uuid),
            }
        )

    def validate(self, context: StepContext) -> StepHandlerResult:
        """Validation-only entry point.

        Can be invoked with: `method: "validate"`
        """
        input_data = context.input_data.get("data", {})

        # Simple validation logic for testing
        has_required_fields = "amount" in input_data

        if not has_required_fields:
            return StepHandlerResult.failure(
                message='Validation failed: missing required field "amount"',
                error_type="validation_error",
                retryable=False,
            )

        return StepHandlerResult.success(
            {
                "invoked_method": "validate",
                "handler": "MultiMethodHandler",
                "message": "Validation completed successfully",
                "validated": True,
                "input_validated": input_data,
                "step_uuid": str(context.step_uuid),
            }
        )

    def process(self, context: StepContext) -> StepHandlerResult:
        """Processing entry point.

        Can be invoked with: `method: "process"`
        """
        input_data = context.input_data.get("data", {})
        amount = input_data.get("amount", 0)

        # Simple processing logic for testing
        processed_amount = amount * 1.1  # Add 10% processing fee

        return StepHandlerResult.success(
            {
                "invoked_method": "process",
                "handler": "MultiMethodHandler",
                "message": "Processing completed",
                "original_amount": amount,
                "processed_amount": processed_amount,
                "processing_fee": processed_amount - amount,
                "step_uuid": str(context.step_uuid),
            }
        )

    def refund(self, context: StepContext) -> StepHandlerResult:
        """Refund entry point.

        Can be invoked with: `method: "refund"`
        Demonstrates payment domain method dispatch pattern.
        """
        import time

        input_data = context.input_data.get("data", {})
        amount = input_data.get("amount", 0)
        reason = input_data.get("reason", "not_specified")

        return StepHandlerResult.success(
            {
                "invoked_method": "refund",
                "handler": "MultiMethodHandler",
                "message": "Refund processed",
                "refund_amount": amount,
                "refund_reason": reason,
                "refund_id": f"refund_{int(time.time())}",
                "step_uuid": str(context.step_uuid),
            }
        )


class AlternateMethodHandler(StepHandler):
    """Second multi-method handler for testing resolver chain with different handlers.

    This handler is used to verify that the resolver chain can find
    handlers by different callable addresses.
    """

    handler_name = "resolver_tests.step_handlers.AlternateMethodHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Default entry point."""
        return StepHandlerResult.success(
            {
                "invoked_method": "call",
                "handler": "AlternateMethodHandler",
                "message": "Alternate handler default method",
                "step_uuid": str(context.step_uuid),
            }
        )

    def execute_action(self, context: StepContext) -> StepHandlerResult:
        """Custom action method.

        Can be invoked with: `method: "execute_action"`
        """
        action = context.input_data.get("action_type", "default_action")

        return StepHandlerResult.success(
            {
                "invoked_method": "execute_action",
                "handler": "AlternateMethodHandler",
                "message": "Custom action executed",
                "action_type": action,
                "step_uuid": str(context.step_uuid),
            }
        )


__all__ = [
    "MultiMethodHandler",
    "AlternateMethodHandler",
]
