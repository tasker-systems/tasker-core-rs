/**
 * TAS-65/TAS-122: Custom publisher for payment-related domain events.
 *
 * Demonstrates durable delivery mode with custom payload enrichment.
 * This publisher covers the "durable + custom publisher" case (2b) in the
 * 2x2 test matrix.
 *
 * ## 2x2 Test Matrix Coverage
 *
 * - Delivery mode: durable (PGMQ)
 * - Publisher: custom (PaymentEventPublisher)
 *
 * ## Features
 *
 * - Conditional event publishing based on result status
 * - Payload enrichment with business logic
 * - Multiple events from a single step (success/failure)
 * - Analytics event for all outcomes
 *
 * @example YAML declaration
 * ```yaml
 * steps:
 *   - name: process_payment
 *     publishes_events:
 *       - name: payment.processed
 *         condition: success
 *         delivery_mode: durable
 *         publisher: DomainEvents::Publishers::PaymentEventPublisher
 *       - name: payment.failed
 *         condition: failure
 *         delivery_mode: durable
 *         publisher: DomainEvents::Publishers::PaymentEventPublisher
 * ```
 *
 * @module
 */

import {
  BasePublisher,
  type StepEventContext,
  type EventDeclaration,
} from '../../../../../src/handler/domain-events.js';
import type { StepHandlerResult } from '../../../../../src/types/step-handler-result.js';

/**
 * Error codes and their corresponding retry delays in seconds.
 */
const ERROR_RETRY_DELAYS: Record<string, number> = {
  RATE_LIMITED: 600, // 10 minutes
  TEMPORARY_FAILURE: 300, // 5 minutes
  NETWORK_ERROR: 60, // 1 minute
  CARD_DECLINED: 0, // No retry (user action needed)
};

const DEFAULT_RETRY_DELAY = 120; // 2 minutes default

/**
 * Custom publisher for payment-related domain events.
 *
 * Enriches payment events with business context and provides conditional
 * publishing based on transaction data availability.
 */
export class PaymentEventPublisher extends BasePublisher {
  readonly publisherName = 'DomainEvents::Publishers::PaymentEventPublisher';

  /**
   * Transform step result into payment event payload.
   *
   * Handles both success and failure cases with appropriate payloads.
   */
  protected transformPayload(
    stepResult: StepHandlerResult,
    eventDeclaration: EventDeclaration,
    stepContext: StepEventContext | null
  ): Record<string, unknown> {
    const result = stepResult.result ?? {};
    const eventName = eventDeclaration.name;

    if (stepResult.success && eventName?.includes('processed')) {
      return this.buildSuccessPayload(result, stepResult, stepContext);
    }

    if (!stepResult.success && eventName?.includes('failed')) {
      return this.buildFailurePayload(result, stepResult);
    }

    // Fallback for analytics or other events
    return result;
  }

  /**
   * Determine if this event should be published.
   *
   * Only publishes for payment-related steps with valid transaction data.
   */
  protected shouldPublish(
    stepResult: StepHandlerResult,
    eventDeclaration: EventDeclaration,
    _stepContext: StepEventContext | null
  ): boolean {
    const result = stepResult.result ?? {};
    const eventName = eventDeclaration.name;

    // For success events, verify we have transaction data
    if (eventName?.includes('processed')) {
      const hasTransactionId = Boolean(result.transaction_id);
      return stepResult.success && hasTransactionId;
    }

    // For failure events, verify we have error info
    if (eventName?.includes('failed')) {
      const metadata = stepResult.metadata ?? {};
      const hasErrorCode = Boolean(metadata.error_code);
      return !stepResult.success && hasErrorCode;
    }

    // Default: always publish
    return true;
  }

  /**
   * Add execution metrics to event metadata.
   */
  protected additionalMetadata(
    stepResult: StepHandlerResult,
    _eventDeclaration: EventDeclaration,
    _stepContext: StepEventContext | null
  ): Record<string, unknown> {
    const metadata = stepResult.metadata ?? {};
    return {
      execution_time_ms: metadata.execution_time_ms,
      publisher_type: 'custom',
      publisher_name: this.publisherName,
      payment_provider: metadata.payment_provider,
    };
  }

  /**
   * Hook called before publishing.
   */
  protected beforePublish(
    eventName: string,
    _payload: Record<string, unknown>,
    _metadata: Record<string, unknown>
  ): void {
    this.logger.debug(
      `[${this.publisherName}] Publishing ${eventName} via durable delivery`
    );
  }

  /**
   * Hook called after successful publishing.
   */
  protected afterPublish(
    eventName: string,
    _payload: Record<string, unknown>,
    _metadata: Record<string, unknown>
  ): void {
    this.logger.info(
      `[${this.publisherName}] Published ${eventName} (durable + custom publisher)`
    );
  }

  /**
   * Build payload for successful payment.
   */
  private buildSuccessPayload(
    result: Record<string, unknown>,
    _stepResult: StepHandlerResult,
    stepContext: StepEventContext | null
  ): Record<string, unknown> {
    return {
      transaction_id: result.transaction_id,
      amount: result.amount,
      currency: result.currency ?? 'USD',
      payment_method: result.payment_method ?? 'credit_card',
      processed_at: result.processed_at ?? new Date().toISOString(),
      customer_tier: this.determineCustomerTier(stepContext),
      delivery_mode: 'durable',
      publisher: this.publisherName,
    };
  }

  /**
   * Build payload for failed payment.
   */
  private buildFailurePayload(
    result: Record<string, unknown>,
    stepResult: StepHandlerResult
  ): Record<string, unknown> {
    const metadata = stepResult.metadata ?? {};
    const errorCode = String(metadata.error_code ?? 'UNKNOWN');

    return {
      error_code: errorCode,
      error_type: metadata.error_type ?? 'PaymentError',
      error_message: metadata.error_message ?? 'Payment failed',
      order_id: result.order_id,
      failed_at: new Date().toISOString(),
      retry_after_seconds: this.calculateRetryDelay(errorCode),
      is_retryable: metadata.retryable ?? false,
      delivery_mode: 'durable',
      publisher: this.publisherName,
    };
  }

  /**
   * Determine customer tier from task context.
   */
  private determineCustomerTier(
    stepContext: StepEventContext | null
  ): string {
    if (!stepContext) {
      return 'standard';
    }

    const task = stepContext.task;
    const context = task.context ?? {};
    return String(context.customer_tier ?? 'standard');
  }

  /**
   * Calculate retry delay based on error code.
   */
  private calculateRetryDelay(errorCode: string): number {
    return ERROR_RETRY_DELAYS[errorCode] ?? DEFAULT_RETRY_DELAY;
  }
}
