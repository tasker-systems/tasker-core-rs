"""
Validate Payment Eligibility Handler for Blog Post 04 - Payments namespace.

First step in the payments refund workflow.
Validates if the payment can be refunded via the payment gateway.

TAS-137 Best Practices:
- get_input(): Access task context fields (payment_id, refund_amount, refund_reason, partial_refund)
- get_input_or(): Access task context with defaults (partial_refund)
- Note: This is the first step - no dependencies to access
"""

import re
from datetime import datetime, timedelta, timezone

from tasker_core import StepContext, StepHandler, StepHandlerResult


class ValidatePaymentEligibilityHandler(StepHandler):
    """Validate payment eligibility for refund."""

    handler_name = "team_scaling.payments.step_handlers.ValidatePaymentEligibilityHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Execute the validate payment eligibility step."""
        # TAS-137: Use get_input() for task context access
        payment_id = context.get_input("payment_id")
        refund_amount = context.get_input("refund_amount")
        _refund_reason = context.get_input("refund_reason")  # For audit logging
        # TAS-137: Use get_input_or() for task context with default
        partial_refund = context.get_input_or("partial_refund", False)

        # Validate required fields
        missing_fields = []
        if not payment_id:
            missing_fields.append("payment_id")
        if not refund_amount:
            missing_fields.append("refund_amount")

        if missing_fields:
            return self.failure(
                f"Missing required fields for payment validation: {', '.join(missing_fields)}",
                error_code="MISSING_REQUIRED_FIELDS",
                retryable=False,
            )

        # Validate refund amount is positive
        if refund_amount <= 0:
            return self.failure(
                f"Refund amount must be positive, got: {refund_amount}",
                error_code="INVALID_REFUND_AMOUNT",
                retryable=False,
            )

        # Validate payment ID format
        if not re.match(r"^pay_[a-zA-Z0-9_]+$", payment_id):
            return self.failure(
                f"Invalid payment ID format: {payment_id}",
                error_code="INVALID_PAYMENT_ID",
                retryable=False,
            )

        # Simulate payment gateway validation
        validation_result = self._simulate_payment_gateway_validation(
            payment_id, refund_amount, partial_refund
        )

        # Ensure payment is eligible
        eligibility_error = self._ensure_payment_eligible(validation_result)
        if eligibility_error:
            return eligibility_error

        now = datetime.now(timezone.utc).isoformat()

        return self.success(
            {
                "payment_validated": True,
                "payment_id": payment_id,
                "original_amount": validation_result["original_amount"],
                "refund_amount": refund_amount,
                "payment_method": validation_result["payment_method"],
                "gateway_provider": validation_result["gateway_provider"],
                "eligibility_status": validation_result["status"],
                "validation_timestamp": now,
                "namespace": "payments",
            },
            {
                "operation": "validate_payment_eligibility",
                "service": "payment_gateway",
                "payment_id": payment_id,
                "gateway_provider": validation_result["gateway_provider"],
            },
        )

    def _simulate_payment_gateway_validation(
        self, payment_id: str, refund_amount: float, _partial_refund: bool
    ) -> dict:
        """Simulate payment gateway validation."""
        # Simulate different payment scenarios based on payment_id patterns
        if "pay_test_insufficient" in payment_id:
            return {
                "status": "insufficient_funds",
                "reason": "Not enough funds available for refund",
                "payment_id": payment_id,
            }
        elif "pay_test_processing" in payment_id:
            return {
                "status": "processing",
                "reason": "Payment still processing",
                "payment_id": payment_id,
            }
        elif "pay_test_ineligible" in payment_id:
            return {
                "status": "ineligible",
                "reason": "Payment is past refund window",
                "payment_id": payment_id,
            }
        else:
            # Success case - payment is eligible
            transaction_date = (datetime.now(timezone.utc) - timedelta(days=5)).isoformat()
            return {
                "status": "eligible",
                "payment_id": payment_id,
                "original_amount": refund_amount + 1000,  # Original was higher
                "payment_method": "credit_card",
                "gateway_provider": "MockPaymentGateway",
                "transaction_date": transaction_date,
                "refundable_amount": refund_amount,
            }

    def _ensure_payment_eligible(self, validation_result: dict) -> StepHandlerResult | None:
        """Ensure payment is eligible for refund."""
        status = validation_result.get("status")

        if status == "eligible":
            return None  # Eligible
        elif status == "ineligible":
            return self.failure(
                f"Payment is not eligible for refund: {validation_result.get('reason')}",
                error_code="PAYMENT_INELIGIBLE",
                retryable=False,
            )
        elif status in ("processing", "pending"):
            return self.failure(
                "Payment is still processing, cannot refund yet",
                error_code="PAYMENT_PROCESSING",
                retryable=True,
            )
        elif status == "insufficient_funds":
            return self.failure(
                "Insufficient funds available for refund",
                error_code="INSUFFICIENT_FUNDS",
                retryable=False,
            )
        else:
            return self.failure(
                f"Unknown payment eligibility status: {status}",
                error_code="UNKNOWN_STATUS",
                retryable=True,
            )
