/**
 * Metrics Subscriber Example
 *
 * Demonstrates how to create a domain event subscriber for metrics collection:
 * - Counts events by type, namespace, and outcome
 * - Tracks timing distributions (latency percentiles)
 * - Provides exportable metrics for Prometheus/StatsD/Datadog
 *
 * @module examples/domain-events/metrics-subscriber
 * @see TAS-112
 */

import { BaseSubscriber, type DomainEvent } from '../../handler/domain-events.js';

/**
 * Simple counter storage for event metrics.
 */
interface CounterMap {
  [key: string]: number;
}

/**
 * Histogram bucket for timing distribution.
 */
interface HistogramBucket {
  le: number; // Less than or equal to (milliseconds)
  count: number;
}

/**
 * Metrics subscriber that collects event statistics.
 *
 * Provides real-time metrics for:
 * - Event counts by name, namespace, and outcome
 * - Event processing latency distribution
 * - Error rates and patterns
 *
 * @example
 * ```typescript
 * // Register and use for monitoring
 * import { SubscriberRegistry, InProcessDomainEventPoller } from 'tasker-core/handler';
 * import { MetricsSubscriber } from './examples/domain-events';
 *
 * const poller = new InProcessDomainEventPoller();
 * SubscriberRegistry.instance().register(MetricsSubscriber);
 * SubscriberRegistry.instance().startAll(poller);
 *
 * // Export metrics periodically
 * const metrics = MetricsSubscriber.exportMetrics();
 * sendToPrometheus(metrics);
 * ```
 */
export class MetricsSubscriber extends BaseSubscriber {
  // Event counters
  private static eventsByName: CounterMap = {};
  private static eventsByNamespace: CounterMap = {};
  private static eventsByOutcome: CounterMap = { success: 0, failure: 0, unknown: 0 };
  private static errorCount = 0;

  // Timing histogram (Prometheus-style buckets)
  private static latencyBuckets: HistogramBucket[] = [
    { le: 1, count: 0 }, // <= 1ms
    { le: 5, count: 0 }, // <= 5ms
    { le: 10, count: 0 }, // <= 10ms
    { le: 25, count: 0 }, // <= 25ms
    { le: 50, count: 0 }, // <= 50ms
    { le: 100, count: 0 }, // <= 100ms
    { le: 250, count: 0 }, // <= 250ms
    { le: 500, count: 0 }, // <= 500ms
    { le: 1000, count: 0 }, // <= 1s
    { le: Infinity, count: 0 }, // +Inf
  ];

  private static totalLatencySum = 0;
  private static totalLatencyCount = 0;

  // Instance state for timing
  private eventStartTime = 0;

  /**
   * Subscribe to all events for comprehensive metrics.
   */
  static subscribesTo(): string[] {
    return ['*'];
  }

  /**
   * Record metrics for each event.
   */
  handle(event: DomainEvent): void {
    // Count by event name
    const eventName = event.eventName;
    MetricsSubscriber.eventsByName[eventName] =
      (MetricsSubscriber.eventsByName[eventName] ?? 0) + 1;

    // Count by namespace
    const namespace = event.metadata.namespace ?? 'unknown';
    MetricsSubscriber.eventsByNamespace[namespace] =
      (MetricsSubscriber.eventsByNamespace[namespace] ?? 0) + 1;

    // Count by outcome (success/failure based on execution result)
    const outcome = this.determineOutcome(event);
    MetricsSubscriber.eventsByOutcome[outcome] =
      (MetricsSubscriber.eventsByOutcome[outcome] ?? 0) + 1;

    this.logger.debug(
      {
        eventName,
        namespace,
        outcome,
        totalByName: MetricsSubscriber.eventsByName[eventName],
      },
      'Metrics recorded'
    );
  }

  /**
   * Start timing before handling.
   */
  beforeHandle(_event: DomainEvent): boolean {
    this.eventStartTime = performance.now();
    return true;
  }

  /**
   * Record latency after handling.
   */
  afterHandle(_event: DomainEvent): void {
    const latencyMs = performance.now() - this.eventStartTime;
    this.recordLatency(latencyMs);
  }

  /**
   * Track error metrics.
   */
  onHandleError(event: DomainEvent, error: Error): void {
    MetricsSubscriber.errorCount++;
    this.logger.warn(
      {
        eventName: event.eventName,
        error: error.message,
        totalErrors: MetricsSubscriber.errorCount,
      },
      'Metrics subscriber error'
    );
  }

