"""Blog example handlers demonstrating real-world workflow patterns.

This package contains handlers from the Tasker blog series:

- post_01_ecommerce: E-commerce order processing (linear workflow)
- post_02_data_pipeline: Analytics data pipeline (DAG workflow) - TODO
- post_03_microservices_coordination: User registration (tree workflow) - TODO
- post_04_team_scaling: Refund processing with namespaces - TODO

Each post demonstrates progressively more complex workflow patterns
using idiomatic Python with TAS-137 StepContext best practices.
"""

# Post 01: E-commerce Order Processing (Linear Workflow)
from .post_01_ecommerce import (
    CreateOrderHandler,
    ProcessPaymentHandler,
    SendConfirmationHandler,
    UpdateInventoryHandler,
    ValidateCartHandler,
)

__all__ = [
    # Post 01 - E-commerce
    "ValidateCartHandler",
    "ProcessPaymentHandler",
    "UpdateInventoryHandler",
    "CreateOrderHandler",
    "SendConfirmationHandler",
]
