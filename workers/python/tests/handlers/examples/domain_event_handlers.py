"""Domain event publishing handlers for E2E testing.

TAS-65/TAS-69: Domain Event Publication Architecture

These handlers demonstrate domain event publishing with:
- Success condition events (published only on successful step completion)
- Failure condition events (published only on step failure)
- Always condition events (published regardless of outcome)
- Both durable (PGMQ) and fast (in-memory) delivery modes

The handlers focus on business logic only - event publishing is handled
by the worker's post-execution callback system based on YAML declarations.

Pattern (4 steps):
1. validate_order: Publishes order.validated on success (fast delivery)
2. process_payment: Publishes payment.processed/payment.failed (durable)
3. update_inventory: Publishes inventory.updated always (fast delivery)
4. send_notification: Publishes notification.sent on success (fast delivery)
"""

from __future__ import annotations

import os
import time
import uuid
from datetime import datetime, timezone
from typing import TYPE_CHECKING

from tasker_core import StepHandler, StepHandlerResult

if TYPE_CHECKING:
    from tasker_core import StepContext


def _log_info(handler_name: str, message: str) -> None:
    """Log info message in test environment."""
    if os.environ.get("TASKER_ENV") == "test":
        print(f"[{handler_name}] {message}")


class ValidateOrderHandler(StepHandler):
    """Validate order data and publish order.validated event on success.

    This handler demonstrates step-level domain event declarations with:
    - Success condition event (order.validated)
    - Fast delivery mode (in-memory)

    Event published on success:
        - order.validated (fast delivery mode)
    """

    handler_name = "domain_events_py.step_handlers.ValidateOrderHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Process the validation step."""
        start_time = time.perf_counter()

        # Extract context data
        input_data = context.input_data or {}
        order_id = input_data.get("order_id") or str(uuid.uuid4())
        customer_id = input_data.get("customer_id") or "unknown"
        amount = input_data.get("amount", 0)

        # Get configuration from step config
        step_config = context.step_config or {}
        validation_mode = step_config.get("validation_mode", "standard")

        _log_info(
            "ValidateOrderHandler",
            f"Validating order: {order_id} for customer: {customer_id}",
        )

        # Perform validation
        validation_checks = ["order_id_present", "customer_id_present"]

        if validation_mode == "strict" and amount <= 0:
            return StepHandlerResult.failure(
                message="Amount must be positive in strict mode",
                error_type="ValidationError",
                retryable=False,
                metadata={"validation_mode": validation_mode},
            )

        if amount > 0:
            validation_checks.append("amount_positive")

        execution_time_ms = int((time.perf_counter() - start_time) * 1000)

        # Return success - event publishing is handled by worker callback
        return StepHandlerResult.success(
            {
                "order_id": order_id,
                "validation_timestamp": datetime.now(timezone.utc).isoformat(),
                "validation_checks": validation_checks,
                "validated": True,
            },
            metadata={
                "execution_time_ms": execution_time_ms,
                "validation_mode": validation_mode,
            },
        )


class ProcessPaymentHandler(StepHandler):
    """Process payment and publish events for both success and failure scenarios.

    This handler demonstrates step-level domain event declarations with:
    - Success condition event (payment.processed)
    - Failure condition event (payment.failed)
    - Durable delivery mode (PGMQ)

    Events published based on outcome:
        - payment.processed (durable, on success)
        - payment.failed (durable, on failure)
    """

    handler_name = "domain_events_py.step_handlers.ProcessPaymentHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Process the payment step."""
        start_time = time.perf_counter()

        # Extract context data
        input_data = context.input_data or {}
        order_id = input_data.get("order_id", "unknown")
        amount = input_data.get("amount", 0)
        simulate_failure = input_data.get("simulate_failure", False)

        # Get configuration from step config
        step_config = context.step_config or {}
        payment_provider = step_config.get("payment_provider", "mock")

        _log_info(
            "ProcessPaymentHandler",
            f"Processing payment for order: {order_id}, amount: {amount}, provider: {payment_provider}",
        )

        # Simulate failure if requested
        if simulate_failure:
            execution_time_ms = int((time.perf_counter() - start_time) * 1000)

            return StepHandlerResult.failure(
                message="Simulated payment failure",
                error_type="PaymentError",
                retryable=True,
                metadata={
                    "execution_time_ms": execution_time_ms,
                    "order_id": order_id,
                    "failed_at": datetime.now(timezone.utc).isoformat(),
                },
            )

        # Generate transaction ID
        transaction_id = f"TXN-{uuid.uuid4()}"
        execution_time_ms = int((time.perf_counter() - start_time) * 1000)

        # Return success - event publishing is handled by worker callback
        return StepHandlerResult.success(
            {
                "transaction_id": transaction_id,
                "amount": amount,
                "payment_method": "credit_card",
                "processed_at": datetime.now(timezone.utc).isoformat(),
                "status": "success",
            },
            metadata={
                "execution_time_ms": execution_time_ms,
                "payment_provider": payment_provider,
            },
        )


