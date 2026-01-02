/**
 * Domain Events Module Tests.
 *
 * TAS-122: TypeScript Domain Events Implementation
 *
 * Verifies:
 * - BasePublisher lifecycle hooks and payload transformation
 * - BaseSubscriber pattern matching and event handling
 * - PublisherRegistry registration, resolution, and freezing
 * - SubscriberRegistry lifecycle management
 * - InProcessDomainEventPoller polling and subscription
 * - Factory functions for creating contexts and events
 */

import { afterEach, beforeEach, describe, expect, it, mock } from 'bun:test';
import {
  BasePublisher,
  BaseSubscriber,
  createDomainEvent,
  createStepEventContext,
  DefaultPublisher,
  type DomainEvent,
  DuplicatePublisherError,
  type EventDeclaration,
  InProcessDomainEventPoller,
  type PublishContext,
  PublisherNotFoundError,
  PublisherRegistry,
  PublisherValidationError,
  RegistryFrozenError,
  type StepEventContext,
  type StepResult,
  SubscriberRegistry,
} from '../../../src/handler/domain-events.js';

// ---------------------------------------------------------------------------
// Test Fixtures
// ---------------------------------------------------------------------------

function createTestStepResult(overrides: Partial<StepResult> = {}): StepResult {
  return {
    success: true,
    result: { transaction_id: 'txn-123', amount: 100 },
    metadata: { execution_time_ms: 50 },
    ...overrides,
  };
}

function createTestPublishContext(overrides: Partial<PublishContext> = {}): PublishContext {
  return {
    eventName: 'payment.processed',
    stepResult: createTestStepResult(),
    eventDeclaration: createTestEventDeclaration(),
    stepContext: createTestStepContext(),
    ...overrides,
  };
}

function createTestEventDeclaration(overrides: Partial<EventDeclaration> = {}): EventDeclaration {
  return {
    name: 'payment.processed',
    condition: 'success',
    deliveryMode: 'fast',
    ...overrides,
  };
}

function createTestStepContext(overrides: Partial<StepEventContext> = {}): StepEventContext {
  return createStepEventContext({
    taskUuid: 'task-uuid-123',
    stepUuid: 'step-uuid-456',
    stepName: 'process_payment',
    namespace: 'payments',
    correlationId: 'correlation-789',
    result: { transaction_id: 'txn-123' },
    metadata: { attempt: 1 },
    ...overrides,
  });
}

function createTestDomainEvent(overrides: Partial<DomainEvent> = {}): DomainEvent {
  return createDomainEvent({
    eventId: 'event-uuid-001',
    eventName: 'payment.processed',
    payload: { transaction_id: 'txn-123', amount: 100 },
    metadata: {
      taskUuid: 'task-uuid-123',
      stepUuid: 'step-uuid-456',
      stepName: 'process_payment',
      namespace: 'payments',
      correlationId: 'correlation-789',
      publishedAt: new Date().toISOString(),
    },
    executionResult: createTestStepResult(),
    ...overrides,
  });
}

// ---------------------------------------------------------------------------
// Test Publisher Implementations
// ---------------------------------------------------------------------------

class TestPublisher extends BasePublisher {
  readonly publisherName = 'TestPublisher';

  public callCount = 0;
  public lastTransformArgs: unknown[] = [];

  transformPayload(
    stepResult: StepResult,
    eventDeclaration?: EventDeclaration,
    stepContext?: StepEventContext
  ): Record<string, unknown> {
    this.lastTransformArgs = [stepResult, eventDeclaration, stepContext];
    this.callCount++;
    return {
      transformed: true,
      original: stepResult.result,
    };
  }
}

class TestPublisher2 extends BasePublisher {
  readonly publisherName = 'TestPublisher2';

  transformPayload(stepResult: StepResult): Record<string, unknown> {
    return stepResult.result ?? {};
  }
}

class ConditionalPublisher extends BasePublisher {
  readonly publisherName = 'ConditionalPublisher';

