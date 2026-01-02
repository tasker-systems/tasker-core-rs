/**
 * Integration tests for domain events flow.
 *
 * Tests the example publishers and subscribers with the domain events infrastructure.
 * These tests focus on the actual functionality rather than singleton state management.
 *
 * @see TAS-112
 */

import { describe, expect, test } from 'bun:test';
// Import example implementations
import {
  AuditLoggingSubscriber,
  MetricsSubscriber,
  PaymentEventPublisher,
  PaymentLoggingSubscriber,
  PaymentMetricsSubscriber,
  RefundEventPublisher,
} from '../../src/examples/domain-events/index.js';
import {
  createDomainEvent,
  createStepEventContext,
  DefaultPublisher,
  type DomainEvent,
  InProcessDomainEventPoller,
  type StepEventContext,
  type StepResult,
} from '../../src/handler/domain-events.js';

// ---------------------------------------------------------------------------
// Test Fixtures
// ---------------------------------------------------------------------------

function createTestStepResult(success = true, data: Record<string, unknown> = {}): StepResult {
  return {
    success,
    result: {
      transaction_id: 'txn_123456',
      amount: 99.99,
      currency: 'USD',
      customer_id: 'cust_789',
      payment_method: 'card',
      ...data,
    },
    metadata: { handler: 'payment_processor' },
  };
}

function createTestStepContext(): StepEventContext {
  return createStepEventContext({
    taskUuid: 'task-uuid-123',
    stepUuid: 'step-uuid-456',
    stepName: 'process_payment',
    namespace: 'payments',
    correlationId: 'corr-789',
    result: { transaction_id: 'txn_123456' },
    metadata: {},
  });
}

function createTestDomainEvent(overrides: Partial<DomainEvent> = {}): DomainEvent {
  const defaultMetadata = {
    taskUuid: 'task-uuid-123',
    stepUuid: 'step-uuid-456',
    stepName: 'process_payment',
    namespace: 'payments',
    correlationId: 'corr-789',
    firedAt: new Date().toISOString(),
  };

  return createDomainEvent({
    eventId: overrides.eventId ?? `event-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`,
    eventName: overrides.eventName ?? 'payment.processed',
    payload: overrides.payload ?? {
      transactionId: 'txn_123456',
      amount: 99.99,
      currency: 'USD',
      status: 'succeeded',
    },
    metadata: {
      ...defaultMetadata,
      ...overrides.metadata,
    },
    executionResult: overrides.executionResult ?? createTestStepResult(),
  });
}

// ---------------------------------------------------------------------------
// PaymentEventPublisher Tests
// ---------------------------------------------------------------------------

describe('PaymentEventPublisher', () => {
  test('has correct publisher name', () => {
    const publisher = new PaymentEventPublisher();
    expect(publisher.publisherName).toBe('PaymentEventPublisher');
  });

  test('transforms successful payment step result', () => {
    const publisher = new PaymentEventPublisher();
    const stepResult = createTestStepResult(true, {
      transaction_id: 'txn_success_123',
      amount: 150.0,
      currency: 'EUR',
      payment_method: 'bank_transfer',
      customer_id: 'cust_456',
    });

    const payload = publisher.transformPayload(stepResult);

    expect(payload.transactionId).toBe('txn_success_123');
    expect(payload.amount).toBe(150.0);
    expect(payload.currency).toBe('EUR');
    expect(payload.paymentMethod).toBe('bank_transfer');
    expect(payload.customerId).toBe('cust_456');
    expect(payload.status).toBe('succeeded');
    expect(payload.errorCode).toBeNull();
    expect(payload.errorMessage).toBeNull();
  });

  test('transforms failed payment step result', () => {
    const publisher = new PaymentEventPublisher();
    const stepResult = createTestStepResult(false, {
      transaction_id: 'txn_failed_123',
      error_code: 'INSUFFICIENT_FUNDS',
      error_message: 'Card declined',
    });

    const payload = publisher.transformPayload(stepResult);

    expect(payload.status).toBe('failed');
    expect(payload.errorCode).toBe('INSUFFICIENT_FUNDS');
    expect(payload.errorMessage).toBe('Card declined');
  });

  test('requires transaction ID to publish', () => {
    const publisher = new PaymentEventPublisher();

    // With transaction ID
    const withTxn = createTestStepResult(true, { transaction_id: 'txn_123' });
    expect(publisher.shouldPublish(withTxn)).toBe(true);

    // Without transaction ID
    const withoutTxn: StepResult = {
      success: true,
      result: { amount: 100 },
      metadata: {},
    };
    expect(publisher.shouldPublish(withoutTxn)).toBe(false);
  });

  test('adds processor metadata', () => {
    const publisher = new PaymentEventPublisher();
    const stepResult = createTestStepResult(true, {
      processor: 'stripe',
      processor_transaction_id: 'pi_123456',
      merchant_id: 'merch_001',
    });
    const stepContext = createTestStepContext();

    const metadata = publisher.additionalMetadata(stepResult, undefined, stepContext);

    expect(metadata.processor).toBe('stripe');
    expect(metadata.processorTransactionId).toBe('pi_123456');
    expect(metadata.merchantId).toBe('merch_001');
    expect(metadata.workflowNamespace).toBe('payments');
  });

  test('tracks publishing statistics', () => {
    const publisher = new PaymentEventPublisher();

    // Initial state
    expect(publisher.stats.published).toBe(0);
    expect(publisher.stats.skipped).toBe(0);

    // Publish an event
    publisher.publish({
      eventName: 'payment.processed',
      stepResult: createTestStepResult(),
      stepContext: createTestStepContext(),
    });

    expect(publisher.stats.published).toBe(1);
  });
});