  /**
   * Determine event outcome from execution result.
   */
  private determineOutcome(event: DomainEvent): 'success' | 'failure' | 'unknown' {
    if (event.executionResult == null) {
      return 'unknown';
    }
    return event.executionResult.success ? 'success' : 'failure';
  }

  /**
   * Record latency in histogram buckets.
   */
  private recordLatency(latencyMs: number): void {
    MetricsSubscriber.totalLatencySum += latencyMs;
    MetricsSubscriber.totalLatencyCount++;

    // Update all applicable buckets
    for (const bucket of MetricsSubscriber.latencyBuckets) {
      if (latencyMs <= bucket.le) {
        bucket.count++;
      }
    }
  }

  /**
   * Export all metrics in Prometheus-compatible format.
   */
  static exportMetrics(): string {
    const lines: string[] = [];

    // Event counts by name
    lines.push('# HELP domain_events_total Total domain events by name');
    lines.push('# TYPE domain_events_total counter');
    for (const [name, count] of Object.entries(MetricsSubscriber.eventsByName)) {
      const safeName = name.replace(/\./g, '_');
      lines.push(`domain_events_total{event_name="${safeName}"} ${count}`);
    }

    // Event counts by namespace
    lines.push('');
    lines.push('# HELP domain_events_by_namespace_total Events by namespace');
    lines.push('# TYPE domain_events_by_namespace_total counter');
    for (const [ns, count] of Object.entries(MetricsSubscriber.eventsByNamespace)) {
      lines.push(`domain_events_by_namespace_total{namespace="${ns}"} ${count}`);
    }

    // Event counts by outcome
    lines.push('');
    lines.push('# HELP domain_events_outcome_total Events by outcome');
    lines.push('# TYPE domain_events_outcome_total counter');
    for (const [outcome, count] of Object.entries(MetricsSubscriber.eventsByOutcome)) {
      lines.push(`domain_events_outcome_total{outcome="${outcome}"} ${count}`);
    }

    // Error count
    lines.push('');
    lines.push('# HELP domain_events_errors_total Total processing errors');
    lines.push('# TYPE domain_events_errors_total counter');
    lines.push(`domain_events_errors_total ${MetricsSubscriber.errorCount}`);

    // Latency histogram
    lines.push('');
    lines.push('# HELP domain_events_latency_seconds Event processing latency');
    lines.push('# TYPE domain_events_latency_seconds histogram');
    for (const bucket of MetricsSubscriber.latencyBuckets) {
      const le = bucket.le === Infinity ? '+Inf' : (bucket.le / 1000).toFixed(3);
      lines.push(`domain_events_latency_seconds_bucket{le="${le}"} ${bucket.count}`);
    }
    lines.push(`domain_events_latency_seconds_sum ${MetricsSubscriber.totalLatencySum / 1000}`);
    lines.push(`domain_events_latency_seconds_count ${MetricsSubscriber.totalLatencyCount}`);

    return lines.join('\n');
  }

  /**
   * Get summary metrics object (for JSON export).
   */
  static getSummary(): {
    eventsByName: CounterMap;
    eventsByNamespace: CounterMap;
    eventsByOutcome: CounterMap;
    errorCount: number;
    latency: {
      avgMs: number;
      count: number;
      p50BucketMs: number;
      p99BucketMs: number;
    };
  } {
    // Find approximate percentile buckets
    const findPercentileBucket = (percentile: number): number => {
      const target = Math.floor(MetricsSubscriber.totalLatencyCount * percentile);
      for (const bucket of MetricsSubscriber.latencyBuckets) {
        if (bucket.count >= target) {
          return bucket.le === Infinity ? 1000 : bucket.le;
        }
      }
      return 1000;
    };

    return {
      eventsByName: { ...MetricsSubscriber.eventsByName },
      eventsByNamespace: { ...MetricsSubscriber.eventsByNamespace },
      eventsByOutcome: { ...MetricsSubscriber.eventsByOutcome },
      errorCount: MetricsSubscriber.errorCount,
      latency: {
        avgMs:
          MetricsSubscriber.totalLatencyCount > 0
            ? Math.round(MetricsSubscriber.totalLatencySum / MetricsSubscriber.totalLatencyCount)
            : 0,
        count: MetricsSubscriber.totalLatencyCount,
        p50BucketMs: findPercentileBucket(0.5),
        p99BucketMs: findPercentileBucket(0.99),
      },
    };
  }