  shouldPublish(
    stepResult: StepResult,
    eventDeclaration?: EventDeclaration,
    _stepContext?: StepEventContext
  ): boolean {
    // Only publish for success events with transaction_id
    if (eventDeclaration?.name.includes('processed')) {
      return stepResult.success && Boolean(stepResult.result?.transaction_id);
    }
    return true;
  }

  transformPayload(stepResult: StepResult): Record<string, unknown> {
    return stepResult.result ?? {};
  }
}

class LifecycleTrackingPublisher extends BasePublisher {
  readonly publisherName = 'LifecycleTrackingPublisher';

  public hooks: string[] = [];

  transformPayload(stepResult: StepResult): Record<string, unknown> {
    return stepResult.result ?? {};
  }

  beforePublish(
    eventName: string,
    _payload: Record<string, unknown>,
    _metadata: Record<string, unknown>
  ): boolean {
    this.hooks.push(`beforePublish:${eventName}`);
    return true;
  }

  afterPublish(
    eventName: string,
    _payload: Record<string, unknown>,
    _metadata: Record<string, unknown>
  ): void {
    this.hooks.push(`afterPublish:${eventName}`);
  }

  onPublishError(eventName: string, error: Error, _payload: Record<string, unknown>): void {
    this.hooks.push(`onPublishError:${eventName}:${error.message}`);
  }
}

// ---------------------------------------------------------------------------
// Test Subscriber Implementations
// ---------------------------------------------------------------------------

class TestSubscriber extends BaseSubscriber {
  readonly subscriberName = 'TestSubscriber';

  static subscribesTo(): string[] {
    return ['payment.*'];
  }

  public receivedEvents: DomainEvent[] = [];

  async handle(event: DomainEvent): Promise<void> {
    this.receivedEvents.push(event);
  }
}

class WildcardSubscriber extends BaseSubscriber {
  readonly subscriberName = 'WildcardSubscriber';

  static subscribesTo(): string[] {
    return ['*'];
  }

  public eventCount = 0;

  async handle(_event: DomainEvent): Promise<void> {
    this.eventCount++;
  }
}

class MultiPatternSubscriber extends BaseSubscriber {
  readonly subscriberName = 'MultiPatternSubscriber';

  static subscribesTo(): string[] {
    return ['payment.processed', 'order.created'];
  }

  public matchedEvents: string[] = [];

  async handle(event: DomainEvent): Promise<void> {
    this.matchedEvents.push(event.eventName);
  }
}

/**
 * Subscriber that tracks lifecycle hook calls for testing.
 */
class LifecycleTrackingSubscriber extends BaseSubscriber {
  readonly subscriberName = 'LifecycleTrackingSubscriber';

  static subscribesTo(): string[] {
    return ['*'];
  }

  public hooks: string[] = [];
  public handledEvents: DomainEvent[] = [];
  public shouldSkip = false;
  public shouldFail = false;
  public lastError: Error | null = null;

  beforeHandle(event: DomainEvent): boolean {
    this.hooks.push(`beforeHandle:${event.eventName}`);
    return !this.shouldSkip;
  }

  async handle(event: DomainEvent): Promise<void> {
    this.hooks.push(`handle:${event.eventName}`);
    if (this.shouldFail) {
      throw new Error('Simulated handler error');
    }
    this.handledEvents.push(event);
  }

  afterHandle(event: DomainEvent): void {
    this.hooks.push(`afterHandle:${event.eventName}`);
  }

  onHandleError(event: DomainEvent, error: Error): void {
    this.hooks.push(`onHandleError:${event.eventName}:${error.message}`);
    this.lastError = error;
  }

  reset(): void {
    this.hooks = [];
    this.handledEvents = [];
    this.shouldSkip = false;
    this.shouldFail = false;
    this.lastError = null;
  }
}

// ---------------------------------------------------------------------------
// BasePublisher Tests
// ---------------------------------------------------------------------------