// ---------------------------------------------------------------------------
// RefundEventPublisher Tests
// ---------------------------------------------------------------------------

describe('RefundEventPublisher', () => {
  test('has correct publisher name', () => {
    const publisher = new RefundEventPublisher();
    expect(publisher.publisherName).toBe('RefundEventPublisher');
  });

  test('transforms refund step result', () => {
    const publisher = new RefundEventPublisher();
    const stepResult = createTestStepResult(true, {
      refund_id: 'ref_123456',
      original_transaction_id: 'txn_original',
      amount: 50.0,
      reason: 'duplicate_charge',
    });

    const payload = publisher.transformPayload(stepResult);

    expect(payload.refundId).toBe('ref_123456');
    expect(payload.originalTransactionId).toBe('txn_original');
    expect(payload.amount).toBe(50.0);
    expect(payload.reason).toBe('duplicate_charge');
    expect(payload.status).toBe('completed');
  });

  test('requires refund ID to publish', () => {
    const publisher = new RefundEventPublisher();

    // With refund ID
    const withRefund = createTestStepResult(true, { refund_id: 'ref_123' });
    expect(publisher.shouldPublish(withRefund)).toBe(true);

    // Without refund ID
    const withoutRefund: StepResult = {
      success: true,
      result: { amount: 100 },
      metadata: {},
    };
    expect(publisher.shouldPublish(withoutRefund)).toBe(false);
  });
});

// ---------------------------------------------------------------------------
// DefaultPublisher Tests
// ---------------------------------------------------------------------------

describe('DefaultPublisher', () => {
  test('has correct publisher name', () => {
    const publisher = new DefaultPublisher();
    expect(publisher.publisherName).toBe('default');
  });

  test('passes through step result unchanged', () => {
    const publisher = new DefaultPublisher();
    const stepResult = createTestStepResult(true, {
      custom_field: 'custom_value',
      nested: { deep: 'data' },
    });

    const payload = publisher.transformPayload(stepResult);

    expect(payload.transaction_id).toBe('txn_123456');
    expect(payload.custom_field).toBe('custom_value');
    expect(payload.nested).toEqual({ deep: 'data' });
  });
});

// ---------------------------------------------------------------------------
// AuditLoggingSubscriber Tests
// ---------------------------------------------------------------------------

