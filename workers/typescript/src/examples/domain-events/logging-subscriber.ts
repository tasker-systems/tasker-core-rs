/**
 * Logging Subscriber Example
 *
 * Demonstrates how to create a domain event subscriber that:
 * - Subscribes to all events for comprehensive audit logging
 * - Uses lifecycle hooks for timing and error tracking
 * - Structures log output for downstream log aggregation (ELK, Datadog)
 *
 * @module examples/domain-events/logging-subscriber
 * @see TAS-112
 */

import { BaseSubscriber, type DomainEvent } from '../../handler/domain-events.js';

/**
 * Audit logging subscriber for all domain events.
 *
 * Provides structured logging of every domain event for:
 * - Compliance and audit trails
 * - Debugging workflow execution
 * - Integration with log aggregation systems
 *
 * @example
 * ```typescript
 * // Register at bootstrap
 * import { SubscriberRegistry, InProcessDomainEventPoller } from 'tasker-core/handler';
 * import { AuditLoggingSubscriber } from './examples/domain-events';
 *
 * const poller = new InProcessDomainEventPoller();
 * SubscriberRegistry.instance().register(AuditLoggingSubscriber);
 * SubscriberRegistry.instance().startAll(poller);
 * poller.start();
 * ```
 */
export class AuditLoggingSubscriber extends BaseSubscriber {
  private eventCount = 0;
  private errorCount = 0;
  private processingTimes: number[] = [];
  private startTime: number = 0;

  /**
   * Subscribe to ALL events for comprehensive audit logging.
   */
  static subscribesTo(): string[] {
    return ['*'];
  }

  /**
   * Log the event with structured fields for aggregation.
   */
  handle(event: DomainEvent): void {
    this.eventCount++;

    // Structure log entry for easy parsing by log aggregators
    const logEntry = {
      // Event identification
      eventId: event.eventId,
      eventName: event.eventName,

      // Task context for correlation
      taskUuid: event.metadata.taskUuid,
      stepUuid: event.metadata.stepUuid,
      stepName: event.metadata.stepName,
      namespace: event.metadata.namespace,
      correlationId: event.metadata.correlationId,

      // Timing
      firedAt: event.metadata.firedAt,
      receivedAt: new Date().toISOString(),

      // Event source
      publisher: event.metadata.publisher,

      // Execution result summary
      executionSuccess: event.executionResult?.success ?? null,

      // Payload summary (avoid logging sensitive data)
      payloadKeys: Object.keys(event.payload),
      payloadSize: JSON.stringify(event.payload).length,

      // Sequence tracking
      sequenceNumber: this.eventCount,
    };

    this.logger.info(logEntry, `[AUDIT] Domain event received: ${event.eventName}`);
  }

  /**
   * Record start time for latency tracking.
   */
  beforeHandle(_event: DomainEvent): boolean {
    this.startTime = Date.now();
    return true;
  }

  /**
   * Record processing time after successful handling.
   */
  afterHandle(event: DomainEvent): void {
    const processingTime = Date.now() - this.startTime;
    this.processingTimes.push(processingTime);

    // Keep only last 1000 processing times for rolling average
    if (this.processingTimes.length > 1000) {
      this.processingTimes.shift();
    }

    this.logger.debug(
      {
        eventName: event.eventName,
        processingTimeMs: processingTime,
        avgProcessingTimeMs: this.averageProcessingTime,
      },
      'Audit log processing complete'
    );
  }

  /**
   * Track and log errors with context for debugging.
   */
  onHandleError(event: DomainEvent, error: Error): void {
    this.errorCount++;

    this.logger.error(
      {
        eventId: event.eventId,
        eventName: event.eventName,
        taskUuid: event.metadata.taskUuid,
        correlationId: event.metadata.correlationId,
        error: error.message,
        errorStack: error.stack,
        totalErrors: this.errorCount,
      },
      '[AUDIT] Failed to log domain event'
    );
  }

  /**
   * Get average processing time in milliseconds.
   */
  get averageProcessingTime(): number {
    if (this.processingTimes.length === 0) return 0;
    const sum = this.processingTimes.reduce((a, b) => a + b, 0);
    return Math.round(sum / this.processingTimes.length);
  }

  /**
   * Get subscriber statistics for monitoring.
   */
  get stats(): {
    eventsLogged: number;
    errors: number;
    avgProcessingTimeMs: number;
  } {
    return {
      eventsLogged: this.eventCount,
      errors: this.errorCount,
      avgProcessingTimeMs: this.averageProcessingTime,
    };
  }
}

/**
 * Payment-specific logging subscriber.
 *
 * Demonstrates pattern-based subscription for domain-specific logging
 * with enhanced detail for payment events.
 */
export class PaymentLoggingSubscriber extends BaseSubscriber {
  private paymentEvents: Array<{
    eventId: string;
    eventName: string;
    transactionId: unknown;
    amount: unknown;
    status: unknown;
    timestamp: string;
  }> = [];

  /**
   * Subscribe only to payment events.
   */
  static subscribesTo(): string[] {
    return ['payment.*'];
  }

  /**
   * Log payment events with payment-specific fields.
   */
  handle(event: DomainEvent): void {
    const paymentLog = {
      eventId: event.eventId,
      eventName: event.eventName,
      transactionId: event.payload.transactionId ?? event.payload.transaction_id,
      amount: event.payload.amount,
      currency: event.payload.currency,
      status: event.payload.status,
      customerId: event.payload.customerId ?? event.payload.customer_id,
      correlationId: event.metadata.correlationId,
      timestamp: new Date().toISOString(),
    };

    // Store for potential replay/debugging
    this.paymentEvents.push({
      eventId: event.eventId,
      eventName: event.eventName,
      transactionId: paymentLog.transactionId,
      amount: paymentLog.amount,
      status: paymentLog.status,
      timestamp: paymentLog.timestamp,
    });

    // Keep only last 100 events in memory
    if (this.paymentEvents.length > 100) {
      this.paymentEvents.shift();
    }

    this.logger.info(paymentLog, `[PAYMENT LOG] ${event.eventName}: ${paymentLog.transactionId}`);
  }

  /**
   * Filter out test transactions from logging.
   */
  beforeHandle(event: DomainEvent): boolean {
    // Skip test/sandbox transactions
    const isTest =
      event.payload.test === true ||
      event.payload.sandbox === true ||
      String(event.payload.transactionId ?? '').startsWith('test_');

    if (isTest) {
      this.logger.debug({ eventName: event.eventName }, 'Skipping test transaction log');
      return false;
    }

    return true;
  }

  /**
   * Get recent payment events (useful for debugging).
   */
  get recentEvents(): typeof this.paymentEvents {
    return [...this.paymentEvents];
  }
}
