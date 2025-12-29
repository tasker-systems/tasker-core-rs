/**
 * TAS-65/TAS-122: Example logging subscribers for fast/in-process domain events.
 *
 * Demonstrates how to create subscribers that log events matching patterns.
 * Useful for debugging and audit trails.
 *
 * @example Register in bootstrap
 * ```typescript
 * const registry = SubscriberRegistry.getInstance();
 * registry.register(new LoggingSubscriber());
 * await registry.startAll();
 * ```
 *
 * @example Output
 * ```
 * INFO [LoggingSubscriber] Event: payment.processed
 *   Task: 550e8400-e29b-41d4-a716-446655440000
 *   Step: process_payment
 *   Namespace: payments
 *   Correlation: 7c9e6679-7425-40de-944b-e07fc1f90ae7
 * ```
 *
 * @module
 */

import { BaseSubscriber, type DomainEvent } from '../../../../../src/handler/domain-events.js';

/**
 * Basic logging subscriber that logs all events at INFO level.
 *
 * Subscribes to all events using the '*' pattern.
 */
export class LoggingSubscriber extends BaseSubscriber {
  readonly subscriberName = 'LoggingSubscriber';

  /**
   * Patterns to subscribe to. '*' matches all events.
   */
  protected patterns = ['*'];

  /**
   * Handle any domain event by logging its details.
   */
  async handle(event: DomainEvent): Promise<void> {
    const { eventName, metadata } = event;

    this.logger.info(`[LoggingSubscriber] Event: ${eventName}`);
    this.logger.info(`  Task: ${metadata.taskUuid}`);
    this.logger.info(`  Step: ${metadata.stepName}`);
    this.logger.info(`  Namespace: ${metadata.namespace}`);
    this.logger.info(`  Correlation: ${metadata.correlationId}`);
  }
}

/**
 * Debug-level logging subscriber.
 *
 * Same as LoggingSubscriber but logs at DEBUG level.
 * Useful for verbose environments where INFO is too noisy.
 */
export class DebugLoggingSubscriber extends BaseSubscriber {
  readonly subscriberName = 'DebugLoggingSubscriber';

  protected patterns = ['*'];

  async handle(event: DomainEvent): Promise<void> {
    const { eventName, metadata } = event;

    this.logger.debug(`[DebugLoggingSubscriber] Event: ${eventName}`);
    this.logger.debug(`  Task: ${metadata.taskUuid}`);
    this.logger.debug(`  Step: ${metadata.stepName}`);
  }
}

/**
 * Maximum characters to log from payload.
 */
const MAX_PAYLOAD_LENGTH = 500;

/**
 * Verbose logging subscriber that includes payload.
 *
 * Logs full event details including business payload.
 * Use with caution in production as payloads may contain sensitive data.
 */
export class VerboseLoggingSubscriber extends BaseSubscriber {
  readonly subscriberName = 'VerboseLoggingSubscriber';

  protected patterns = ['*'];

  async handle(event: DomainEvent): Promise<void> {
    const { eventName, metadata, businessPayload } = event;

    // Truncate payload for logging
    const payloadStr = JSON.stringify(businessPayload ?? {});
    const payloadPreview =
      payloadStr.length > MAX_PAYLOAD_LENGTH
        ? `${payloadStr.slice(0, MAX_PAYLOAD_LENGTH)}...(truncated)`
        : payloadStr;

    this.logger.info(`[VerboseLoggingSubscriber] Event: ${eventName}`);
    this.logger.info(`  Task: ${metadata.taskUuid}`);
    this.logger.info(`  Step: ${metadata.stepName}`);
    this.logger.info(`  Namespace: ${metadata.namespace}`);
    this.logger.info(`  Correlation: ${metadata.correlationId}`);
    this.logger.info(`  Payload: ${payloadPreview}`);
  }
}

/**
 * Payment-specific logging subscriber.
 *
 * Only subscribes to payment.* events and logs payment-specific details.
 */
export class PaymentLoggingSubscriber extends BaseSubscriber {
  readonly subscriberName = 'PaymentLoggingSubscriber';

  protected patterns = ['payment.*'];

  async handle(event: DomainEvent): Promise<void> {
    const { eventName, metadata, businessPayload } = event;
    const payload = businessPayload ?? {};

    this.logger.info(`[PaymentLoggingSubscriber] ${eventName}`);
    this.logger.info(`  Task: ${metadata.taskUuid}`);

    if (eventName.includes('processed')) {
      this.logger.info(`  Transaction: ${payload.transaction_id}`);
      this.logger.info(`  Amount: ${payload.amount} ${payload.currency ?? 'USD'}`);
    } else if (eventName.includes('failed')) {
      this.logger.info(`  Error: ${payload.error_code} - ${payload.error_message}`);
      this.logger.info(`  Retryable: ${payload.is_retryable}`);
    }
  }
}

/**
 * Metrics subscriber that tracks event counts.
 *
 * Demonstrates how to build observability tooling with domain events.
 */
export class MetricsSubscriber extends BaseSubscriber {
  readonly subscriberName = 'MetricsSubscriber';

  protected patterns = ['*'];

  /** Event counts by name */
  private eventCounts = new Map<string, number>();

  /** Event counts by namespace */
  private namespaceCounts = new Map<string, number>();

  async handle(event: DomainEvent): Promise<void> {
    const { eventName, metadata } = event;

    // Increment event count
    const currentCount = this.eventCounts.get(eventName) ?? 0;
    this.eventCounts.set(eventName, currentCount + 1);

    // Increment namespace count
    const namespace = metadata.namespace ?? 'unknown';
    const nsCount = this.namespaceCounts.get(namespace) ?? 0;
    this.namespaceCounts.set(namespace, nsCount + 1);
  }

  /**
   * Get metrics summary.
   */
  getMetrics(): {
    eventCounts: Record<string, number>;
    namespaceCounts: Record<string, number>;
    totalEvents: number;
  } {
    const eventCounts = Object.fromEntries(this.eventCounts);
    const namespaceCounts = Object.fromEntries(this.namespaceCounts);
    const totalEvents = Array.from(this.eventCounts.values()).reduce(
      (sum, count) => sum + count,
      0
    );

    return { eventCounts, namespaceCounts, totalEvents };
  }

  /**
   * Reset all metrics.
   */
  resetMetrics(): void {
    this.eventCounts.clear();
    this.namespaceCounts.clear();
  }
}