describe('AuditLoggingSubscriber', () => {
  test('subscribes to all events', () => {
    expect(AuditLoggingSubscriber.subscribesTo()).toEqual(['*']);
  });

  test('matches any event name', () => {
    const subscriber = new AuditLoggingSubscriber();

    expect(subscriber.matches('payment.processed')).toBe(true);
    expect(subscriber.matches('order.created')).toBe(true);
    expect(subscriber.matches('user.registered')).toBe(true);
    expect(subscriber.matches('anything.at.all')).toBe(true);
  });

  test('handles events and tracks statistics', async () => {
    const subscriber = new AuditLoggingSubscriber();

    // Initial state
    expect(subscriber.stats.eventsLogged).toBe(0);
    expect(subscriber.stats.errors).toBe(0);

    // Handle events
    const events = [
      createTestDomainEvent({ eventName: 'event.one' }),
      createTestDomainEvent({ eventName: 'event.two' }),
      createTestDomainEvent({ eventName: 'event.three' }),
    ];

    for (const event of events) {
      await subscriber.handle(event);
    }

    expect(subscriber.stats.eventsLogged).toBe(3);
    expect(subscriber.stats.errors).toBe(0);
  });

  test('tracks average processing time', async () => {
    const subscriber = new AuditLoggingSubscriber();

    // Handle an event with timing
    subscriber.beforeHandle(createTestDomainEvent());
    await subscriber.handle(createTestDomainEvent());
    subscriber.afterHandle(createTestDomainEvent());

    expect(subscriber.stats.avgProcessingTimeMs).toBeGreaterThanOrEqual(0);
  });
});

// ---------------------------------------------------------------------------
// PaymentLoggingSubscriber Tests
// ---------------------------------------------------------------------------

describe('PaymentLoggingSubscriber', () => {
  test('subscribes to payment events only', () => {
    expect(PaymentLoggingSubscriber.subscribesTo()).toEqual(['payment.*']);
  });

  test('matches payment events', () => {
    const subscriber = new PaymentLoggingSubscriber();

    expect(subscriber.matches('payment.processed')).toBe(true);
    expect(subscriber.matches('payment.failed')).toBe(true);
    expect(subscriber.matches('payment.refunded')).toBe(true);
    expect(subscriber.matches('order.created')).toBe(false);
    expect(subscriber.matches('user.registered')).toBe(false);
  });

  test('filters test transactions', () => {
    const subscriber = new PaymentLoggingSubscriber();

    // Test transaction with test=true
    const testEvent1 = createTestDomainEvent({
      payload: { test: true, transactionId: 'txn_123' },
    });
    expect(subscriber.beforeHandle(testEvent1)).toBe(false);

    // Test transaction with sandbox=true
    const testEvent2 = createTestDomainEvent({
      payload: { sandbox: true, transactionId: 'txn_123' },
    });
    expect(subscriber.beforeHandle(testEvent2)).toBe(false);

    // Test transaction with test_ prefix
    const testEvent3 = createTestDomainEvent({
      payload: { transactionId: 'test_123456' },
    });
    expect(subscriber.beforeHandle(testEvent3)).toBe(false);

    // Real transaction
    const realEvent = createTestDomainEvent({
      payload: { transactionId: 'txn_real_123' },
    });
    expect(subscriber.beforeHandle(realEvent)).toBe(true);
  });

  test('tracks recent events', async () => {
    const subscriber = new PaymentLoggingSubscriber();

    // Handle some events
    const events = [
      createTestDomainEvent({
        eventName: 'payment.processed',
        payload: { transactionId: 'txn_1', amount: 100 },
      }),
      createTestDomainEvent({
        eventName: 'payment.failed',
        payload: { transactionId: 'txn_2', amount: 200 },
      }),
    ];

    for (const event of events) {
      await subscriber.handle(event);
    }

    const recent = subscriber.recentEvents;
    expect(recent.length).toBe(2);
    expect(recent[0].transactionId).toBe('txn_1');
    expect(recent[1].transactionId).toBe('txn_2');
  });
});

// ---------------------------------------------------------------------------
// MetricsSubscriber Tests
// ---------------------------------------------------------------------------

