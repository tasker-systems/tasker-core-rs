"""E-commerce order processing handlers (Blog Post 01).

This module demonstrates a complete e-commerce checkout workflow with 5 sequential steps:

    validate_cart -> process_payment -> update_inventory -> create_order -> send_confirmation

TAS-137 Best Practices Demonstrated:
- get_input() for task context field access (cross-language standard)
- get_dependency_result() for upstream step results (auto-unwraps)
- get_dependency_field() for nested field extraction from dependencies
- PermanentError vs RetryableError for intelligent error classification
"""

from .create_order_handler import CreateOrderHandler
from .process_payment_handler import ProcessPaymentHandler
from .send_confirmation_handler import SendConfirmationHandler
from .update_inventory_handler import UpdateInventoryHandler
from .validate_cart_handler import ValidateCartHandler

__all__ = [
    "ValidateCartHandler",
    "ProcessPaymentHandler",
    "UpdateInventoryHandler",
    "CreateOrderHandler",
    "SendConfirmationHandler",
]
