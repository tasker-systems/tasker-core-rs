"""Process payment handler for e-commerce checkout workflow.

TAS-137 Best Practices Demonstrated:
- get_input() for task context field access (cross-language standard)
- get_dependency_field() for nested field extraction from dependencies
- PermanentError for payment declines (card issues)
- RetryableError for transient gateway issues
"""

from __future__ import annotations

import logging
import secrets
from datetime import datetime, timezone
from typing import TYPE_CHECKING, Any

from tasker_core import StepHandler, StepHandlerResult
from tasker_core.errors import PermanentError, RetryableError

if TYPE_CHECKING:
    from tasker_core import StepContext

logger = logging.getLogger(__name__)


class ProcessPaymentHandler(StepHandler):
    """Process customer payment using mock payment gateway.

    Input (from task context):
        payment_info: dict - Payment method, token, and amount

    Input (from dependency):
        validate_cart.total: float - Amount to charge

    Output:
        payment_id: str - Payment identifier
        amount_charged: float - Amount charged
        currency: str - Currency code
        transaction_id: str - Gateway transaction ID
        status: str - Payment status
    """

    handler_name = "ecommerce.step_handlers.ProcessPaymentHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Process the payment."""
        # TAS-137: Use get_input() for task context access
        payment_info = context.get_input("payment_info")

        # TAS-137: Use get_dependency_field() for nested field extraction
        amount_to_charge = context.get_dependency_field("validate_cart", "total")

        # Validate required inputs
        self._validate_inputs(payment_info, amount_to_charge)

        payment_method = payment_info["method"]
        payment_token = payment_info["token"]
        provided_amount = payment_info.get("amount", 0)

        # Validate payment amount matches cart total
        if abs(float(provided_amount) - float(amount_to_charge)) > 0.01:
            raise PermanentError(
                message=(
                    f"Payment amount mismatch. "
                    f"Expected: ${amount_to_charge}, Provided: ${provided_amount}"
                ),
                error_code="PAYMENT_AMOUNT_MISMATCH",
                context={
                    "expected_amount": amount_to_charge,
                    "provided_amount": provided_amount,
                },
            )

        logger.info(
            "ProcessPaymentHandler: Processing payment - task_uuid=%s, amount=$%.2f",
            context.task_uuid,
            amount_to_charge,
        )

        # Simulate payment processing
        payment_result = self._simulate_payment_processing(
            amount=amount_to_charge,
            method=payment_method,
            token=payment_token,
        )

        # Handle payment result
        self._ensure_payment_successful(payment_result)

        logger.info(
            "ProcessPaymentHandler: Payment processed - payment_id=%s, amount_charged=$%.2f",
            payment_result["payment_id"],
            payment_result["amount_charged"],
        )

        return StepHandlerResult.success(
            {
                "payment_id": payment_result["payment_id"],
                "amount_charged": payment_result["amount_charged"],
                "currency": payment_result.get("currency", "USD"),
                "payment_method_type": payment_result.get("payment_method_type", "credit_card"),
                "transaction_id": payment_result["transaction_id"],
                "processed_at": datetime.now(timezone.utc).isoformat(),
                "status": "completed",
            },
            metadata={
                "operation": "process_payment",
                "execution_hints": {
                    "payment_gateway": "mock",
                    "amount_charged": payment_result["amount_charged"],
                    "fees": payment_result.get("fees", {}),
                    "processing_time_ms": 150,
                },
                "http_headers": {
                    "X-Payment-Gateway": "SimulatedPaymentService",
                    "X-Payment-ID": payment_result["payment_id"],
                    "X-Transaction-ID": payment_result["transaction_id"],
                },
                "input_refs": {
                    "amount": 'context.get_dependency_field("validate_cart", "total")',
                    "payment_info": 'context.get_input("payment_info")',
                },
            },
        )

    def _validate_inputs(
        self, payment_info: dict[str, Any] | None, amount_to_charge: float | None
    ) -> None:
        """Validate required inputs are present."""
        if not payment_info:
            raise PermanentError(
                message="Payment information is required but was not provided",
                error_code="MISSING_PAYMENT_INFO",
            )

        if not payment_info.get("method"):
            raise PermanentError(
                message="Payment method is required but was not provided",
                error_code="MISSING_PAYMENT_METHOD",
            )

        if not payment_info.get("token"):
            raise PermanentError(
                message="Payment token is required but was not provided",
                error_code="MISSING_PAYMENT_TOKEN",
            )

        if amount_to_charge is None:
            raise PermanentError(
                message="Cart total is required but was not found from validate_cart step",
                error_code="MISSING_CART_TOTAL",
            )

    def _simulate_payment_processing(
        self, amount: float, method: str, token: str
    ) -> dict[str, Any]:
        """Simulate payment processing with realistic responses."""
        # Test tokens for different error scenarios
        error_responses: dict[str, dict[str, Any]] = {
            "tok_test_declined": {
                "status": "card_declined",
                "error": "Card was declined by issuer",
                "payment_id": None,
                "transaction_id": None,
            },
            "tok_test_insufficient_funds": {
                "status": "insufficient_funds",
                "error": "Insufficient funds",
                "payment_id": None,
                "transaction_id": None,
            },
            "tok_test_network_error": {
                "status": "timeout",
                "error": "Payment gateway timeout",
                "payment_id": None,
                "transaction_id": None,
            },
        }

        if token in error_responses:
            return error_responses[token]

        # Success case
        return {
            "payment_id": f"pay_{secrets.token_hex(12)}",
            "status": "succeeded",
            "amount_charged": amount,
            "currency": "USD",
            "payment_method_type": method,
            "transaction_id": f"txn_{secrets.token_hex(12)}",
            "fees": self._calculate_payment_fees(amount),
        }

    def _calculate_payment_fees(self, amount: float) -> dict[str, float]:
        """Calculate payment processing fees."""
        processing_fee = round(amount * 0.029, 2)  # 2.9%
        fixed_fee = 0.30
        return {
            "processing_fee": processing_fee,
            "fixed_fee": fixed_fee,
            "total_fees": round(processing_fee + fixed_fee, 2),
        }

    def _ensure_payment_successful(self, payment_result: dict[str, Any]) -> None:
        """Ensure payment was successful, handling different error types."""
        status = payment_result.get("status")

        if status == "succeeded":
            return

        if status in ("insufficient_funds", "card_declined", "invalid_card"):
            # Permanent customer/card issues - don't retry
            error_message = payment_result.get("error", "Payment declined")
            raise PermanentError(
                message=f"Payment declined: {error_message}",
                error_code="PAYMENT_DECLINED",
                context={"payment_status": status},
            )

        if status == "rate_limited":
            raise RetryableError(
                message="Payment service rate limited",
                retry_after=30,
                context={"payment_status": status},
            )

        if status in ("service_unavailable", "timeout"):
            raise RetryableError(
                message="Payment service temporarily unavailable",
                retry_after=15,
                context={"payment_status": status},
            )

        # Unknown status - treat as retryable
        error_message = payment_result.get("error", "Unknown payment error")
        raise RetryableError(
            message=f"Payment service returned unknown status: {error_message}",
            retry_after=30,
            context={"payment_status": status, "payment_result": payment_result},
        )