class UpdateInventoryHandler(StepHandler):
    """Update inventory and publish event regardless of outcome.

    This handler demonstrates step-level domain event declarations with:
    - Always condition event (inventory.updated)
    - Fast delivery mode (in-memory)

    The "always" condition means the event is published regardless of
    whether the step succeeds or fails.

    Event published:
        - inventory.updated (fast, always condition)
    """

    handler_name = "domain_events_py.step_handlers.UpdateInventoryHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Process the inventory update step."""
        start_time = time.perf_counter()

        # Extract context data
        input_data = context.input_data or {}
        order_id = input_data.get("order_id", "unknown")

        # Get configuration from step config
        step_config = context.step_config or {}
        inventory_source = step_config.get("inventory_source", "mock")

        _log_info(
            "UpdateInventoryHandler",
            f"Updating inventory for order: {order_id}, source: {inventory_source}",
        )

        # Mock inventory items
        items = [
            {"sku": "ITEM-001", "quantity": 1},
            {"sku": "ITEM-002", "quantity": 2},
        ]

        execution_time_ms = int((time.perf_counter() - start_time) * 1000)

        # Return success - event publishing is handled by worker callback
        # Note: inventory.updated event is published with condition: always
        return StepHandlerResult.success(
            {
                "order_id": order_id,
                "items": items,
                "success": True,
                "updated_at": datetime.now(timezone.utc).isoformat(),
            },
            metadata={
                "execution_time_ms": execution_time_ms,
                "inventory_source": inventory_source,
            },
        )


class SendNotificationHandler(StepHandler):
    """Send customer notification with fast delivery event.

    This handler demonstrates step-level domain event declarations with:
    - Success condition event (notification.sent)
    - Fast delivery mode (in-memory)

    Event published on success:
        - notification.sent (fast)
    """

    handler_name = "domain_events_py.step_handlers.SendNotificationHandler"
    handler_version = "1.0.0"

    def call(self, context: StepContext) -> StepHandlerResult:
        """Process the notification step."""
        start_time = time.perf_counter()

        # Extract context data
        input_data = context.input_data or {}
        customer_id = input_data.get("customer_id", "unknown")

        # Get configuration from step config
        step_config = context.step_config or {}
        notification_type = step_config.get("notification_type", "email")

        _log_info(
            "SendNotificationHandler",
            f"Sending notification to: {customer_id}, type: {notification_type}",
        )

        # Generate notification ID
        notification_id = f"NOTIF-{uuid.uuid4()}"
        execution_time_ms = int((time.perf_counter() - start_time) * 1000)

        # Return success - event publishing is handled by worker callback
        return StepHandlerResult.success(
            {
                "notification_id": notification_id,
                "channel": notification_type,
                "recipient": customer_id,
                "sent_at": datetime.now(timezone.utc).isoformat(),
                "status": "delivered",
            },
            metadata={
                "execution_time_ms": execution_time_ms,
                "notification_type": notification_type,
            },
        )


__all__ = [
    "ValidateOrderHandler",
    "ProcessPaymentHandler",
    "UpdateInventoryHandler",
    "SendNotificationHandler",
]
