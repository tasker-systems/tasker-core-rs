/**
 * Domain Event Publishing Step Handlers for E2E Testing.
 *
 * Implements an order processing workflow that publishes domain events:
 * 1. ValidateOrderHandler: Validate order, publish order.validated (fast)
 * 2. ProcessPaymentHandler: Process payment, publish payment.processed/failed (durable)
 * 3. UpdateInventoryHandler: Update inventory, publish inventory.updated (always)
 * 4. SendNotificationHandler: Send notification, publish notification.sent (fast)
 *
 * Note: Event publishing is handled by Rust via template configuration.
 * These handlers just need to succeed; Rust PostHandlerCallback dispatches events.
 *
 * Matches Ruby and Python domain event implementations for testing parity.
 */

import { StepHandler } from '../../../../../src/handler/base.js';
import type { StepContext } from '../../../../../src/types/step-context.js';
import type { StepHandlerResult } from '../../../../../src/types/step-handler-result.js';

/**
 * Validate order details.
 * Publishes: order.validated (condition: success, delivery: fast)
 */
export class ValidateOrderHandler extends StepHandler {
  static handlerName = 'domain_events.step_handlers.ValidateOrderHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    const orderId = context.getInput<string>('order_id');
    const customerId = context.getInput<string>('customer_id');
    const amount = context.getInput<number>('amount');

    const errors: string[] = [];

    if (!orderId) {
      errors.push('order_id is required');
    }

    if (!customerId) {
      errors.push('customer_id is required');
    }

    if (amount === undefined || amount === null || amount <= 0) {
      errors.push('amount must be a positive number');
    }

    if (errors.length > 0) {
      return this.failure(errors.join('; '), 'validation_error', false);
    }

    return this.success({
      order_id: orderId,
      customer_id: customerId,
      amount,
      validated: true,
      validated_at: new Date().toISOString(),
    });
  }
}

/**
 * Process payment for the order.
 * Publishes: payment.processed (condition: success, delivery: durable)
 * Publishes: payment.failed (condition: failure, delivery: durable)
 */
export class ProcessPaymentHandler extends StepHandler {
  static handlerName = 'domain_events.step_handlers.ProcessPaymentHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // getDependencyResult() already unwraps the 'result' field, so we get the inner value directly
    const validateResult = context.getDependencyResult('domain_events_ts_validate_order') as Record<
      string,
      unknown
    > | null;

    if (!validateResult) {
      return this.failure(
        'Missing dependency result from domain_events_ts_validate_order',
        'dependency_error',
        false
      );
    }

    const orderId = validateResult.order_id as string;
    const amount = validateResult.amount as number;

    // Simulate payment processing (always succeeds in test)
    const paymentId = `PAY-${Date.now()}-${Math.random().toString(36).substring(7)}`;

    return this.success({
      order_id: orderId,
      payment_id: paymentId,
      amount,
      payment_status: 'completed',
      processed_at: new Date().toISOString(),
    });
  }
}

/**
 * Update inventory for order items.
 * Publishes: inventory.updated (condition: always, delivery: fast)
 */
export class UpdateInventoryHandler extends StepHandler {
  static handlerName = 'domain_events.step_handlers.UpdateInventoryHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // getDependencyResult() already unwraps the 'result' field, so we get the inner value directly
    const validateResult = context.getDependencyResult('domain_events_ts_validate_order') as Record<
      string,
      unknown
    > | null;
    const paymentResult = context.getDependencyResult('domain_events_ts_process_payment') as Record<
      string,
      unknown
    > | null;

    if (!validateResult) {
      return this.failure(
        'Missing dependency result from domain_events_ts_validate_order',
        'dependency_error',
        false
      );
    }

    if (!paymentResult) {
      return this.failure(
        'Missing dependency result from domain_events_ts_process_payment',
        'dependency_error',
        false
      );
    }

    const orderId = validateResult.order_id as string;

    // Simulate inventory update
    const itemsUpdated = Math.floor(Math.random() * 5) + 1;

    return this.success({
      order_id: orderId,
      items_updated: itemsUpdated,
      inventory_status: 'updated',
      updated_at: new Date().toISOString(),
    });
  }
}

/**
 * Send order confirmation notification.
 * Publishes: notification.sent (condition: success, delivery: fast)
 */
export class SendNotificationHandler extends StepHandler {
  static handlerName = 'domain_events.step_handlers.SendNotificationHandler';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // getDependencyResult() already unwraps the 'result' field, so we get the inner value directly
    const validateResult = context.getDependencyResult('domain_events_ts_validate_order') as Record<
      string,
      unknown
    > | null;
    const paymentResult = context.getDependencyResult('domain_events_ts_process_payment') as Record<
      string,
      unknown
    > | null;
    const inventoryResult = context.getDependencyResult(
      'domain_events_ts_update_inventory'
    ) as Record<string, unknown> | null;

    if (!validateResult) {
      return this.failure(
        'Missing dependency result from domain_events_ts_validate_order',
        'dependency_error',
        false
      );
    }

    if (!paymentResult) {
      return this.failure(
        'Missing dependency result from domain_events_ts_process_payment',
        'dependency_error',
        false
      );
    }

    if (!inventoryResult) {
      return this.failure(
        'Missing dependency result from domain_events_ts_update_inventory',
        'dependency_error',
        false
      );
    }

    const orderId = validateResult.order_id as string;
    const customerId = validateResult.customer_id as string;

    // Simulate sending notification
    const channels = ['email', 'sms'];

    return this.success({
      order_id: orderId,
      customer_id: customerId,
      channels,
      notification_status: 'sent',
      sent_at: new Date().toISOString(),
    });
  }
}