describe('BasePublisher', () => {
  describe('publish', () => {
    it('transforms payload using transformPayload hook', () => {
      const publisher = new TestPublisher();
      const ctx = createTestPublishContext();

      const result = publisher.publish(ctx);

      expect(result).toBe(true);
      expect(publisher.callCount).toBe(1);
    });

    it('respects shouldPublish returning false', () => {
      const publisher = new ConditionalPublisher();
      const ctx = createTestPublishContext({
        stepResult: createTestStepResult({
          success: true,
          result: { no_transaction: true }, // Missing transaction_id
        }),
        eventDeclaration: createTestEventDeclaration({
          name: 'payment.processed',
        }),
      });

      const result = publisher.publish(ctx);

      expect(result).toBe(false);
    });

    it('calls lifecycle hooks in correct order', () => {
      const publisher = new LifecycleTrackingPublisher();
      const ctx = createTestPublishContext({
        eventName: 'test.event',
        eventDeclaration: createTestEventDeclaration({ name: 'test.event' }),
      });

      publisher.publish(ctx);

      expect(publisher.hooks).toEqual(['beforePublish:test.event', 'afterPublish:test.event']);
    });
  });
});

describe('DefaultPublisher', () => {
  it('passes through step result via publish context', () => {
    const publisher = new DefaultPublisher();
    const ctx = createTestPublishContext({
      stepResult: createTestStepResult({
        result: { custom: 'data', count: 42 },
      }),
    });

    const result = publisher.publish(ctx);

    expect(result).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// BaseSubscriber Tests
// ---------------------------------------------------------------------------

describe('BaseSubscriber', () => {
  describe('matches', () => {
    it('matches exact event names', () => {
      const subscriber = new MultiPatternSubscriber();

      expect(subscriber.matches('payment.processed')).toBe(true);
      expect(subscriber.matches('order.created')).toBe(true);
      expect(subscriber.matches('unknown.event')).toBe(false);
    });

    it('matches wildcard patterns', () => {
      const subscriber = new WildcardSubscriber();

      expect(subscriber.matches('payment.processed')).toBe(true);
      expect(subscriber.matches('order.created')).toBe(true);
      expect(subscriber.matches('anything.here')).toBe(true);
    });

    it('matches namespace wildcards', () => {
      const subscriber = new TestSubscriber();

      expect(subscriber.matches('payment.processed')).toBe(true);
      expect(subscriber.matches('payment.failed')).toBe(true);
      expect(subscriber.matches('order.created')).toBe(false);
    });
  });

  describe('handle', () => {
    it('can be called directly with an event', async () => {
      const subscriber = new TestSubscriber();
      const event = createTestDomainEvent({ eventName: 'payment.processed' });

      await subscriber.handle(event);

      expect(subscriber.receivedEvents).toHaveLength(1);
      expect(subscriber.receivedEvents[0]).toBe(event);
    });

    it('stores multiple events', async () => {
      const subscriber = new TestSubscriber();
      const event1 = createTestDomainEvent({ eventName: 'payment.processed' });
      const event2 = createTestDomainEvent({ eventName: 'payment.failed' });

      await subscriber.handle(event1);
      await subscriber.handle(event2);

      expect(subscriber.receivedEvents).toHaveLength(2);
    });
  });

  describe('lifecycle hooks', () => {
    it('calls beforeHandle before handle', async () => {
      const subscriber = new LifecycleTrackingSubscriber();
      const poller = new InProcessDomainEventPoller({ pollIntervalMs: 100 });

      // Start subscriber to enable event handling
      subscriber.start(poller);

      // Manually invoke the safe handler to test lifecycle
      // @ts-expect-error - accessing private method for testing
      await subscriber.handleEventSafely(createTestDomainEvent({ eventName: 'test.event' }));

      expect(subscriber.hooks[0]).toBe('beforeHandle:test.event');
      expect(subscriber.hooks[1]).toBe('handle:test.event');

      subscriber.stop();
      poller.stop();
    });

    it('calls afterHandle on successful handling', async () => {
      const subscriber = new LifecycleTrackingSubscriber();
      const poller = new InProcessDomainEventPoller({ pollIntervalMs: 100 });

      subscriber.start(poller);

      // @ts-expect-error - accessing private method for testing
      await subscriber.handleEventSafely(createTestDomainEvent({ eventName: 'test.event' }));

      expect(subscriber.hooks).toContain('afterHandle:test.event');
      expect(subscriber.handledEvents).toHaveLength(1);

      subscriber.stop();
      poller.stop();
    });

    it('calls hooks in correct order', async () => {
      const subscriber = new LifecycleTrackingSubscriber();
      const poller = new InProcessDomainEventPoller({ pollIntervalMs: 100 });

      subscriber.start(poller);

      // @ts-expect-error - accessing private method for testing
      await subscriber.handleEventSafely(createTestDomainEvent({ eventName: 'test.event' }));

      expect(subscriber.hooks).toEqual([
        'beforeHandle:test.event',
        'handle:test.event',
        'afterHandle:test.event',
      ]);

      subscriber.stop();
      poller.stop();
    });

    it('skips handling when beforeHandle returns false', async () => {
      const subscriber = new LifecycleTrackingSubscriber();
      const poller = new InProcessDomainEventPoller({ pollIntervalMs: 100 });

      subscriber.start(poller);
      subscriber.shouldSkip = true;

      // @ts-expect-error - accessing private method for testing
      await subscriber.handleEventSafely(createTestDomainEvent({ eventName: 'test.event' }));

      expect(subscriber.hooks).toEqual(['beforeHandle:test.event']);
      expect(subscriber.handledEvents).toHaveLength(0);

      subscriber.stop();
      poller.stop();
    });

    it('calls onHandleError when handle throws', async () => {
      const subscriber = new LifecycleTrackingSubscriber();
      const poller = new InProcessDomainEventPoller({ pollIntervalMs: 100 });

      subscriber.start(poller);
      subscriber.shouldFail = true;

      // @ts-expect-error - accessing private method for testing
      await subscriber.handleEventSafely(createTestDomainEvent({ eventName: 'test.event' }));

      expect(subscriber.hooks).toContain('onHandleError:test.event:Simulated handler error');
      expect(subscriber.lastError?.message).toBe('Simulated handler error');
      // afterHandle should NOT be called on error
      expect(subscriber.hooks).not.toContain('afterHandle:test.event');

      subscriber.stop();
      poller.stop();
    });

    it('does not propagate errors from handle', async () => {
      const subscriber = new LifecycleTrackingSubscriber();
      const poller = new InProcessDomainEventPoller({ pollIntervalMs: 100 });

      subscriber.start(poller);
      subscriber.shouldFail = true;

      // Should not throw
      // @ts-expect-error - accessing private method for testing
      await subscriber.handleEventSafely(createTestDomainEvent({ eventName: 'test.event' }));

      subscriber.stop();
      poller.stop();
    });

    it('inactive subscriber does not process events', async () => {
      const subscriber = new LifecycleTrackingSubscriber();
      // Don't start the subscriber - it should be inactive

      // @ts-expect-error - accessing private method for testing
      await subscriber.handleEventSafely(createTestDomainEvent({ eventName: 'test.event' }));

      expect(subscriber.hooks).toHaveLength(0);
      expect(subscriber.handledEvents).toHaveLength(0);
    });
  });

  describe('start/stop', () => {
    it('starts inactive and becomes active after start', () => {
      const subscriber = new TestSubscriber();
      const poller = new InProcessDomainEventPoller({ pollIntervalMs: 100 });

      expect(subscriber.active).toBe(false);

      subscriber.start(poller);

      expect(subscriber.active).toBe(true);

      subscriber.stop();
      poller.stop();
    });

    it('becomes inactive after stop', () => {
      const subscriber = new TestSubscriber();
      const poller = new InProcessDomainEventPoller({ pollIntervalMs: 100 });

      subscriber.start(poller);
      subscriber.stop();

      expect(subscriber.active).toBe(false);

      poller.stop();
    });

    it('tracks subscriptions after start', () => {
      const subscriber = new TestSubscriber();
      const poller = new InProcessDomainEventPoller({ pollIntervalMs: 100 });

      subscriber.start(poller);

      expect(subscriber.subscriptions).toEqual(['payment.*']);

      subscriber.stop();
      poller.stop();
    });
  });
});

// ---------------------------------------------------------------------------
// PublisherRegistry Tests
// ---------------------------------------------------------------------------

describe('PublisherRegistry', () => {
  let registry: PublisherRegistry;

  beforeEach(() => {
    registry = PublisherRegistry.instance;
    registry.reset();
  });

  describe('register', () => {
    it('registers a publisher', () => {
      const publisher = new TestPublisher();

      registry.register(publisher);

      expect(registry.isRegistered('TestPublisher')).toBe(true);
    });

    it('throws DuplicatePublisherError for duplicate names', () => {
      const publisher1 = new TestPublisher();
      const publisher2 = new TestPublisher();

      registry.register(publisher1);

      expect(() => registry.register(publisher2)).toThrow(DuplicatePublisherError);
    });

    it('throws RegistryFrozenError when frozen', () => {
      registry.freeze();

      const publisher = new TestPublisher();

      expect(() => registry.register(publisher)).toThrow(RegistryFrozenError);
    });
  });

  describe('get', () => {
    it('returns registered publisher', () => {
      const publisher = new TestPublisher();
      registry.register(publisher);

      const resolved = registry.get('TestPublisher');

      expect(resolved).toBe(publisher);
    });

    it('returns undefined for unknown publisher', () => {
      const result = registry.get('UnknownPublisher');

      expect(result).toBeUndefined();
    });
  });

  describe('getStrict', () => {
    it('throws PublisherNotFoundError for unknown publisher', () => {
      expect(() => registry.getStrict('UnknownPublisher')).toThrow(PublisherNotFoundError);
    });

    it('returns default publisher when name is "default"', () => {
      const resolved = registry.getStrict('default');

      expect(resolved).toBeInstanceOf(DefaultPublisher);
    });
  });

  describe('getOrDefault', () => {
    it('returns publisher if registered', () => {
      const publisher = new TestPublisher();
      registry.register(publisher);

      const resolved = registry.getOrDefault('TestPublisher');

      expect(resolved).toBe(publisher);
    });

    it('returns default publisher for empty name', () => {
      const resolved = registry.getOrDefault('');

      expect(resolved).toBeInstanceOf(DefaultPublisher);
    });

    it('returns default publisher for unknown name', () => {
      const resolved = registry.getOrDefault('Unknown');

      expect(resolved).toBeInstanceOf(DefaultPublisher);
    });
  });

  describe('freeze/reset', () => {
    it('freezes registry preventing new registrations', () => {
      registry.freeze();

      expect(registry.isFrozen).toBe(true);
    });

    it('reset unfreezes and clears registrations', () => {
      const publisher = new TestPublisher();
      registry.register(publisher);
      registry.freeze();

      registry.reset();

      expect(registry.isFrozen).toBe(false);
      expect(registry.isRegistered('TestPublisher')).toBe(false);
    });
  });

  describe('count', () => {
    it('returns number of registered publishers', () => {
      expect(registry.count).toBe(0);

      registry.register(new TestPublisher());
      expect(registry.count).toBe(1);

      registry.register(new TestPublisher2());
      expect(registry.count).toBe(2);
    });
  });
});

// ---------------------------------------------------------------------------
// SubscriberRegistry Tests
// ---------------------------------------------------------------------------

describe('SubscriberRegistry', () => {
  let registry: SubscriberRegistry;
  let poller: InProcessDomainEventPoller;

  beforeEach(() => {
    registry = SubscriberRegistry.instance;
    registry.reset();
    poller = new InProcessDomainEventPoller({ pollIntervalMs: 100 });
  });

  afterEach(() => {
    registry.stopAll();
    poller.stop();
  });

  describe('register', () => {
    it('registers a subscriber class', () => {
      registry.register(TestSubscriber);

      expect(registry.count).toBe(1);
    });

    it('registers multiple subscriber classes', () => {
      registry.register(TestSubscriber);
      registry.register(WildcardSubscriber);

      expect(registry.count).toBe(2);
    });
  });

  describe('registerInstance', () => {
    it('registers a subscriber instance', () => {
      const subscriber = new TestSubscriber();

      registry.registerInstance(subscriber);

      expect(registry.count).toBe(1);
    });

    it('registers multiple instances', () => {
      registry.registerInstance(new TestSubscriber());
      registry.registerInstance(new WildcardSubscriber());

      expect(registry.count).toBe(2);
    });
  });

  describe('startAll', () => {
    it('instantiates and starts registered classes', () => {
      registry.register(TestSubscriber);
      registry.register(WildcardSubscriber);

      registry.startAll(poller);

      expect(registry.stats.activeCount).toBe(2);
    });

    it('starts pre-registered instances', () => {
      const subscriber = new TestSubscriber();
      registry.registerInstance(subscriber);

      registry.startAll(poller);

      expect(subscriber.active).toBe(true);
      expect(registry.stats.activeCount).toBe(1);
    });

    it('starts both classes and instances', () => {
      registry.register(TestSubscriber);
      registry.registerInstance(new WildcardSubscriber());

      registry.startAll(poller);

      expect(registry.stats.activeCount).toBe(2);
    });
  });

  describe('stopAll', () => {
    it('stops all active subscribers', () => {
      registry.register(TestSubscriber);
      registry.registerInstance(new WildcardSubscriber());

      registry.startAll(poller);
      expect(registry.stats.activeCount).toBe(2);

      registry.stopAll();
      expect(registry.stats.activeCount).toBe(0);
    });

    it('is idempotent', () => {
      registry.register(TestSubscriber);
      registry.startAll(poller);

      registry.stopAll();
      registry.stopAll(); // Should not throw

      expect(registry.stats.activeCount).toBe(0);
    });
  });

  describe('stats', () => {
    it('tracks subscriber count', () => {
      registry.register(TestSubscriber);
      registry.register(WildcardSubscriber);

      registry.startAll(poller);

      expect(registry.stats.subscriberCount).toBe(2);
    });

    it('tracks started status', () => {
      registry.register(TestSubscriber);

      expect(registry.stats.started).toBe(false);

      registry.startAll(poller);

      expect(registry.stats.started).toBe(true);
    });

    it('tracks active count after startAll', () => {
      registry.register(TestSubscriber);
      registry.registerInstance(new WildcardSubscriber());

      registry.startAll(poller);

      expect(registry.stats.activeCount).toBe(2);
    });

    it('provides subscriber info in stats', () => {
      registry.register(TestSubscriber);
      registry.startAll(poller);

      const stats = registry.stats;

      expect(stats.subscribers).toHaveLength(1);
      expect(stats.subscribers[0].className).toBe('TestSubscriber');
      expect(stats.subscribers[0].active).toBe(true);
      expect(stats.subscribers[0].patterns).toEqual(['payment.*']);
    });
  });

  describe('reset', () => {
    it('clears all registrations and stops active subscribers', () => {
      registry.register(TestSubscriber);
      registry.registerInstance(new WildcardSubscriber());
      registry.startAll(poller);

      registry.reset();

      expect(registry.count).toBe(0);
      expect(registry.stats.activeCount).toBe(0);
    });
  });

  describe('count', () => {
    it('tracks total registered (classes + instances)', () => {
      expect(registry.count).toBe(0);

      registry.register(TestSubscriber);
      expect(registry.count).toBe(1);

      registry.registerInstance(new WildcardSubscriber());
      expect(registry.count).toBe(2);
    });
  });
});

// ---------------------------------------------------------------------------
// InProcessDomainEventPoller Tests
// ---------------------------------------------------------------------------

describe('InProcessDomainEventPoller', () => {
  let poller: InProcessDomainEventPoller;

  beforeEach(() => {
    poller = new InProcessDomainEventPoller({
      pollIntervalMs: 10,
      maxEventsPerPoll: 10,
    });
  });

  afterEach(() => {
    poller.stop();
  });

  describe('subscribe/unsubscribe', () => {
    it('registers subscription for pattern', () => {
      const callback = mock(() => {});

      poller.subscribe('payment.*', callback);

      expect(poller.stats.running).toBe(false); // Not started yet
    });

    it('unsubscribes from pattern', () => {
      const callback = mock(() => {});

      poller.subscribe('payment.*', callback);
      poller.unsubscribe('payment.*');

      // No way to verify directly, but should not throw
    });
  });

  describe('start/stop', () => {
    it('starts polling', () => {
      poller.start();

      expect(poller.stats.running).toBe(true);
    });

    it('stops polling', () => {
      poller.start();
      poller.stop();

      expect(poller.stats.running).toBe(false);
    });
  });

  describe('stats', () => {
    it('tracks poll count and events', () => {
      const stats = poller.stats;

      expect(stats.pollCount).toBe(0);
      expect(stats.eventsProcessed).toBe(0);
      expect(stats.eventsLagged).toBe(0);
    });
  });
});

// ---------------------------------------------------------------------------
// Factory Function Tests
// ---------------------------------------------------------------------------

describe('createStepEventContext', () => {
  it('creates context with all fields', () => {
    const context = createStepEventContext({
      taskUuid: 'task-123',
      stepUuid: 'step-456',
      stepName: 'process_payment',
      namespace: 'payments',
      correlationId: 'corr-789',
      result: { amount: 100 },
      metadata: { attempt: 1 },
    });

    expect(context.taskUuid).toBe('task-123');
    expect(context.stepUuid).toBe('step-456');
    expect(context.stepName).toBe('process_payment');
    expect(context.namespace).toBe('payments');
    expect(context.correlationId).toBe('corr-789');
    expect(context.result).toEqual({ amount: 100 });
    expect(context.metadata).toEqual({ attempt: 1 });
  });

  it('uses defaults for optional fields', () => {
    const context = createStepEventContext({
      taskUuid: 'task-123',
      stepUuid: 'step-456',
      stepName: 'process_payment',
    });

    expect(context.namespace).toBe('default');
    expect(context.correlationId).toBe('task-123'); // Uses taskUuid as default
    expect(context.metadata).toEqual({});
    expect(context.result).toBeUndefined();
  });
});

describe('createDomainEvent', () => {
  it('creates event with all fields', () => {
    const event = createDomainEvent({
      eventId: 'evt-001',
      eventName: 'payment.processed',
      payload: { amount: 100 },
      metadata: {
        taskUuid: 'task-123',
        stepUuid: 'step-456',
        stepName: 'process_payment',
        namespace: 'payments',
        correlationId: 'corr-789',
        publishedAt: '2024-01-01T00:00:00Z',
      },
      executionResult: createTestStepResult(),
    });

    expect(event.eventId).toBe('evt-001');
    expect(event.eventName).toBe('payment.processed');
    expect(event.payload).toEqual({ amount: 100 });
    expect(event.metadata.taskUuid).toBe('task-123');
  });
});

// ---------------------------------------------------------------------------
// Error Classes Tests
// ---------------------------------------------------------------------------

describe('Error Classes', () => {
  it('PublisherNotFoundError includes publisher name and registered list', () => {
    const error = new PublisherNotFoundError('CustomPublisher', ['Publisher1', 'Publisher2']);

    expect(error.message).toContain('CustomPublisher');
    expect(error.publisherName).toBe('CustomPublisher');
    expect(error.registeredPublishers).toEqual(['Publisher1', 'Publisher2']);
  });

  it('PublisherValidationError includes missing and registered lists', () => {
    const error = new PublisherValidationError(['Missing1'], ['Registered1']);

    expect(error.message).toContain('Missing1');
    expect(error.message).toContain('Registered1');
    expect(error.missingPublishers).toEqual(['Missing1']);
    expect(error.registeredPublishers).toEqual(['Registered1']);
  });

  it('RegistryFrozenError has descriptive message', () => {
    const error = new RegistryFrozenError();

    expect(error.message).toContain('frozen');
  });

  it('DuplicatePublisherError includes publisher name', () => {
    const error = new DuplicatePublisherError('TestPublisher');

    expect(error.message).toContain('TestPublisher');
    expect(error.publisherName).toBe('TestPublisher');
  });
});
