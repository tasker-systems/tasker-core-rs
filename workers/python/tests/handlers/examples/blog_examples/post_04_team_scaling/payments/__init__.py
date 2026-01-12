"""
Payments namespace handlers for Blog Post 04.

This namespace handles direct payment gateway refunds.
Can be called directly or via cross-namespace coordination from customer_success.

Workflow: process_refund (payments namespace)
Steps:
1. validate_payment_eligibility - Check if payment can be refunded
2. process_gateway_refund - Execute refund through payment processor
3. update_payment_records - Update internal payment status
4. notify_customer - Send refund confirmation
"""

from .notify_customer_handler import NotifyCustomerHandler
from .process_gateway_refund_handler import ProcessGatewayRefundHandler
from .update_payment_records_handler import UpdatePaymentRecordsHandler
from .validate_payment_eligibility_handler import ValidatePaymentEligibilityHandler

__all__ = [
    "ValidatePaymentEligibilityHandler",
    "ProcessGatewayRefundHandler",
    "UpdatePaymentRecordsHandler",
    "NotifyCustomerHandler",
]
