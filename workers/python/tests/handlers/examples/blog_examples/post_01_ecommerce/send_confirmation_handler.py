"""Send confirmation handler for e-commerce checkout workflow.

TAS-137 Best Practices Demonstrated:
- get_input() for task context field access (cross-language standard)
- get_dependency_result() for upstream step results (auto-unwraps)
- PermanentError for invalid email addresses
- RetryableError for transient email service issues
"""

from __future__ import annotations

import logging
import re
import secrets
from datetime import datetime, timezone
from typing import TYPE_CHECKING, Any

from tasker_core import StepHandler, StepHandlerResult
from tasker_core.errors import PermanentError, RetryableError

if TYPE_CHECKING:
    from tasker_core import StepContext

logger = logging.getLogger(__name__)


class SendConfirmationHandler(StepHandler):
    """Send order confirmation email to customer.

    Input (from task context):
        customer_info: dict - Customer information with email

    Input (from dependencies):
        create_order: dict - Order creation results
        validate_cart: dict - Cart validation results

    Output:
        email_sent: bool - Whether email was sent
        recipient: str - Email recipient
        email_type: str - Type of email sent
        message_id: str - Email message identifier
    """

    handler_name = "ecommerce.step_handlers.SendConfirmationHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Send the confirmation email."""
        # TAS-137: Use get_input() for task context access
        customer_info = context.get_input("customer_info")

        # TAS-137: Use get_dependency_result() for upstream step results (auto-unwraps)
        order_result = context.get_dependency_result("create_order")
        cart_validation = context.get_dependency_result("validate_cart")

        # Validate required inputs
        self._validate_inputs(customer_info, order_result, cart_validation)

        customer_email = customer_info.get("email")

        logger.info(
            "SendConfirmationHandler: Sending confirmation email - task_uuid=%s, recipient=%s",
            context.task_uuid,
            customer_email,
        )

        # Send confirmation email
        email_response = self._send_confirmation_email(
            customer_info=customer_info,
            order_result=order_result,
            cart_validation=cart_validation,
        )

        delivery_result = email_response["delivery_result"]

        logger.info(
            "SendConfirmationHandler: Confirmation email sent - recipient=%s",
            customer_email,
        )

        return StepHandlerResult.success(
            {
                "email_sent": True,
                "recipient": customer_email,
                "email_type": "order_confirmation",
                "sent_at": datetime.now(timezone.utc).isoformat(),
                "message_id": delivery_result.get("message_id", f"mock_{secrets.token_hex(8)}"),
            },
            metadata={
                "operation": "send_confirmation_email",
                "execution_hints": {
                    "recipient": customer_email,
                    "email_type": "order_confirmation",
                    "delivery_status": delivery_result.get("status"),
                    "sent_timestamp": email_response["sent_timestamp"],
                },
                "http_headers": {
                    "X-Email-Service": "MockEmailService",
                    "X-Message-ID": delivery_result.get("message_id", "N/A"),
                    "X-Recipient": customer_email,
                },
                "input_refs": {
                    "customer_info": 'context.get_input("customer_info")',
                    "order_result": 'context.get_dependency_result("create_order")',
                    "cart_validation": 'context.get_dependency_result("validate_cart")',
                },
            },
        )

    def _validate_inputs(
        self,
        customer_info: dict[str, Any] | None,
        order_result: dict[str, Any] | None,
        cart_validation: dict[str, Any] | None,
    ) -> None:
        """Validate required inputs are present."""
        if not customer_info or not customer_info.get("email"):
            raise PermanentError(
                message="Customer email is required but was not provided",
                error_code="MISSING_CUSTOMER_EMAIL",
            )

        if not order_result or not order_result.get("order_id"):
            raise PermanentError(
                message="Order results are required but were not found from create_order step",
                error_code="MISSING_ORDER_RESULT",
            )

        if not cart_validation or not cart_validation.get("validated_items"):
            raise PermanentError(
                message="Cart validation results are required but were not found from validate_cart step",
                error_code="MISSING_CART_VALIDATION",
            )

    def _send_confirmation_email(
        self,
        customer_info: dict[str, Any],
        order_result: dict[str, Any],
        cart_validation: dict[str, Any],
    ) -> dict[str, Any]:
        """Send confirmation email using validated inputs."""
        # Prepare email data
        email_data = {
            "to": customer_info.get("email"),
            "customer_name": customer_info.get("name"),
            "order_number": order_result.get("order_number"),
            "order_id": order_result.get("order_id"),
            "total_amount": order_result.get("total_amount"),
            "estimated_delivery": order_result.get("estimated_delivery"),
            "items": cart_validation.get("validated_items"),
            "order_url": f"https://example.com/orders/{order_result.get('order_id')}",
        }

        # Simulate email sending
        delivery_result = self._simulate_email_delivery(email_data)

        # Ensure email was sent successfully
        self._ensure_email_sent_successfully(delivery_result)

        return {
            "delivery_result": delivery_result,
            "customer_email": customer_info.get("email"),
            "email_data": email_data,
            "sent_timestamp": datetime.now(timezone.utc).isoformat(),
        }

    def _simulate_email_delivery(self, email_data: dict[str, Any]) -> dict[str, Any]:
        """Simulate email delivery with realistic responses."""
        email = email_data.get("to")

        # Validate email address format
        if not email or not re.match(r"^[^@\s]+@[^@\s]+$", email):
            return {
                "status": "invalid_email",
                "error": "Invalid email address",
                "email": email,
            }

        # Success case
        return {
            "status": "sent",
            "message_id": f"msg_{secrets.token_hex(12)}",
            "email": email,
            "sent_at": datetime.now(timezone.utc).isoformat(),
        }

    def _ensure_email_sent_successfully(self, delivery_result: dict[str, Any]) -> None:
        """Ensure email was sent successfully, handling different error types."""
        status = delivery_result.get("status")

        if status in ("sent", "delivered"):
            return

        if status == "rate_limited":
            raise RetryableError(
                message="Email service rate limited",
                retry_after=60,
                context={"delivery_status": "rate_limited"},
            )

        if status in ("service_unavailable", "timeout"):
            raise RetryableError(
                message="Email service temporarily unavailable",
                retry_after=30,
                context={"delivery_status": status},
            )

        if status == "invalid_email":
            raise PermanentError(
                message="Invalid email address provided",
                error_code="INVALID_EMAIL",
                context={"email": delivery_result.get("email")},
            )

        # Unknown status or generic error - treat as retryable
        error_message = delivery_result.get("error", "Unknown email delivery error")
        raise RetryableError(
            message=f"Email delivery failed: {error_message}",
            retry_after=30,
            context={
                "delivery_status": status,
                "delivery_result": delivery_result,
            },
        )