  /**
   * Reset all metrics (useful for testing).
   */
  static reset(): void {
    MetricsSubscriber.eventsByName = {};
    MetricsSubscriber.eventsByNamespace = {};
    MetricsSubscriber.eventsByOutcome = { success: 0, failure: 0, unknown: 0 };
    MetricsSubscriber.errorCount = 0;
    MetricsSubscriber.latencyBuckets = [
      { le: 1, count: 0 },
      { le: 5, count: 0 },
      { le: 10, count: 0 },
      { le: 25, count: 0 },
      { le: 50, count: 0 },
      { le: 100, count: 0 },
      { le: 250, count: 0 },
      { le: 500, count: 0 },
      { le: 1000, count: 0 },
      { le: Infinity, count: 0 },
    ];
    MetricsSubscriber.totalLatencySum = 0;
    MetricsSubscriber.totalLatencyCount = 0;
  }
}

/**
 * Payment-specific metrics subscriber.
 *
 * Tracks payment-specific metrics like transaction amounts,
 * success rates by payment method, etc.
 */
export class PaymentMetricsSubscriber extends BaseSubscriber {
  private static totalAmount = 0;
  private static transactionCount = 0;
  private static successCount = 0;
  private static failureCount = 0;
  private static amountByMethod: CounterMap = {};
  private static countByMethod: CounterMap = {};

  static subscribesTo(): string[] {
    return ['payment.*'];
  }

  handle(event: DomainEvent): void {
    PaymentMetricsSubscriber.transactionCount++;

    // Track success/failure
    const status = event.payload.status;
    if (status === 'succeeded') {
      PaymentMetricsSubscriber.successCount++;
    } else if (status === 'failed') {
      PaymentMetricsSubscriber.failureCount++;
    }

    // Track amount
    const amount = Number(event.payload.amount) || 0;
    PaymentMetricsSubscriber.totalAmount += amount;

    // Track by payment method
    const method = String(event.payload.paymentMethod ?? event.payload.payment_method ?? 'unknown');
    PaymentMetricsSubscriber.amountByMethod[method] =
      (PaymentMetricsSubscriber.amountByMethod[method] ?? 0) + amount;
    PaymentMetricsSubscriber.countByMethod[method] =
      (PaymentMetricsSubscriber.countByMethod[method] ?? 0) + 1;

    this.logger.debug(
      {
        transactionCount: PaymentMetricsSubscriber.transactionCount,
        totalAmount: PaymentMetricsSubscriber.totalAmount,
        successRate: PaymentMetricsSubscriber.successRate,
      },
      'Payment metrics updated'
    );
  }

  /**
   * Calculate success rate as percentage.
   */
  static get successRate(): number {
    if (PaymentMetricsSubscriber.transactionCount === 0) return 0;
    return (
      Math.round(
        (PaymentMetricsSubscriber.successCount / PaymentMetricsSubscriber.transactionCount) *
          100 *
          100
      ) / 100
    );
  }

  /**
   * Get payment metrics summary.
   */
  static getSummary(): {
    totalAmount: number;
    transactionCount: number;
    successCount: number;
    failureCount: number;
    successRate: number;
    avgTransactionAmount: number;
    amountByMethod: CounterMap;
    countByMethod: CounterMap;
  } {
    return {
      totalAmount: PaymentMetricsSubscriber.totalAmount,
      transactionCount: PaymentMetricsSubscriber.transactionCount,
      successCount: PaymentMetricsSubscriber.successCount,
      failureCount: PaymentMetricsSubscriber.failureCount,
      successRate: PaymentMetricsSubscriber.successRate,
      avgTransactionAmount:
        PaymentMetricsSubscriber.transactionCount > 0
          ? Math.round(
              (PaymentMetricsSubscriber.totalAmount / PaymentMetricsSubscriber.transactionCount) *
                100
            ) / 100
          : 0,
      amountByMethod: { ...PaymentMetricsSubscriber.amountByMethod },
      countByMethod: { ...PaymentMetricsSubscriber.countByMethod },
    };
  }

  /**
   * Reset all metrics.
   */
  static reset(): void {
    PaymentMetricsSubscriber.totalAmount = 0;
    PaymentMetricsSubscriber.transactionCount = 0;
    PaymentMetricsSubscriber.successCount = 0;
    PaymentMetricsSubscriber.failureCount = 0;
    PaymentMetricsSubscriber.amountByMethod = {};
    PaymentMetricsSubscriber.countByMethod = {};
  }
}
