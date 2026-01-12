"""
Notify Customer Handler for Blog Post 04 - Payments namespace.

Final step in the payments refund workflow.
Sends refund confirmation notification to the customer.

TAS-137 Best Practices:
- get_input(): Access task context fields (customer_email, refund_reason)
- get_dependency_result(): Access upstream step results (process_gateway_refund)
- get_dependency_field(): Extract nested fields (refund_id, refund_amount, payment_id, estimated_arrival)
"""

import re
import uuid
from datetime import datetime, timezone

from tasker_core import StepContext, StepHandler, StepHandlerResult


class NotifyCustomerHandler(StepHandler):
    """Send refund confirmation notification to customer."""

    handler_name = "team_scaling.payments.step_handlers.NotifyCustomerHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Execute the notify customer step."""
        # TAS-137: Validate dependency result exists
        refund_result = context.get_dependency_result("process_gateway_refund")

        if not refund_result or not refund_result.get("refund_processed"):
            return self.failure(
                "Refund must be processed before sending notification",
                error_code="MISSING_REFUND",
                retryable=False,
            )

        # TAS-137: Use get_input() for task context access
        customer_email = context.get_input("customer_email")
        if not customer_email:
            return self.failure(
                "Customer email is required for notification",
                error_code="MISSING_CUSTOMER_EMAIL",
                retryable=False,
            )

        # Validate email format
        if not re.match(r"^[^@\s]+@[^@\s]+$", customer_email):
            return self.failure(
                f"Invalid customer email format: {customer_email}",
                error_code="INVALID_EMAIL_FORMAT",
                retryable=False,
            )

        # TAS-137: Use get_dependency_field() for nested extraction
        refund_id = context.get_dependency_field("process_gateway_refund", "refund_id")
        refund_amount = context.get_dependency_field("process_gateway_refund", "refund_amount")
        _payment_id = context.get_dependency_field(
            "process_gateway_refund", "payment_id"
        )  # For audit logging
        _estimated_arrival = context.get_dependency_field(
            "process_gateway_refund", "estimated_arrival"
        )  # For enhanced notifications

        # TAS-137: Use get_input_or() for task context with default
        _refund_reason = context.get_input_or(
            "refund_reason", "customer_request"
        )  # For audit logging

        # Simulate notification sending
        notification_result = self._simulate_notification_sending(
            customer_email, refund_id, refund_amount
        )

        # Ensure notification was sent
        notification_error = self._ensure_notification_sent(notification_result)
        if notification_error:
            return notification_error

        return self.success(
            {
                "notification_sent": True,
                "customer_email": customer_email,
                "message_id": notification_result["message_id"],
                "notification_type": "refund_confirmation",
                "sent_at": notification_result["sent_at"],
                "delivery_status": notification_result["delivery_status"],
                "refund_id": refund_id,
                "refund_amount": refund_amount,
                "namespace": "payments",
            },
            {
                "operation": "notify_customer",
                "service": "email_service",
                "customer_email": customer_email,
                "message_id": notification_result["message_id"],
            },
        )

    def _simulate_notification_sending(
        self, customer_email: str, _refund_id: str, refund_amount: float
    ) -> dict:
        """Simulate notification sending."""
        now = datetime.now(timezone.utc).isoformat()

        # Simulate different notification scenarios based on email patterns
        if "@test_bounce" in customer_email:
            return {"status": "bounced", "error": "Email bounced"}
        elif "@test_invalid" in customer_email:
            return {"status": "invalid", "error": "Invalid email address"}
        elif "@test_rate_limit" in customer_email:
            return {"status": "rate_limited", "error": "Rate limit exceeded"}
        else:
            # Success case
            return {
                "status": "sent",
                "message_id": f"msg_{uuid.uuid4().hex[:24]}",
                "delivery_status": "delivered",
                "recipient": customer_email,
                "sent_at": now,
                "notification_type": "refund_confirmation",
                "subject": f"Your refund of ${refund_amount / 100:.2f} has been processed",
                "template_id": "refund_confirmation_v2",
            }

    def _ensure_notification_sent(self, notification_result: dict) -> StepHandlerResult | None:
        """Ensure notification was sent successfully."""
        status = notification_result.get("status")

        if status in ("sent", "delivered", "queued"):
            return None  # Success
        elif status == "bounced":
            return self.failure(
                "Customer email bounced",
                error_code="EMAIL_BOUNCED",
                retryable=False,
            )
        elif status == "invalid":
            return self.failure(
                "Invalid customer email address",
                error_code="INVALID_EMAIL",
                retryable=False,
            )
        elif status == "rate_limited":
            return self.failure(
                "Email service rate limited, will retry",
                error_code="RATE_LIMITED",
                retryable=True,
            )
        elif status in ("failed", "error"):
            return self.failure(
                f"Notification failed: {notification_result.get('error')}",
                error_code="NOTIFICATION_FAILED",
                retryable=True,
            )
        else:
            return self.failure(
                f"Unknown notification status: {status}",
                error_code="UNKNOWN_STATUS",
                retryable=True,
            )
