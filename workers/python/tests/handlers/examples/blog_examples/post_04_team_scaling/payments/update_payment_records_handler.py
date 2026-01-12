"""
Update Payment Records Handler for Blog Post 04 - Payments namespace.

Updates internal payment status and history after gateway refund.

TAS-137 Best Practices:
- get_input(): Access task context fields (refund_reason)
- get_input_or(): Access task context with defaults (refund_reason)
- get_dependency_result(): Access upstream step results (process_gateway_refund, validate_payment_eligibility)
- get_dependency_field(): Extract nested fields (payment_id, refund_id, refund_amount, gateway_transaction_id, original_amount)
"""

import uuid
from datetime import datetime, timezone

from tasker_core import StepContext, StepHandler, StepHandlerResult


class UpdatePaymentRecordsHandler(StepHandler):
    """Update internal payment records after refund."""

    handler_name = "team_scaling.payments.step_handlers.UpdatePaymentRecordsHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Execute the update payment records step."""
        # TAS-137: Validate dependency result exists
        refund_result = context.get_dependency_result("process_gateway_refund")

        if not refund_result or not refund_result.get("refund_processed"):
            return self.failure(
                "Gateway refund must be completed before updating records",
                error_code="MISSING_REFUND",
                retryable=False,
            )

        # TAS-137: Use get_dependency_field() for nested extraction
        payment_id = context.get_dependency_field("process_gateway_refund", "payment_id")
        refund_id = context.get_dependency_field("process_gateway_refund", "refund_id")
        refund_amount = context.get_dependency_field("process_gateway_refund", "refund_amount")
        gateway_transaction_id = context.get_dependency_field(
            "process_gateway_refund", "gateway_transaction_id"
        )
        _original_amount = context.get_dependency_field(
            "validate_payment_eligibility", "original_amount"
        )  # For audit logging

        # TAS-137: Use get_input_or() for task context with default
        refund_reason = context.get_input_or("refund_reason", "customer_request")

        # Simulate payment record update
        update_result = self._simulate_payment_record_update(
            payment_id, refund_id, refund_amount, refund_reason, gateway_transaction_id
        )

        # Ensure update was successful
        update_error = self._ensure_update_successful(update_result)
        if update_error:
            return update_error

        return self.success(
            {
                "records_updated": True,
                "payment_id": payment_id,
                "refund_id": refund_id,
                "record_id": update_result["record_id"],
                "payment_status": update_result["payment_status"],
                "refund_status": update_result["refund_status"],
                "history_entries_created": update_result["history_entries_created"],
                "updated_at": update_result["updated_at"],
                "namespace": "payments",
            },
            {
                "operation": "update_payment_records",
                "service": "payment_record_system",
                "payment_id": payment_id,
                "record_id": update_result["record_id"],
            },
        )

    def _simulate_payment_record_update(
        self,
        payment_id: str,
        refund_id: str,
        refund_amount: float,
        refund_reason: str,
        gateway_transaction_id: str,
    ) -> dict:
        """Simulate payment record update."""
        now = datetime.now(timezone.utc).isoformat()

        # Simulate different record update scenarios based on payment_id patterns
        if "pay_test_record_lock" in payment_id:
            return {"status": "locked", "error": "Record locked by another process"}
        elif "pay_test_record_error" in payment_id:
            return {"status": "error", "error": "Database error"}
        else:
            # Success case
            return {
                "status": "updated",
                "record_id": f"rec_{uuid.uuid4().hex[:16]}",
                "payment_id": payment_id,
                "refund_id": refund_id,
                "payment_status": "refunded",
                "refund_status": "completed",
                "history_entries_created": 2,
                "updated_at": now,
                "history_entries": [
                    {
                        "type": "refund_initiated",
                        "refund_id": refund_id,
                        "amount": refund_amount,
                        "reason": refund_reason,
                        "timestamp": now,
                    },
                    {
                        "type": "refund_completed",
                        "refund_id": refund_id,
                        "gateway_transaction_id": gateway_transaction_id,
                        "timestamp": now,
                    },
                ],
            }

    def _ensure_update_successful(self, update_result: dict) -> StepHandlerResult | None:
        """Ensure record update was successful."""
        status = update_result.get("status")

        if status in ("updated", "success"):
            return None  # Success
        elif status == "locked":
            return self.failure(
                "Payment record locked, will retry",
                error_code="RECORD_LOCKED",
                retryable=True,
            )
        elif status == "not_found":
            return self.failure(
                "Payment record not found",
                error_code="PAYMENT_NOT_FOUND",
                retryable=False,
            )
        elif status in ("error", "failed"):
            return self.failure(
                f"Record update failed: {update_result.get('error')}",
                error_code="UPDATE_FAILED",
                retryable=True,
            )
        else:
            return self.failure(
                f"Unknown update status: {status}",
                error_code="UNKNOWN_STATUS",
                retryable=True,
            )
