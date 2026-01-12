"""Validate cart handler for e-commerce checkout workflow.

TAS-137 Best Practices Demonstrated:
- get_input() for task context field access (cross-language standard)
- PermanentError for validation failures
- RetryableError for transient inventory issues
"""

from __future__ import annotations

import logging
from datetime import datetime, timezone
from typing import TYPE_CHECKING, Any

from tasker_core import StepHandler, StepHandlerResult
from tasker_core.errors import PermanentError, RetryableError

if TYPE_CHECKING:
    from tasker_core import StepContext

logger = logging.getLogger(__name__)

# Mock product database for e-commerce demonstration
PRODUCTS: dict[int, dict[str, Any]] = {
    1: {"id": 1, "name": "Widget A", "price": 29.99, "stock": 100, "active": True},
    2: {"id": 2, "name": "Widget B", "price": 49.99, "stock": 50, "active": True},
    3: {"id": 3, "name": "Widget C", "price": 19.99, "stock": 25, "active": True},
    4: {"id": 4, "name": "Widget D", "price": 39.99, "stock": 0, "active": True},  # Out of stock
    5: {"id": 5, "name": "Widget E", "price": 59.99, "stock": 10, "active": False},  # Inactive
}


class ValidateCartHandler(StepHandler):
    """Validates cart items, checks product availability, and calculates totals.

    Input (from task context):
        cart_items: list[dict] - Array of {product_id, quantity}

    Output:
        validated_items: list[dict] - Validated items with prices
        subtotal: float - Cart subtotal
        tax: float - Calculated tax
        shipping: float - Shipping cost
        total: float - Grand total
        item_count: int - Number of items
    """

    handler_name = "ecommerce.step_handlers.ValidateCartHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Validate cart and calculate totals."""
        # TAS-137: Use get_input() for task context access
        cart_items = context.get_input("cart_items")

        if not cart_items:
            raise PermanentError(
                message="Cart items are required but were not provided",
                error_code="MISSING_CART_ITEMS",
            )

        logger.info(
            "ValidateCartHandler: Validating cart - task_uuid=%s, item_count=%d",
            context.task_uuid,
            len(cart_items),
        )

        # Validate each cart item has required fields
        self._validate_cart_item_structure(cart_items)

        # Validate each item and build validated items list
        validated_items = self._validate_cart_items(cart_items)

        # Calculate totals
        subtotal = sum(item["line_total"] for item in validated_items)
        tax_rate = 0.08  # 8% tax rate
        tax = round(subtotal * tax_rate, 2)
        shipping = self._calculate_shipping(validated_items)
        total = subtotal + tax + shipping

        logger.info(
            "ValidateCartHandler: Cart validation completed - subtotal=$%.2f, total=$%.2f",
            subtotal,
            total,
        )

        return StepHandlerResult.success(
            {
                "validated_items": validated_items,
                "subtotal": subtotal,
                "tax": tax,
                "shipping": shipping,
                "total": total,
                "item_count": len(validated_items),
                "validated_at": datetime.now(timezone.utc).isoformat(),
            },
            metadata={
                "operation": "validate_cart",
                "execution_hints": {
                    "items_validated": len(validated_items),
                    "total_amount": total,
                    "tax_rate": tax_rate,
                    "shipping_cost": shipping,
                },
                "input_refs": {
                    "cart_items": 'context.get_input("cart_items")',
                },
            },
        )

    def _validate_cart_item_structure(self, cart_items: list[dict[str, Any]]) -> None:
        """Validate each cart item has required fields."""
        for index, item in enumerate(cart_items):
            if not item.get("product_id"):
                raise PermanentError(
                    message=f"Product ID is required for cart item {index + 1}",
                    error_code="MISSING_PRODUCT_ID",
                )

            quantity = item.get("quantity")
            if not quantity or quantity <= 0:
                raise PermanentError(
                    message=f"Valid quantity is required for cart item {index + 1}",
                    error_code="INVALID_QUANTITY",
                )

    def _validate_cart_items(self, cart_items: list[dict[str, Any]]) -> list[dict[str, Any]]:
        """Validate each cart item exists and is available."""
        validated_items = []

        for item in cart_items:
            product_id = item["product_id"]
            quantity = item["quantity"]
            product = PRODUCTS.get(product_id)

            if not product:
                raise PermanentError(
                    message=f"Product {product_id} not found",
                    error_code="PRODUCT_NOT_FOUND",
                )

            if not product["active"]:
                raise PermanentError(
                    message=f"Product {product['name']} is no longer available",
                    error_code="PRODUCT_INACTIVE",
                )

            if product["stock"] < quantity:
                # Temporary failure - inventory might be updated soon
                raise RetryableError(
                    message=(
                        f"Insufficient stock for {product['name']}. "
                        f"Available: {product['stock']}, Requested: {quantity}"
                    ),
                    retry_after=30,
                    context={
                        "product_id": product["id"],
                        "product_name": product["name"],
                        "available_stock": product["stock"],
                        "requested_quantity": quantity,
                    },
                )

            validated_items.append(
                {
                    "product_id": product["id"],
                    "name": product["name"],
                    "price": product["price"],
                    "quantity": quantity,
                    "line_total": round(product["price"] * quantity, 2),
                }
            )

        return validated_items

    def _calculate_shipping(self, items: list[dict[str, Any]]) -> float:
        """Calculate shipping based on total weight."""
        total_weight = sum(item["quantity"] * 0.5 for item in items)  # 0.5 lbs per item

        if total_weight <= 2:
            return 5.99
        elif total_weight <= 10:
            return 9.99
        else:
            return 14.99
