"""Create order handler for e-commerce checkout workflow.

TAS-137 Best Practices Demonstrated:
- get_input() for task context field access (cross-language standard)
- get_dependency_result() for upstream step results (auto-unwraps)
- Aggregating results from multiple upstream dependencies
"""

from __future__ import annotations

import logging
import random
import secrets
from datetime import datetime, timedelta, timezone
from typing import TYPE_CHECKING, Any

from tasker_core import StepHandler, StepHandlerResult
from tasker_core.errors import PermanentError

if TYPE_CHECKING:
    from tasker_core import StepContext

logger = logging.getLogger(__name__)


class CreateOrderHandler(StepHandler):
    """Create order record with all details from upstream steps.

    Input (from task context):
        customer_info: dict - Customer information

    Input (from dependencies):
        validate_cart: dict - Cart validation results
        process_payment: dict - Payment results
        update_inventory: dict - Inventory reservation results

    Output:
        order_id: int - Order identifier
        order_number: str - Human-readable order number
        status: str - Order status
        total_amount: float - Order total
        customer_email: str - Customer email
        estimated_delivery: str - Estimated delivery date
    """

    handler_name = "ecommerce.step_handlers.CreateOrderHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Create the order record."""
        # TAS-137: Use get_input() for task context access
        customer_info = context.get_input("customer_info")

        # TAS-137: Use get_dependency_result() for upstream step results (auto-unwraps)
        cart_validation = context.get_dependency_result("validate_cart")
        payment_result = context.get_dependency_result("process_payment")
        inventory_result = context.get_dependency_result("update_inventory")

        # Validate required inputs
        self._validate_inputs(customer_info, cart_validation, payment_result, inventory_result)

        logger.info(
            "CreateOrderHandler: Creating order - task_uuid=%s, customer=%s",
            context.task_uuid,
            customer_info.get("email"),
        )

        # Create the order record
        order_response = self._create_order_record(
            customer_info=customer_info,
            cart_validation=cart_validation,
            payment_result=payment_result,
            inventory_result=inventory_result,
            task_uuid=str(context.task_uuid),
        )

        order = order_response["order"]

        logger.info(
            "CreateOrderHandler: Order created - order_id=%s, order_number=%s",
            order["id"],
            order["order_number"],
        )

        return StepHandlerResult.success(
            {
                "order_id": order["id"],
                "order_number": order["order_number"],
                "status": order["status"],
                "total_amount": order["total_amount"],
                "customer_email": order["customer_email"],
                "created_at": order["created_at"],
                "estimated_delivery": self._calculate_estimated_delivery(),
            },
            metadata={
                "operation": "create_order",
                "execution_hints": {
                    "order_id": order["id"],
                    "order_number": order["order_number"],
                    "item_count": order["item_count"],
                    "total_amount": order["total_amount"],
                    "creation_timestamp": order_response["creation_timestamp"],
                },
                "http_headers": {
                    "X-Order-Service": "OrderManagement",
                    "X-Order-ID": str(order["id"]),
                    "X-Order-Number": order["order_number"],
                    "X-Order-Status": order["status"],
                },
                "input_refs": {
                    "customer_info": 'context.get_input("customer_info")',
                    "cart_validation": 'context.get_dependency_result("validate_cart")',
                    "payment_result": 'context.get_dependency_result("process_payment")',
                    "inventory_result": 'context.get_dependency_result("update_inventory")',
                },
            },
        )

    def _validate_inputs(
        self,
        customer_info: dict[str, Any] | None,
        cart_validation: dict[str, Any] | None,
        payment_result: dict[str, Any] | None,
        inventory_result: dict[str, Any] | None,
    ) -> None:
        """Validate required inputs are present."""
        if not customer_info:
            raise PermanentError(
                message="Customer information is required but was not provided",
                error_code="MISSING_CUSTOMER_INFO",
            )

        if not cart_validation or not cart_validation.get("validated_items"):
            raise PermanentError(
                message="Cart validation results are required but were not found from validate_cart step",
                error_code="MISSING_CART_VALIDATION",
            )

        if not payment_result or not payment_result.get("payment_id"):
            raise PermanentError(
                message="Payment results are required but were not found from process_payment step",
                error_code="MISSING_PAYMENT_RESULT",
            )

        if not inventory_result or not inventory_result.get("updated_products"):
            raise PermanentError(
                message="Inventory results are required but were not found from update_inventory step",
                error_code="MISSING_INVENTORY_RESULT",
            )

    def _create_order_record(
        self,
        customer_info: dict[str, Any],
        cart_validation: dict[str, Any],
        payment_result: dict[str, Any],
        inventory_result: dict[str, Any],
        task_uuid: str,
    ) -> dict[str, Any]:
        """Create the order record using validated inputs."""
        order_id = random.randint(1000, 9999)
        order_number = self._generate_order_number()
        now = datetime.now(timezone.utc)

        order_data = {
            "id": order_id,
            "order_number": order_number,
            "status": "confirmed",
            # Customer information
            "customer_email": customer_info.get("email"),
            "customer_name": customer_info.get("name"),
            "customer_phone": customer_info.get("phone"),
            # Order totals
            "subtotal": cart_validation.get("subtotal"),
            "tax_amount": cart_validation.get("tax"),
            "shipping_amount": cart_validation.get("shipping"),
            "total_amount": cart_validation.get("total"),
            # Payment information
            "payment_id": payment_result.get("payment_id"),
            "payment_status": "completed",
            "transaction_id": payment_result.get("transaction_id"),
            # Order items
            "items": cart_validation.get("validated_items"),
            "item_count": cart_validation.get("item_count"),
            # Inventory tracking
            "inventory_log_id": inventory_result.get("inventory_log_id"),
            # Tracking
            "task_uuid": task_uuid,
            "workflow_version": "1.0.0",
            # Timestamps
            "created_at": now.isoformat(),
            "updated_at": now.isoformat(),
            "placed_at": now.isoformat(),
        }

        return {
            "order": order_data,
            "creation_timestamp": now.isoformat(),
        }

    def _generate_order_number(self) -> str:
        """Generate a human-readable order number."""
        today = datetime.now(timezone.utc).strftime("%Y%m%d")
        suffix = secrets.token_hex(4).upper()
        return f"ORD-{today}-{suffix}"

    def _calculate_estimated_delivery(self) -> str:
        """Calculate estimated delivery date (7 days from now)."""
        delivery_date = datetime.now(timezone.utc) + timedelta(days=7)
        return delivery_date.strftime("%B %d, %Y")
