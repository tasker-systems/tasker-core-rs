/**
 * Payment Event Publisher Example
 *
 * Demonstrates how to create a custom domain event publisher that:
 * - Transforms step results into business-focused payment events
 * - Adds custom metadata (payment processor, merchant info)
 * - Conditionally publishes based on payment validation
 * - Uses lifecycle hooks for logging and metrics
 *
 * @module examples/domain-events/payment-publisher
 * @see TAS-112
 */

import {
  BasePublisher,
  type EventDeclaration,
  type StepEventContext,
  type StepResult,
} from '../../handler/domain-events.js';

/**
 * Custom publisher for payment domain events.
 *
 * Transforms raw step results into a standardized payment event format
 * suitable for downstream consumers (accounting, notifications, analytics).
 *
 * @example
 * ```yaml
 * # In task template YAML
 * events:
 *   - name: payment.processed
 *     condition: success
 *     delivery_mode: broadcast
 *     publisher: PaymentEventPublisher
 * ```
 *
 * @example
 * ```typescript
 * // Register at bootstrap
 * import { PublisherRegistry } from 'tasker-core/handler';
 * import { PaymentEventPublisher } from './examples/domain-events';
 *
 * PublisherRegistry.instance().register(new PaymentEventPublisher());
 * ```
 */
export class PaymentEventPublisher extends BasePublisher {
  readonly publisherName = 'PaymentEventPublisher';

  private publishCount = 0;
  private skipCount = 0;

  /**
   * Transform step result into a payment event payload.
   *
   * Extracts payment-specific fields and normalizes them into
   * a standardized format for downstream consumers.
   */
  transformPayload(
    stepResult: StepResult,
    _eventDeclaration?: EventDeclaration,
    _stepContext?: StepEventContext
  ): Record<string, unknown> {
    const result = stepResult.result ?? {};

    return {
      // Transaction identity
      transactionId: result.transaction_id ?? result.transactionId,
      referenceId: result.reference_id ?? result.referenceId,

      // Payment details
      amount: result.amount,
      currency: result.currency ?? 'USD',
      paymentMethod: result.payment_method ?? result.paymentMethod ?? 'unknown',

      // Status
      status: stepResult.success ? 'succeeded' : 'failed',
      errorCode: stepResult.success ? null : (result.error_code ?? result.errorCode),
      errorMessage: stepResult.success ? null : (result.error_message ?? result.errorMessage),

      // Timing
      processedAt: new Date().toISOString(),

      // Optional customer info (if present)
      customerId: result.customer_id ?? result.customerId,
      customerEmail: result.customer_email ?? result.customerEmail,
    };
  }

  /**
   * Only publish if we have a valid transaction ID.
   *
   * Prevents publishing incomplete or invalid payment events.
   */
  shouldPublish(
    stepResult: StepResult,
    _eventDeclaration?: EventDeclaration,
    _stepContext?: StepEventContext
  ): boolean {
    const result = stepResult.result ?? {};
    const hasTransactionId = result.transaction_id != null || result.transactionId != null;

    if (!hasTransactionId) {
      this.logger.debug('Skipping payment event: no transaction ID');
      return false;
    }

    return true;
  }

  /**
   * Add payment processor metadata.
   */
  additionalMetadata(
    stepResult: StepResult,
    _eventDeclaration?: EventDeclaration,
    stepContext?: StepEventContext
  ): Record<string, unknown> {
    const result = stepResult.result ?? {};

    return {
      // Processor information
      processor: result.processor ?? 'stripe',
      processorTransactionId: result.processor_transaction_id ?? result.processorTransactionId,

      // Merchant context
      merchantId: result.merchant_id ?? result.merchantId,

      // Workflow context
      workflowNamespace: stepContext?.namespace ?? 'payments',
    };
  }

  /**
   * Log and track before publishing.
   */
  beforePublish(
    eventName: string,
    payload: Record<string, unknown>,
    _metadata: Record<string, unknown>
  ): boolean {
    this.logger.info(
      {
        eventName,
        transactionId: payload.transactionId,
        amount: payload.amount,
        currency: payload.currency,
        status: payload.status,
      },
      'Publishing payment event'
    );
    return true;
  }

  /**
   * Update metrics after successful publish.
   */
  afterPublish(
    eventName: string,
    payload: Record<string, unknown>,
    _metadata: Record<string, unknown>
  ): void {
    this.publishCount++;
    this.logger.debug(
      {
        eventName,
        transactionId: payload.transactionId,
        totalPublished: this.publishCount,
      },
      'Payment event published successfully'
    );
  }

  /**
   * Handle publish errors with alerting context.
   */
  onPublishError(eventName: string, error: Error, payload: Record<string, unknown>): void {
    this.logger.error(
      {
        eventName,
        error: error.message,
        transactionId: payload.transactionId,
        amount: payload.amount,
      },
      'Failed to publish payment event - potential data loss'
    );
  }

  /**
   * Get publishing statistics (useful for monitoring).
   */
  get stats(): { published: number; skipped: number } {
    return {
      published: this.publishCount,
      skipped: this.skipCount,
    };
  }
}

/**
 * Refund-specific publisher that transforms refund step results.
 *
 * Demonstrates how to create specialized publishers for different
 * event types within the same domain.
 */
export class RefundEventPublisher extends BasePublisher {
  readonly publisherName = 'RefundEventPublisher';

  transformPayload(
    stepResult: StepResult,
    _eventDeclaration?: EventDeclaration,
    _stepContext?: StepEventContext
  ): Record<string, unknown> {
    const result = stepResult.result ?? {};

    return {
      refundId: result.refund_id ?? result.refundId,
      originalTransactionId: result.original_transaction_id ?? result.originalTransactionId,
      amount: result.amount,
      currency: result.currency ?? 'USD',
      reason: result.reason ?? 'customer_request',
      status: stepResult.success ? 'completed' : 'failed',
      processedAt: new Date().toISOString(),
    };
  }

  shouldPublish(stepResult: StepResult): boolean {
    const result = stepResult.result ?? {};
    // Only publish if we have a refund ID
    return result.refund_id != null || result.refundId != null;
  }

  additionalMetadata(stepResult: StepResult): Record<string, unknown> {
    const result = stepResult.result ?? {};
    return {
      refundType: result.refund_type ?? result.refundType ?? 'full',
      initiatedBy: result.initiated_by ?? result.initiatedBy ?? 'system',
    };
  }
}
