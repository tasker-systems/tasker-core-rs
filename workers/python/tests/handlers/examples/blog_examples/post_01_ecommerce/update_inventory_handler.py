"""Update inventory handler for e-commerce checkout workflow.

TAS-137 Best Practices Demonstrated:
- get_input() for task context field access (cross-language standard)
- get_dependency_result() for upstream step results (auto-unwraps)
- get_dependency_field() for nested field extraction from dependencies
- RetryableError for transient inventory issues
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

# Mock product inventory database (shared with ValidateCartHandler)
PRODUCTS: dict[int, dict[str, Any]] = {
    1: {"id": 1, "name": "Widget A", "stock": 100},
    2: {"id": 2, "name": "Widget B", "stock": 50},
    3: {"id": 3, "name": "Widget C", "stock": 25},
    4: {"id": 4, "name": "Widget D", "stock": 0},
    5: {"id": 5, "name": "Widget E", "stock": 10},
}


class UpdateInventoryHandler(StepHandler):
    """Reserve inventory for order items using mock inventory service.

    Input (from task context):
        customer_info: dict - Customer information

    Input (from dependency):
        validate_cart.validated_items: list[dict] - Validated cart items

    Output:
        updated_products: list[dict] - Products with updated stock
        total_items_reserved: int - Total quantity reserved
        inventory_changes: list[dict] - Change log entries
        inventory_log_id: str - Log identifier
    """

    handler_name = "ecommerce.step_handlers.UpdateInventoryHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Update inventory for cart items."""
        # TAS-137: Use get_dependency_result() for upstream step results (auto-unwraps)
        cart_validation = context.get_dependency_result("validate_cart")

        # TAS-137: Use get_input() for task context access
        customer_info = context.get_input("customer_info")

        # Validate required inputs
        self._validate_inputs(cart_validation, customer_info)

        validated_items = cart_validation.get("validated_items", [])

        logger.info(
            "UpdateInventoryHandler: Updating inventory - task_uuid=%s, item_count=%d",
            context.task_uuid,
            len(validated_items),
        )

        # Process inventory reservations
        reservation_response = self._process_inventory_reservations(
            validated_items=validated_items,
            _customer_info=customer_info,
            _task_uuid=str(context.task_uuid),
        )

        updated_products = reservation_response["updated_products"]
        inventory_changes = reservation_response["inventory_changes"]
        total_items_reserved = sum(p["quantity_reserved"] for p in updated_products)

        logger.info(
            "UpdateInventoryHandler: Inventory updated - products_updated=%d",
            len(updated_products),
        )

        return StepHandlerResult.success(
            {
                "updated_products": updated_products,
                "total_items_reserved": total_items_reserved,
                "inventory_changes": inventory_changes,
                "inventory_log_id": reservation_response["inventory_log_id"],
                "updated_at": datetime.now(timezone.utc).isoformat(),
            },
            metadata={
                "operation": "update_inventory",
                "execution_hints": {
                    "products_updated": len(updated_products),
                    "total_items_reserved": total_items_reserved,
                    "reservation_timestamp": reservation_response["reservation_timestamp"],
                },
                "http_headers": {
                    "X-Inventory-Service": "MockInventoryService",
                    "X-Reservation-Count": str(len(updated_products)),
                    "X-Total-Quantity": str(total_items_reserved),
                },
                "input_refs": {
                    "validated_items": 'context.get_dependency_field("validate_cart", "validated_items")',
                    "customer_info": 'context.get_input("customer_info")',
                },
            },
        )

    def _validate_inputs(
        self,
        cart_validation: dict[str, Any] | None,
        customer_info: dict[str, Any] | None,
    ) -> None:
        """Validate required inputs are present."""
        if not cart_validation or not cart_validation.get("validated_items"):
            raise PermanentError(
                message="Validated cart items are required but were not found from validate_cart step",
                error_code="MISSING_VALIDATED_ITEMS",
            )

        if not customer_info:
            raise PermanentError(
                message="Customer information is required but was not provided",
                error_code="MISSING_CUSTOMER_INFO",
            )

    def _process_inventory_reservations(
        self,
        validated_items: list[dict[str, Any]],
        _customer_info: dict[str, Any],
        _task_uuid: str,
    ) -> dict[str, Any]:
        """Process inventory reservations for all validated items."""
        updated_products = []
        inventory_changes = []

        for item in validated_items:
            product_id = item["product_id"]
            quantity = item["quantity"]
            product = PRODUCTS.get(product_id)

            if not product:
                raise PermanentError(
                    message=f"Product {product_id} not found",
                    error_code="PRODUCT_NOT_FOUND",
                )

            # Check availability
            stock_level = product["stock"]
            if stock_level < quantity:
                raise RetryableError(
                    message=(
                        f"Stock not available for {product['name']}. "
                        f"Available: {stock_level}, Needed: {quantity}"
                    ),
                    retry_after=30,
                    context={
                        "product_id": product["id"],
                        "product_name": product["name"],
                        "available_stock": stock_level,
                        "requested_quantity": quantity,
                    },
                )

            # Simulate inventory reservation
            reservation_id = f"rsv_{secrets.token_hex(8)}"

            updated_products.append(
                {
                    "product_id": product["id"],
                    "name": product["name"],
                    "previous_stock": stock_level,
                    "new_stock": stock_level - quantity,
                    "quantity_reserved": quantity,
                    "reservation_id": reservation_id,
                }
            )

            inventory_changes.append(
                {
                    "product_id": product["id"],
                    "change_type": "reservation",
                    "quantity": -quantity,
                    "reason": "order_checkout",
                    "timestamp": datetime.now(timezone.utc).isoformat(),
                    "reservation_id": reservation_id,
                    "inventory_log_id": f"log_{secrets.token_hex(6)}",
                }
            )

        return {
            "updated_products": updated_products,
            "inventory_changes": inventory_changes,
            "reservation_timestamp": datetime.now(timezone.utc).isoformat(),
            "inventory_log_id": f"log_{secrets.token_hex(8)}",
        }
