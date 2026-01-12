"""
Validate Refund Request Handler for Blog Post 04 - Customer Success namespace.

First step in the customer success refund workflow.
Validates ticket and customer information from the customer service platform.

TAS-137 Best Practices:
- get_input(): Access task context fields (ticket_id, customer_id, refund_amount, refund_reason)
- Note: This is the first step - no dependencies to access
"""

import uuid
from datetime import datetime, timedelta, timezone

from tasker_core import StepContext, StepHandler, StepHandlerResult


class ValidateRefundRequestHandler(StepHandler):
    """Validate customer refund request details."""

    handler_name = "team_scaling.customer_success.step_handlers.ValidateRefundRequestHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Execute the validate refund request step."""
        # TAS-137: Extract and validate inputs using get_input()
        ticket_id = context.get_input("ticket_id")
        customer_id = context.get_input("customer_id")
        refund_amount = context.get_input("refund_amount")
        refund_reason = context.get_input("refund_reason")

        # Validate required fields
        missing_fields = []
        if not ticket_id:
            missing_fields.append("ticket_id")
        if not customer_id:
            missing_fields.append("customer_id")
        if not refund_amount:
            missing_fields.append("refund_amount")

        if missing_fields:
            return self.failure(
                f"Missing required fields for refund validation: {', '.join(missing_fields)}",
                error_code="MISSING_REQUIRED_FIELDS",
                retryable=False,
            )

        # Simulate customer service system validation
        service_response = self._simulate_customer_service_validation(
            ticket_id, customer_id, refund_amount, refund_reason
        )

        # Ensure request is valid
        validation_error = self._check_request_validity(service_response)
        if validation_error:
            return validation_error

        now = datetime.now(timezone.utc).isoformat()

        return self.success(
            {
                "request_validated": True,
                "ticket_id": service_response["ticket_id"],
                "customer_id": service_response["customer_id"],
                "ticket_status": service_response["status"],
                "customer_tier": service_response["customer_tier"],
                "original_purchase_date": service_response["purchase_date"],
                "payment_id": service_response["payment_id"],
                "validation_timestamp": now,
                "namespace": "customer_success",
            },
            {
                "operation": "validate_refund_request",
                "service": "customer_service_platform",
                "ticket_id": ticket_id,
                "customer_tier": service_response["customer_tier"],
            },
        )

    def _simulate_customer_service_validation(
        self, ticket_id: str, customer_id: str, _refund_amount: float, _refund_reason: str
    ) -> dict:
        """Simulate customer service system validation."""
        # Simulate different ticket scenarios based on ticket_id patterns
        if "ticket_closed" in ticket_id:
            return {
                "status": "closed",
                "ticket_id": ticket_id,
                "customer_id": customer_id,
                "error": "Ticket is closed",
            }
        elif "ticket_cancelled" in ticket_id:
            return {
                "status": "cancelled",
                "ticket_id": ticket_id,
                "customer_id": customer_id,
                "error": "Ticket was cancelled",
            }
        elif "ticket_duplicate" in ticket_id:
            return {
                "status": "duplicate",
                "ticket_id": ticket_id,
                "customer_id": customer_id,
                "error": "Duplicate ticket",
            }
        else:
            # Success case - valid ticket
            purchase_date = (datetime.now(timezone.utc) - timedelta(days=30)).isoformat()
            return {
                "status": "open",
                "ticket_id": ticket_id,
                "customer_id": customer_id,
                "customer_tier": self._determine_customer_tier(customer_id),
                "purchase_date": purchase_date,
                "payment_id": f"pay_{uuid.uuid4().hex[:12]}",
                "ticket_created_at": (datetime.now(timezone.utc) - timedelta(days=2)).isoformat(),
                "agent_assigned": f"agent_{(hash(ticket_id) % 10) + 1}",
            }

    def _determine_customer_tier(self, customer_id: str) -> str:
        """Determine customer tier based on customer ID."""
        if "vip" in customer_id.lower() or "premium" in customer_id.lower():
            return "premium"
        elif "gold" in customer_id.lower():
            return "gold"
        else:
            return "standard"

    def _check_request_validity(self, service_response: dict) -> StepHandlerResult | None:
        """Check if the request is valid for processing."""
        status = service_response.get("status")

        if status in ("open", "in_progress"):
            return None  # Valid
        elif status == "closed":
            return self.failure(
                "Cannot process refund for closed ticket",
                error_code="TICKET_CLOSED",
                retryable=False,
            )
        elif status == "cancelled":
            return self.failure(
                "Cannot process refund for cancelled ticket",
                error_code="TICKET_CANCELLED",
                retryable=False,
            )
        elif status == "duplicate":
            return self.failure(
                "Cannot process refund for duplicate ticket",
                error_code="TICKET_DUPLICATE",
                retryable=False,
            )
        else:
            return self.failure(
                f"Unknown ticket status: {status}",
                error_code="UNKNOWN_STATUS",
                retryable=True,
            )