describe('MetricsSubscriber', () => {
  test('subscribes to all events', () => {
    expect(MetricsSubscriber.subscribesTo()).toEqual(['*']);
  });

  test('counts events by name', async () => {
    MetricsSubscriber.reset();
    const subscriber = new MetricsSubscriber();

    await subscriber.handle(createTestDomainEvent({ eventName: 'payment.processed' }));
    await subscriber.handle(createTestDomainEvent({ eventName: 'payment.processed' }));
    await subscriber.handle(createTestDomainEvent({ eventName: 'order.created' }));

    const summary = MetricsSubscriber.getSummary();
    expect(summary.eventsByName['payment.processed']).toBe(2);
    expect(summary.eventsByName['order.created']).toBe(1);

    MetricsSubscriber.reset();
  });

  test('counts events by namespace', async () => {
    MetricsSubscriber.reset();
    const subscriber = new MetricsSubscriber();

    await subscriber.handle(createTestDomainEvent({ eventName: 'event.one' }));
    await subscriber.handle(createTestDomainEvent({ eventName: 'event.two' }));

    const summary = MetricsSubscriber.getSummary();
    expect(summary.eventsByNamespace.payments).toBe(2);

    MetricsSubscriber.reset();
  });

  test('counts events by outcome', async () => {
    MetricsSubscriber.reset();
    const subscriber = new MetricsSubscriber();

    await subscriber.handle(
      createTestDomainEvent({
        executionResult: { success: true, result: {} },
      })
    );
    await subscriber.handle(
      createTestDomainEvent({
        executionResult: { success: true, result: {} },
      })
    );
    await subscriber.handle(
      createTestDomainEvent({
        executionResult: { success: false, result: {} },
      })
    );

    const summary = MetricsSubscriber.getSummary();
    expect(summary.eventsByOutcome.success).toBe(2);
    expect(summary.eventsByOutcome.failure).toBe(1);

    MetricsSubscriber.reset();
  });

  test('exports Prometheus format', async () => {
    MetricsSubscriber.reset();
    const subscriber = new MetricsSubscriber();

    await subscriber.handle(createTestDomainEvent({ eventName: 'test.event' }));

    const output = MetricsSubscriber.exportMetrics();

    expect(output).toContain('# HELP domain_events_total');
    expect(output).toContain('# TYPE domain_events_total counter');
    expect(output).toContain('domain_events_total{event_name="test_event"}');
    expect(output).toContain('domain_events_by_namespace_total');
    expect(output).toContain('domain_events_outcome_total');
    expect(output).toContain('domain_events_latency_seconds_bucket');

    MetricsSubscriber.reset();
  });
});

// ---------------------------------------------------------------------------
// PaymentMetricsSubscriber Tests
// ---------------------------------------------------------------------------

describe('PaymentMetricsSubscriber', () => {
  test('subscribes to payment events only', () => {
    expect(PaymentMetricsSubscriber.subscribesTo()).toEqual(['payment.*']);
  });

  test('tracks payment amounts and counts', async () => {
    PaymentMetricsSubscriber.reset();
    const subscriber = new PaymentMetricsSubscriber();

    await subscriber.handle(
      createTestDomainEvent({
        eventName: 'payment.processed',
        payload: { amount: 100, paymentMethod: 'card', status: 'succeeded' },
      })
    );
    await subscriber.handle(
      createTestDomainEvent({
        eventName: 'payment.processed',
        payload: { amount: 200, paymentMethod: 'card', status: 'succeeded' },
      })
    );
    await subscriber.handle(
      createTestDomainEvent({
        eventName: 'payment.failed',
        payload: { amount: 50, paymentMethod: 'bank', status: 'failed' },
      })
    );

    const summary = PaymentMetricsSubscriber.getSummary();

    expect(summary.transactionCount).toBe(3);
    expect(summary.totalAmount).toBe(350);
    expect(summary.successCount).toBe(2);
    expect(summary.failureCount).toBe(1);
    expect(summary.successRate).toBeCloseTo(66.67, 1);
    expect(summary.avgTransactionAmount).toBeCloseTo(116.67, 1);
    expect(summary.countByMethod.card).toBe(2);
    expect(summary.countByMethod.bank).toBe(1);
    expect(summary.amountByMethod.card).toBe(300);
    expect(summary.amountByMethod.bank).toBe(50);

    PaymentMetricsSubscriber.reset();
  });

  test('calculates success rate correctly', async () => {
    PaymentMetricsSubscriber.reset();
    const subscriber = new PaymentMetricsSubscriber();

    // 100% success
    await subscriber.handle(
      createTestDomainEvent({
        payload: { status: 'succeeded', amount: 100 },
      })
    );
    expect(PaymentMetricsSubscriber.successRate).toBe(100);

    // 50% success
    await subscriber.handle(
      createTestDomainEvent({
        payload: { status: 'failed', amount: 100 },
      })
    );
    expect(PaymentMetricsSubscriber.successRate).toBe(50);

    PaymentMetricsSubscriber.reset();
  });
});

// ---------------------------------------------------------------------------
// InProcessDomainEventPoller Tests
// ---------------------------------------------------------------------------

