"""
Process Gateway Refund Handler for Blog Post 04 - Payments namespace.

Executes the refund through the payment processor gateway.

TAS-137 Best Practices:
- get_input(): Access task context fields (refund_reason, partial_refund)
- get_input_or(): Access task context with defaults (refund_reason, partial_refund)
- get_dependency_result(): Access upstream step results (validate_payment_eligibility)
- get_dependency_field(): Extract nested fields (payment_id, refund_amount, original_amount)
"""

import uuid
from datetime import datetime, timedelta, timezone

from tasker_core import StepContext, StepHandler, StepHandlerResult


class ProcessGatewayRefundHandler(StepHandler):
    """Process refund through payment gateway."""

    handler_name = "team_scaling.payments.step_handlers.ProcessGatewayRefundHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Execute the process gateway refund step."""
        # TAS-137: Validate dependency result exists
        validation_result = context.get_dependency_result("validate_payment_eligibility")

        if not validation_result or not validation_result.get("payment_validated"):
            return self.failure(
                "Payment validation must be completed before processing refund",
                error_code="MISSING_VALIDATION",
                retryable=False,
            )

        # TAS-137: Use get_dependency_field() for nested extraction
        payment_id = context.get_dependency_field("validate_payment_eligibility", "payment_id")
        refund_amount = context.get_dependency_field(
            "validate_payment_eligibility", "refund_amount"
        )
        _original_amount = context.get_dependency_field(
            "validate_payment_eligibility", "original_amount"
        )  # For audit logging

        # TAS-137: Use get_input_or() for task context with defaults
        refund_reason = context.get_input_or("refund_reason", "customer_request")
        _partial_refund = context.get_input_or("partial_refund", False)  # For enhanced processing

        # Simulate gateway refund processing
        refund_result = self._simulate_gateway_refund(payment_id, refund_amount, refund_reason)

        # Ensure refund was successful
        refund_error = self._ensure_refund_successful(refund_result)
        if refund_error:
            return refund_error

        return self.success(
            {
                "refund_processed": True,
                "refund_id": refund_result["refund_id"],
                "payment_id": payment_id,
                "refund_amount": refund_amount,
                "refund_status": refund_result["status"],
                "gateway_transaction_id": refund_result["gateway_transaction_id"],
                "gateway_provider": refund_result["gateway_provider"],
                "processed_at": refund_result["processed_at"],
                "estimated_arrival": refund_result["estimated_arrival"],
                "namespace": "payments",
            },
            {
                "operation": "process_gateway_refund",
                "service": "payment_gateway",
                "refund_id": refund_result["refund_id"],
                "gateway_provider": refund_result["gateway_provider"],
            },
        )

    def _simulate_gateway_refund(
        self, payment_id: str, refund_amount: float, _refund_reason: str
    ) -> dict:
        """Simulate gateway refund processing."""
        now = datetime.now(timezone.utc)

        # Simulate different gateway responses based on payment_id patterns
        if "pay_test_gateway_timeout" in payment_id:
            return {"status": "timeout", "error": "Gateway timeout"}
        elif "pay_test_gateway_error" in payment_id:
            return {"status": "failed", "error": "Gateway error"}
        elif "pay_test_rate_limit" in payment_id:
            return {"status": "rate_limited", "error": "Rate limit exceeded"}
        else:
            # Success case
            return {
                "status": "processed",
                "refund_id": f"rfnd_{uuid.uuid4().hex[:24]}",
                "payment_id": payment_id,
                "gateway_transaction_id": f"gtx_{uuid.uuid4().hex[:20]}",
                "gateway_provider": "MockPaymentGateway",
                "refund_amount": refund_amount,
                "processed_at": now.isoformat(),
                "estimated_arrival": (now + timedelta(days=5)).isoformat(),
            }

    def _ensure_refund_successful(self, refund_result: dict) -> StepHandlerResult | None:
        """Ensure refund was processed successfully."""
        status = refund_result.get("status")

        if status in ("processed", "succeeded"):
            return None  # Success
        elif status == "failed":
            return self.failure(
                f"Gateway refund failed: {refund_result.get('error')}",
                error_code="GATEWAY_REFUND_FAILED",
                retryable=False,
            )
        elif status == "timeout":
            return self.failure(
                "Gateway timeout, will retry",
                error_code="GATEWAY_TIMEOUT",
                retryable=True,
            )
        elif status == "rate_limited":
            return self.failure(
                "Gateway rate limited, will retry",
                error_code="RATE_LIMITED",
                retryable=True,
            )
        elif status == "pending":
            return self.failure(
                "Refund pending, checking again",
                error_code="REFUND_PENDING",
                retryable=True,
            )
        else:
            return self.failure(
                f"Unknown refund status: {status}",
                error_code="UNKNOWN_STATUS",
                retryable=True,
            )