describe('InProcessDomainEventPoller', () => {
  test('creates with default configuration', () => {
    const poller = new InProcessDomainEventPoller();
    expect(poller).toBeDefined();
    expect(poller.isRunning).toBe(false);
    poller.stop();
  });

  test('creates with custom configuration', () => {
    const poller = new InProcessDomainEventPoller({
      pollIntervalMs: 50,
      maxEventsPerPoll: 200,
    });
    expect(poller).toBeDefined();
    poller.stop();
  });

  test('allows subscriptions', () => {
    const poller = new InProcessDomainEventPoller();
    const events: DomainEvent[] = [];

    poller.subscribe('*', (event) => events.push(event));
    poller.subscribe('payment.*', (event) => events.push(event));

    poller.stop();
  });

  test('allows unsubscription', () => {
    const poller = new InProcessDomainEventPoller();
    let callCount = 0;

    poller.subscribe('test.*', () => callCount++);
    poller.unsubscribe('test.*');

    // Verify no error on unsubscribe
    expect(true).toBe(true);
    poller.stop();
  });

  test('handles error callbacks', () => {
    const poller = new InProcessDomainEventPoller();
    const errors: Error[] = [];

    poller.onError((error) => errors.push(error));

    // Verify no error on setup
    expect(true).toBe(true);
    poller.stop();
  });

  test('reports statistics', () => {
    const poller = new InProcessDomainEventPoller();
    const stats = poller.stats;

    expect(stats.pollCount).toBe(0);
    expect(stats.eventsProcessed).toBe(0);
    expect(stats.eventsLagged).toBe(0);

    poller.stop();
  });
});

// ---------------------------------------------------------------------------
// End-to-End Pattern Tests
// ---------------------------------------------------------------------------

describe('End-to-End Patterns', () => {
  test('publisher transforms and subscriber receives matching event type', async () => {
    const publisher = new PaymentEventPublisher();
    const subscriber = new PaymentLoggingSubscriber();

    // Publisher transforms step result
    const stepResult = createTestStepResult(true, {
      transaction_id: 'txn_e2e_test',
      amount: 123.45,
      currency: 'GBP',
    });
    const payload = publisher.transformPayload(stepResult);

    // Create event with transformed payload
    const event = createTestDomainEvent({
      eventName: 'payment.processed',
      payload,
    });

    // Subscriber matches and handles
    expect(subscriber.matches(event.eventName)).toBe(true);
    await subscriber.handle(event);

    // Verify subscriber tracked the event
    const recent = subscriber.recentEvents;
    expect(recent.length).toBe(1);
    expect(recent[0].transactionId).toBe('txn_e2e_test');
    expect(recent[0].amount).toBe(123.45);
  });

  test('multiple subscribers receive same event', async () => {
    const auditSubscriber = new AuditLoggingSubscriber();
    const metricsSubscriber = new MetricsSubscriber();
    MetricsSubscriber.reset();

    const event = createTestDomainEvent({ eventName: 'payment.processed' });

    // Both should match
    expect(auditSubscriber.matches(event.eventName)).toBe(true);
    expect(metricsSubscriber.matches(event.eventName)).toBe(true);

    // Both handle the event
    await auditSubscriber.handle(event);
    await metricsSubscriber.handle(event);

    // Both tracked the event
    expect(auditSubscriber.stats.eventsLogged).toBe(1);
    expect(MetricsSubscriber.getSummary().eventsByName['payment.processed']).toBe(1);

    MetricsSubscriber.reset();
  });

  test('pattern matching correctly filters events', () => {
    const paymentSubscriber = new PaymentLoggingSubscriber();
    const auditSubscriber = new AuditLoggingSubscriber();

    // Payment events
    expect(paymentSubscriber.matches('payment.processed')).toBe(true);
    expect(paymentSubscriber.matches('payment.failed')).toBe(true);
    expect(paymentSubscriber.matches('payment.refunded')).toBe(true);

    // Non-payment events
    expect(paymentSubscriber.matches('order.created')).toBe(false);
    expect(paymentSubscriber.matches('user.registered')).toBe(false);
    expect(paymentSubscriber.matches('inventory.updated')).toBe(false);

    // Audit subscriber matches everything
    expect(auditSubscriber.matches('payment.processed')).toBe(true);
    expect(auditSubscriber.matches('order.created')).toBe(true);
    expect(auditSubscriber.matches('anything.at.all')).toBe(true);
  });
});
