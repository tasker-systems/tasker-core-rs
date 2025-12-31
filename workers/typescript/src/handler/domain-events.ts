/**
 * Domain Events Module for TypeScript Workers
 *
 * Provides infrastructure for custom domain event publishers and subscribers.
 * Publishers transform step results into business events; subscribers handle
 * fast (in-process) events for internal processing.
 *
 * Architecture: Handlers DON'T publish events directly. Events are declared
 * in YAML templates, and Rust orchestration publishes them after step completion.
 * Custom publishers only transform payloads. Subscribers only receive fast events.
 *
 * @module handler/domain-events
 * @see docs/architecture/domain-events.md
 * @see TAS-122, TAS-112 Phase 7
 */

import pino, { type Logger, type LoggerOptions } from 'pino';
import type { TaskerRuntime } from '../ffi/runtime-interface.js';
import type { FfiDomainEvent } from '../ffi/types.js';

// ---------------------------------------------------------------------------
// Logger Setup
// ---------------------------------------------------------------------------

const loggerOptions: LoggerOptions = {
  name: 'domain-events',
  level: process.env.RUST_LOG ?? 'info',
};

if (process.env.TASKER_ENV !== 'production') {
  loggerOptions.transport = {
    target: 'pino-pretty',
    options: { colorize: true },
  };
}

const log: Logger = pino(loggerOptions);

// ---------------------------------------------------------------------------
// Type Definitions
// ---------------------------------------------------------------------------

/**
 * Context passed to publishers for event transformation.
 *
 * Contains all information needed to build a business event payload.
 */
export interface StepEventContext {
  /** UUID of the task */
  readonly taskUuid: string;

  /** UUID of the step */
  readonly stepUuid: string;

  /** Name of the step handler */
  readonly stepName: string;

  /** Namespace (e.g., "payments", "inventory") */
  readonly namespace: string;

  /** Correlation ID for distributed tracing */
  readonly correlationId: string;

  /** Step result data (success payload or error info) */
  readonly result?: Record<string, unknown>;

  /** Execution metadata */
  readonly metadata: Record<string, unknown>;
}

/**
 * Event declaration from YAML task template.
 */
export interface EventDeclaration {
  /** Event name (e.g., "payment.processed") */
  readonly name: string;

  /** Publication condition: "success" | "failure" | "always" */
  readonly condition?: 'success' | 'failure' | 'retryable_failure' | 'permanent_failure' | 'always';

  /** Delivery mode: "durable" (PGMQ) | "fast" (in-process) | "broadcast" (both) */
  readonly deliveryMode?: 'durable' | 'fast' | 'broadcast';

  /** Custom publisher name (optional) */
  readonly publisher?: string;

  /** JSON schema for payload validation (optional) */
  readonly schema?: Record<string, unknown>;
}

/**
 * Step result passed to publishers.
 */
export interface StepResult {
  /** Whether the step succeeded */
  readonly success: boolean;

  /** Step handler's return value */
  readonly result?: Record<string, unknown>;

  /** Execution metadata */
  readonly metadata?: Record<string, unknown>;
}

/**
 * Domain event structure for subscribers.
 *
 * This is the shape of events received by BaseSubscriber.handle().
 */
export interface DomainEvent {
  /** Unique event ID (UUID v7) */
  readonly eventId: string;

  /** Event name (e.g., "payment.processed") */
  readonly eventName: string;

  /** Business payload from publisher transformation */
  readonly payload: Record<string, unknown>;

  /** Event metadata */
  readonly metadata: DomainEventMetadata;

  /** Original step execution result */
  readonly executionResult: StepResult;
}

/**
 * Metadata attached to every domain event.
 */
export interface DomainEventMetadata {
  /** Task UUID */
  readonly taskUuid: string;

  /** Step UUID */
  readonly stepUuid: string;

  /** Step name */
  readonly stepName?: string;

  /** Namespace */
  readonly namespace: string;

  /** Correlation ID for tracing */
  readonly correlationId: string;

  /** When the event was fired (ISO8601) */
  readonly firedAt: string;

  /** Publisher that created this event */
  readonly publisher?: string;

  /** Additional custom metadata */
  readonly [key: string]: unknown;
}

/**
 * Context passed to the publish() method.
 *
 * Cross-language standard API (TAS-96).
 */
export interface PublishContext {
  /** Event name */
  readonly eventName: string;

  /** Step result */
  readonly stepResult: StepResult;

  /** Event declaration from YAML */
  readonly eventDeclaration?: EventDeclaration;

  /** Step execution context */
  readonly stepContext?: StepEventContext;
}

// ---------------------------------------------------------------------------
// BasePublisher
// ---------------------------------------------------------------------------

/**
 * Abstract base class for custom domain event publishers.
 *
 * Publishers transform step execution results into business events.
 * They are declared in YAML via the `publisher:` field and registered
 * at bootstrap time.
 *
 * NOTE: Publishers don't call publish APIs directly. They transform data
 * that Rust orchestration publishes.
 *
 * @example
 * ```typescript
 * class PaymentEventPublisher extends BasePublisher {
 *   readonly publisherName = 'PaymentEventPublisher';
 *
 *   transformPayload(stepResult: StepResult, eventDecl?: EventDeclaration): Record<string, unknown> {
 *     const result = stepResult.result ?? {};
 *     return {
 *       transactionId: result.transaction_id,
 *       amount: result.amount,
 *       currency: result.currency ?? 'USD',
 *       status: stepResult.success ? 'succeeded' : 'failed',
 *     };
 *   }
 *
 *   shouldPublish(stepResult: StepResult): boolean {
 *     return stepResult.success && stepResult.result?.transaction_id != null;
 *   }
 * }
 * ```
 */
export abstract class BasePublisher {
  protected readonly logger: Logger = log;

  /**
   * Publisher name for registry lookup.
   * Must match the `publisher:` field in YAML.
   */
  abstract readonly publisherName: string;

  /**
   * Transform step result into business event payload.
   *
   * Override to customize the event payload for your domain.
   * Default returns the step result as-is.
   *
   * @param stepResult - The step execution result
   * @param eventDeclaration - The event declaration from YAML
   * @param stepContext - The step execution context
   * @returns Business event payload to publish
   */
  transformPayload(
    stepResult: StepResult,
    _eventDeclaration?: EventDeclaration,
    _stepContext?: StepEventContext
  ): Record<string, unknown> {
    return stepResult.result ?? {};
  }

  /**
   * Determine if this event should be published.
   *
   * Override to add custom conditions beyond YAML's `condition:` field.
   * The YAML condition is evaluated first by Rust orchestration.
   *
   * @param stepResult - The step execution result
   * @param eventDeclaration - The event declaration from YAML
   * @param stepContext - The step execution context
   * @returns true if the event should be published
   */
  shouldPublish(
    _stepResult: StepResult,
    _eventDeclaration?: EventDeclaration,
    _stepContext?: StepEventContext
  ): boolean {
    return true;
  }

  /**
   * Add additional metadata to the event.
   *
   * Override to add custom metadata fields.
   *
   * @param stepResult - The step execution result
   * @param eventDeclaration - The event declaration from YAML
   * @param stepContext - The step execution context
   * @returns Additional metadata to merge into event metadata
   */
  additionalMetadata(
    _stepResult: StepResult,
    _eventDeclaration?: EventDeclaration,
    _stepContext?: StepEventContext
  ): Record<string, unknown> {
    return {};
  }

  /**
   * Hook called before publishing.
   *
   * Override for pre-publish validation, logging, or metrics.
   * Return false to prevent publishing.
   *
   * @param eventName - The event name
   * @param payload - The transformed payload
   * @param metadata - The event metadata
   * @returns true to continue, false to skip publishing
   */
  beforePublish(
    eventName: string,
    _payload: Record<string, unknown>,
    _metadata: Record<string, unknown>
  ): boolean {
    this.logger.debug({ eventName }, 'Publishing event');
    return true;
  }

  /**
   * Hook called after successful publishing.
   *
   * Override for post-publish logging, metrics, or cleanup.
   *
   * @param eventName - The event name
   * @param payload - The transformed payload
   * @param metadata - The event metadata
   */
  afterPublish(
    eventName: string,
    _payload: Record<string, unknown>,
    _metadata: Record<string, unknown>
  ): void {
    this.logger.debug({ eventName }, 'Event published');
  }

  /**
   * Hook called if publishing fails.
   *
   * Override for error handling, logging, or fallback behavior.
   *
   * @param eventName - The event name
   * @param error - The error that occurred
   * @param payload - The transformed payload
   */
  onPublishError(eventName: string, error: Error, _payload: Record<string, unknown>): void {
    this.logger.error({ eventName, error: error.message }, 'Failed to publish event');
  }

  /**
   * Cross-language standard: Publish an event with unified context.
   *
   * Coordinates the lifecycle hooks into a single publish call.
   *
   * @param ctx - Publish context containing all required data
   * @returns true if event was published, false if skipped
   */
  publish(ctx: PublishContext): boolean {
    const { eventName, stepResult, eventDeclaration, stepContext } = ctx;

    // Check publishing conditions
    if (!this.shouldPublish(stepResult, eventDeclaration, stepContext)) {
      return false;
    }

    // Transform payload
    const payload = this.transformPayload(stepResult, eventDeclaration, stepContext);

    // Build metadata
    const baseMetadata: Record<string, unknown> = {
      publisher: this.publisherName,
      publishedAt: new Date().toISOString(),
    };

    // Add step context info if available
    if (stepContext) {
      baseMetadata.taskUuid = stepContext.taskUuid;
      baseMetadata.stepUuid = stepContext.stepUuid;
      baseMetadata.stepName = stepContext.stepName;
      baseMetadata.namespace = stepContext.namespace;
    }

    const metadata = {
      ...baseMetadata,
      ...this.additionalMetadata(stepResult, eventDeclaration, stepContext),
    };

    try {
      // Pre-publish hook (can abort)
      if (!this.beforePublish(eventName, payload, metadata)) {
        return false;
      }

      // Actual publishing is handled by Rust orchestration
      // This method just prepares and validates the event

      // Post-publish hook
      this.afterPublish(eventName, payload, metadata);

      return true;
    } catch (error) {
      this.onPublishError(
        eventName,
        error instanceof Error ? error : new Error(String(error)),
        payload
      );
      return false;
    }
  }
}

/**
 * Default publisher that passes step result through unchanged.
 */
export class DefaultPublisher extends BasePublisher {
  readonly publisherName = 'default';

  transformPayload(stepResult: StepResult): Record<string, unknown> {
    return stepResult.result ?? {};
  }
}

// ---------------------------------------------------------------------------
// BaseSubscriber
// ---------------------------------------------------------------------------

/**
 * Static interface for subscriber classes (for type checking).
 */
export interface SubscriberClass {
  /** Patterns to subscribe to */
  subscribesTo(): string[];

  /** Constructor */
  new (): BaseSubscriber;
}

/**
 * Abstract base class for domain event subscribers.
 *
 * Subscribers handle fast (in-process) domain events for internal processing.
 * They subscribe to event patterns and receive events via the InProcessEventBus.
 *
 * @example
 * ```typescript
 * class MetricsSubscriber extends BaseSubscriber {
 *   static subscribesTo(): string[] {
 *     return ['*']; // All events
 *   }
 *
 *   handle(event: DomainEvent): void {
 *     metrics.increment(`domain_events.${event.eventName.replace('.', '_')}`);
 *   }
 * }
 *
 * class PaymentSubscriber extends BaseSubscriber {
 *   static subscribesTo(): string[] {
 *     return ['payment.*']; // Only payment events
 *   }
 *
 *   handle(event: DomainEvent): void {
 *     if (event.eventName === 'payment.processed') {
 *       notifyAccounting(event.payload);
 *     }
 *   }
 * }
 * ```
 */
export abstract class BaseSubscriber {
  protected readonly logger: Logger = log;
  private _active = false;
  private _subscriptions: string[] = [];

  /**
   * Event patterns to subscribe to.
   *
   * Override this static method to declare patterns.
   * Supports wildcards: "*" matches all, "payment.*" matches payment.processed, etc.
   *
   * @returns Array of event patterns
   */
  static subscribesTo(): string[] {
    return ['*'];
  }

  /**
   * Check if the subscriber is active.
   */
  get active(): boolean {
    return this._active;
  }

  /**
   * Get subscribed patterns.
   */
  get subscriptions(): readonly string[] {
    return this._subscriptions;
  }

  /**
   * Handle a domain event.
   *
   * Subclasses MUST implement this method.
   *
   * @param event - The domain event to handle
   */
  abstract handle(event: DomainEvent): void | Promise<void>;

  /**
   * Hook called before handling an event.
   *
   * Override for pre-processing, validation, or filtering.
   * Return false to skip handling this event.
   *
   * @param event - The domain event
   * @returns true to continue handling, false to skip
   */
  beforeHandle(_event: DomainEvent): boolean {
    return true;
  }

  /**
   * Hook called after successful handling.
   *
   * Override for post-processing, metrics, or cleanup.
   *
   * @param event - The domain event
   */
  afterHandle(_event: DomainEvent): void {
    // Default: no-op
  }

  /**
   * Hook called if handling fails.
   *
   * Override for custom error handling, alerts, or retry logic.
   * Note: Domain events use fire-and-forget semantics.
   *
   * @param event - The domain event
   * @param error - The error that occurred
   */
  onHandleError(event: DomainEvent, error: Error): void {
    this.logger.error(
      { eventName: event.eventName, error: error.message },
      `${this.constructor.name}: Failed to handle event`
    );
  }

  /**
   * Check if this subscriber matches an event name.
   *
   * Supports wildcard patterns:
   * - "*" matches everything
   * - "payment.*" matches "payment.processed", "payment.failed"
   * - "order.created" matches exactly "order.created"
   *
   * @param eventName - The event name to check
   * @returns true if this subscriber should handle the event
   */
  matches(eventName: string): boolean {
    const ctor = this.constructor as typeof BaseSubscriber;
    const patterns = ctor.subscribesTo();

    return patterns.some((pattern) => {
      if (pattern === '*') {
        return true;
      }
      // Convert glob pattern to regex
      const regex = new RegExp(`^${pattern.replace(/\*/g, '.*')}$`);
      return regex.test(eventName);
    });
  }

  /**
   * Start listening for events.
   *
   * Registers with the InProcessDomainEventPoller.
   *
   * @param poller - The event poller to register with
   */
  start(poller: InProcessDomainEventPoller): void {
    if (this._active) {
      return;
    }

    this._active = true;
    const ctor = this.constructor as typeof BaseSubscriber;
    const patterns = ctor.subscribesTo();

    for (const pattern of patterns) {
      poller.subscribe(pattern, (event) => this.handleEventSafely(event));
      this._subscriptions.push(pattern);
      this.logger.info({ pattern, subscriber: this.constructor.name }, 'Subscribed to pattern');
    }

    this.logger.info(
      { subscriber: this.constructor.name, subscriptionCount: this._subscriptions.length },
      'Subscriber started'
    );
  }

  /**
   * Stop listening for events.
   */
  stop(): void {
    if (!this._active) {
      return;
    }

    this._active = false;
    this._subscriptions = [];
    this.logger.info({ subscriber: this.constructor.name }, 'Subscriber stopped');
  }

  /**
   * Safely handle an event with error capture.
   */
  private async handleEventSafely(event: DomainEvent): Promise<void> {
    if (!this._active) {
      return;
    }

    if (!this.beforeHandle(event)) {
      return;
    }

    try {
      await this.handle(event);
      this.afterHandle(event);
    } catch (error) {
      this.onHandleError(event, error instanceof Error ? error : new Error(String(error)));
    }
  }
}

// ---------------------------------------------------------------------------
// PublisherRegistry
// ---------------------------------------------------------------------------

/**
 * Error thrown when a required publisher is not registered.
 */
export class PublisherNotFoundError extends Error {
  readonly publisherName: string;
  readonly registeredPublishers: string[];

  constructor(name: string, registered: string[]) {
    super(`Publisher '${name}' not found. Registered: ${registered.join(', ')}`);
    this.name = 'PublisherNotFoundError';
    this.publisherName = name;
    this.registeredPublishers = registered;
  }
}

/**
 * Error thrown when required publishers are missing during validation.
 */
export class PublisherValidationError extends Error {
  readonly missingPublishers: string[];
  readonly registeredPublishers: string[];

  constructor(missing: string[], registered: string[]) {
    super(`Missing publishers: ${missing.join(', ')}. Registered: ${registered.join(', ')}`);
    this.name = 'PublisherValidationError';
    this.missingPublishers = missing;
    this.registeredPublishers = registered;
  }
}

/**
 * Error thrown when modifying a frozen registry.
 */
export class RegistryFrozenError extends Error {
  constructor() {
    super('Registry is frozen after validation');
    this.name = 'RegistryFrozenError';
  }
}

/**
 * Error thrown when registering a duplicate publisher.
 */
export class DuplicatePublisherError extends Error {
  readonly publisherName: string;

  constructor(name: string) {
    super(`Publisher '${name}' is already registered`);
    this.name = 'DuplicatePublisherError';
    this.publisherName = name;
  }
}

/**
 * Registry for custom domain event publishers.
 *
 * Maps publisher names (from YAML configuration) to their implementations.
 * Publishers are registered at bootstrap time and validated against task templates.
 *
 * @example
 * ```typescript
 * const registry = PublisherRegistry.instance;
 *
 * // Register custom publishers
 * registry.register(new PaymentEventPublisher());
 * registry.register(new InventoryEventPublisher());
 *
 * // Validate against templates
 * registry.validateRequired(['PaymentEventPublisher', 'InventoryEventPublisher']);
 *
 * // Look up publishers
 * const publisher = registry.get('PaymentEventPublisher');
 * ```
 */
export class PublisherRegistry {
  private static _instance: PublisherRegistry | null = null;

  private readonly publishers = new Map<string, BasePublisher>();
  private readonly defaultPublisher = new DefaultPublisher();
  private frozen = false;
  private readonly logger: Logger = log;

  private constructor() {
    // Private constructor for singleton
  }

  /**
   * Get the singleton instance.
   */
  static get instance(): PublisherRegistry {
    if (!PublisherRegistry._instance) {
      PublisherRegistry._instance = new PublisherRegistry();
    }
    return PublisherRegistry._instance;
  }

  /**
   * Register a custom publisher.
   *
   * @param publisher - The publisher instance to register
   * @throws RegistryFrozenError if registry is frozen
   * @throws DuplicatePublisherError if name is already registered
   */
  register(publisher: BasePublisher): void {
    if (this.frozen) {
      throw new RegistryFrozenError();
    }

    const name = publisher.publisherName;

    if (this.publishers.has(name)) {
      throw new DuplicatePublisherError(name);
    }

    this.logger.info({ publisherName: name }, 'Registering domain event publisher');
    this.publishers.set(name, publisher);
  }

  /**
   * Get a publisher by name.
   *
   * @param name - The publisher name
   * @returns The publisher, or undefined if not found
   */
  get(name: string): BasePublisher | undefined {
    return this.publishers.get(name);
  }

  /**
   * Get a publisher by name, or return the default.
   *
   * @param name - The publisher name (optional)
   * @returns The publisher or default
   */
  getOrDefault(name?: string): BasePublisher {
    if (!name || name === 'default') {
      return this.defaultPublisher;
    }

    const publisher = this.publishers.get(name);
    if (!publisher) {
      this.logger.warn({ publisherName: name }, 'Publisher not found, using default');
      return this.defaultPublisher;
    }

    return publisher;
  }

  /**
   * Get a publisher by name (strict mode).
   *
   * @param name - The publisher name
   * @returns The publisher
   * @throws PublisherNotFoundError if not registered
   */
  getStrict(name: string): BasePublisher {
    if (name === 'default') {
      return this.defaultPublisher;
    }

    const publisher = this.publishers.get(name);
    if (!publisher) {
      throw new PublisherNotFoundError(name, this.registeredNames);
    }

    return publisher;
  }

  /**
   * Check if a publisher is registered.
   */
  isRegistered(name: string): boolean {
    return name === 'default' || this.publishers.has(name);
  }

  /**
   * Get all registered publisher names.
   */
  get registeredNames(): string[] {
    return Array.from(this.publishers.keys());
  }

  /**
   * Get count of registered publishers.
   */
  get count(): number {
    return this.publishers.size;
  }

  /**
   * Check if registry is empty.
   */
  get isEmpty(): boolean {
    return this.publishers.size === 0;
  }

  /**
   * Check if registry is frozen.
   */
  get isFrozen(): boolean {
    return this.frozen;
  }

  /**
   * Unregister a publisher.
   *
   * @throws RegistryFrozenError if registry is frozen
   */
  unregister(name: string): boolean {
    if (this.frozen) {
      throw new RegistryFrozenError();
    }

    this.logger.info({ publisherName: name }, 'Unregistering domain event publisher');
    return this.publishers.delete(name);
  }

  /**
   * Clear all registered publishers.
   *
   * @throws RegistryFrozenError if registry is frozen
   */
  clear(): void {
    if (this.frozen) {
      throw new RegistryFrozenError();
    }

    this.logger.info('Clearing all domain event publishers');
    this.publishers.clear();
  }

  /**
   * Validate that all required publishers are registered.
   *
   * After validation, the registry is frozen.
   *
   * @param requiredPublishers - Publisher names from YAML configs
   * @throws PublisherValidationError if some publishers are missing
   */
  validateRequired(requiredPublishers: string[]): void {
    const missing: string[] = [];

    for (const name of requiredPublishers) {
      if (name === 'default') {
        continue;
      }
      if (!this.isRegistered(name)) {
        missing.push(name);
      }
    }

    if (missing.length > 0) {
      throw new PublisherValidationError(missing, this.registeredNames);
    }

    this.frozen = true;
    this.logger.info({ registeredPublishers: this.registeredNames }, 'Publisher validation passed');
  }

  /**
   * Freeze the registry to prevent further changes.
   */
  freeze(): void {
    this.frozen = true;
    this.logger.info('Publisher registry frozen');
  }

  /**
   * Reset the registry (for testing).
   */
  reset(): void {
    this.publishers.clear();
    this.frozen = false;
    this.logger.info('Publisher registry reset');
  }
}

// ---------------------------------------------------------------------------
// SubscriberRegistry
// ---------------------------------------------------------------------------

/**
 * Subscriber statistics.
 */
export interface SubscriberStats {
  /** Whether subscribers have been started */
  readonly started: boolean;

  /** Total subscriber count */
  readonly subscriberCount: number;

  /** Number of active subscribers */
  readonly activeCount: number;

  /** Individual subscriber info */
  readonly subscribers: ReadonlyArray<{
    readonly className: string;
    readonly active: boolean;
    readonly patterns: readonly string[];
  }>;
}

/**
 * Registry for domain event subscribers.
 *
 * Manages the lifecycle of subscribers. Subscribers are registered at bootstrap
 * time and started/stopped together with the worker.
 *
 * @example
 * ```typescript
 * const registry = SubscriberRegistry.instance;
 *
 * // Register subscriber classes (instantiation deferred)
 * registry.register(PaymentSubscriber);
 * registry.register(MetricsSubscriber);
 *
 * // Start all subscribers with the poller
 * registry.startAll(poller);
 *
 * // Stop all when shutting down
 * registry.stopAll();
 * ```
 */
export class SubscriberRegistry {
  private static _instance: SubscriberRegistry | null = null;

  private readonly subscriberClasses: SubscriberClass[] = [];
  private readonly subscribers: BaseSubscriber[] = [];
  private started = false;
  private readonly logger: Logger = log;

  private constructor() {
    // Private constructor for singleton
  }

  /**
   * Get the singleton instance.
   */
  static get instance(): SubscriberRegistry {
    if (!SubscriberRegistry._instance) {
      SubscriberRegistry._instance = new SubscriberRegistry();
    }
    return SubscriberRegistry._instance;
  }

  /**
   * Register a subscriber class.
   *
   * The class will be instantiated when startAll() is called.
   *
   * @param subscriberClass - A subclass of BaseSubscriber
   */
  register(subscriberClass: SubscriberClass): void {
    this.logger.info(
      { subscriberClass: subscriberClass.name },
      'SubscriberRegistry: Registered subscriber class'
    );
    this.subscriberClasses.push(subscriberClass);
  }

  /**
   * Register a subscriber instance directly.
   *
   * Use this when your subscriber needs custom initialization.
   *
   * @param subscriber - A subscriber instance
   */
  registerInstance(subscriber: BaseSubscriber): void {
    this.logger.info(
      { subscriberClass: subscriber.constructor.name },
      'SubscriberRegistry: Registered subscriber instance'
    );
    this.subscribers.push(subscriber);
  }

  /**
   * Start all registered subscribers.
   *
   * @param poller - The event poller to register subscribers with
   */
  startAll(poller: InProcessDomainEventPoller): void {
    if (this.started) {
      return;
    }

    // Instantiate registered classes
    for (const SubscriberClass of this.subscriberClasses) {
      try {
        const instance = new SubscriberClass();
        this.subscribers.push(instance);
      } catch (error) {
        this.logger.error(
          { subscriberClass: SubscriberClass.name, error: String(error) },
          'Failed to instantiate subscriber'
        );
      }
    }

    // Start all subscribers
    for (const subscriber of this.subscribers) {
      try {
        subscriber.start(poller);
      } catch (error) {
        this.logger.error(
          { subscriberClass: subscriber.constructor.name, error: String(error) },
          'Failed to start subscriber'
        );
      }
    }

    this.started = true;
    this.logger.info(
      { subscriberCount: this.subscribers.length },
      'SubscriberRegistry: Started all subscribers'
    );
  }

  /**
   * Stop all subscribers.
   */
  stopAll(): void {
    if (!this.started) {
      return;
    }

    for (const subscriber of this.subscribers) {
      try {
        subscriber.stop();
      } catch (error) {
        this.logger.error(
          { subscriberClass: subscriber.constructor.name, error: String(error) },
          'Failed to stop subscriber'
        );
      }
    }

    this.started = false;
    this.logger.info('SubscriberRegistry: Stopped all subscribers');
  }

  /**
   * Check if subscribers have been started.
   */
  get isStarted(): boolean {
    return this.started;
  }

  /**
   * Get count of registered subscribers (classes + instances).
   */
  get count(): number {
    return this.subscriberClasses.length + this.subscribers.length;
  }

  /**
   * Get subscriber statistics.
   */
  get stats(): SubscriberStats {
    return {
      started: this.started,
      subscriberCount: this.subscribers.length,
      activeCount: this.subscribers.filter((s) => s.active).length,
      subscribers: this.subscribers.map((s) => ({
        className: s.constructor.name,
        active: s.active,
        patterns: (s.constructor as typeof BaseSubscriber).subscribesTo(),
      })),
    };
  }

  /**
   * Reset the registry (for testing).
   */
  reset(): void {
    this.stopAll();
    this.subscribers.length = 0;
    this.subscriberClasses.length = 0;
    this.started = false;
    this.logger.info('SubscriberRegistry: Reset');
  }
}

// ---------------------------------------------------------------------------
// InProcessDomainEventPoller
// ---------------------------------------------------------------------------

/**
 * Callback type for domain event handlers.
 */
export type DomainEventCallback = (event: DomainEvent) => void | Promise<void>;

/**
 * Error callback type for domain event processing.
 */
export type DomainEventErrorCallback = (error: Error) => void;

/**
 * Poller statistics.
 */
export interface PollerStats {
  /** Total poll cycles */
  readonly pollCount: number;

  /** Total events processed */
  readonly eventsProcessed: number;

  /** Events that lagged (couldn't keep up) */
  readonly eventsLagged: number;

  /** Whether the poller is running */
  readonly running: boolean;
}

/**
 * Poller configuration.
 */
export interface DomainEventPollerConfig {
  /** Polling interval in milliseconds (default: 10) */
  readonly pollIntervalMs?: number;

  /** Maximum events per poll cycle (default: 100) */
  readonly maxEventsPerPoll?: number;
}

/**
 * In-process domain event poller.
 *
 * Polls for fast domain events from the Rust broadcast channel and
 * dispatches them to registered subscribers.
 *
 * Threading Model:
 * - Main Thread: TypeScript application, event handlers
 * - Poll Loop: Async timer-based polling
 * - Rust: Rust worker runtime (separate from TypeScript)
 *
 * @example
 * ```typescript
 * const poller = new InProcessDomainEventPoller({
 *   pollIntervalMs: 10,
 *   maxEventsPerPoll: 100,
 * });
 *
 * // Subscribe to patterns
 * poller.subscribe('payment.*', (event) => {
 *   console.log('Payment event:', event.eventName);
 * });
 *
 * // Start polling
 * poller.start();
 *
 * // Stop when done
 * poller.stop();
 * ```
 */
export class InProcessDomainEventPoller {
  private readonly pollIntervalMs: number;
  private readonly maxEventsPerPoll: number;
  private running = false;
  private pollTimer: ReturnType<typeof setTimeout> | null = null;
  private readonly logger: Logger = log;

  // Callbacks
  private readonly subscriptions = new Map<string, DomainEventCallback[]>();
  private readonly errorCallbacks: DomainEventErrorCallback[] = [];

  // Stats
  private pollCount = 0;
  private eventsProcessed = 0;
  private eventsLagged = 0;

  // FFI function reference (set when FFI layer is available)
  private pollFn: (() => DomainEvent | null) | null = null;

  constructor(config: DomainEventPollerConfig = {}) {
    this.pollIntervalMs = config.pollIntervalMs ?? 10;
    this.maxEventsPerPoll = config.maxEventsPerPoll ?? 100;
  }

  /**
   * Set the FFI poll function.
   *
   * This must be called before start() to enable actual polling.
   * Without it, the poller runs in "dry" mode (for testing).
   *
   * @param pollFn - Function that polls for events from Rust
   */
  setPollFunction(pollFn: () => DomainEvent | null): void {
    this.pollFn = pollFn;
  }

  /**
   * Subscribe to events matching a pattern.
   *
   * @param pattern - Event pattern (e.g., "*", "payment.*")
   * @param callback - Function to call when events match
   */
  subscribe(pattern: string, callback: DomainEventCallback): void {
    const callbacks = this.subscriptions.get(pattern) ?? [];
    callbacks.push(callback);
    this.subscriptions.set(pattern, callbacks);

    this.logger.debug({ pattern }, 'Subscribed to pattern');
  }

  /**
   * Unsubscribe from a pattern.
   *
   * @param pattern - The pattern to unsubscribe from
   */
  unsubscribe(pattern: string): void {
    this.subscriptions.delete(pattern);
    this.logger.debug({ pattern }, 'Unsubscribed from pattern');
  }

  /**
   * Register an error callback.
   */
  onError(callback: DomainEventErrorCallback): void {
    this.errorCallbacks.push(callback);
  }

  /**
   * Start the polling loop.
   */
  start(): void {
    if (this.running) {
      this.logger.warn('InProcessDomainEventPoller already running');
      return;
    }

    this.running = true;
    this.logger.info({ pollIntervalMs: this.pollIntervalMs }, 'InProcessDomainEventPoller started');

    this.schedulePoll();
  }

  /**
   * Stop the polling loop.
   */
  stop(): void {
    if (!this.running) {
      return;
    }

    this.running = false;

    if (this.pollTimer) {
      clearTimeout(this.pollTimer);
      this.pollTimer = null;
    }

    this.logger.info(
      { eventsProcessed: this.eventsProcessed, eventsLagged: this.eventsLagged },
      'InProcessDomainEventPoller stopped'
    );
  }

  /**
   * Check if the poller is running.
   */
  get isRunning(): boolean {
    return this.running;
  }

  /**
   * Get poller statistics.
   */
  get stats(): PollerStats {
    return {
      pollCount: this.pollCount,
      eventsProcessed: this.eventsProcessed,
      eventsLagged: this.eventsLagged,
      running: this.running,
    };
  }

  /**
   * Schedule the next poll cycle.
   */
  private schedulePoll(): void {
    if (!this.running) {
      return;
    }

    this.pollTimer = setTimeout(() => {
      this.pollCycle().catch((error) => {
        this.emitError(error instanceof Error ? error : new Error(String(error)));
      });
    }, this.pollIntervalMs);
  }

  /**
   * Execute a poll cycle.
   */
  private async pollCycle(): Promise<void> {
    if (!this.running) {
      return;
    }

    this.pollCount++;
    let eventsThisCycle = 0;

    try {
      // Poll for events (up to max per cycle)
      while (eventsThisCycle < this.maxEventsPerPoll) {
        const event = this.pollFn?.();

        if (!event) {
          break; // No more events
        }

        await this.dispatchEvent(event);
        eventsThisCycle++;
        this.eventsProcessed++;
      }

      // Check for lag (if we hit max, there might be more)
      if (eventsThisCycle >= this.maxEventsPerPoll) {
        this.eventsLagged++;
        this.logger.warn(
          { maxEventsPerPoll: this.maxEventsPerPoll },
          'Event poller may be lagging'
        );
      }
    } catch (error) {
      this.emitError(error instanceof Error ? error : new Error(String(error)));
    }

    // Schedule next cycle
    this.schedulePoll();
  }

  /**
   * Dispatch an event to matching subscribers.
   */
  private async dispatchEvent(event: DomainEvent): Promise<void> {
    this.logger.debug(
      { eventId: event.eventId, eventName: event.eventName },
      'Dispatching domain event'
    );

    for (const [pattern, callbacks] of this.subscriptions) {
      if (this.matchesPattern(event.eventName, pattern)) {
        for (const callback of callbacks) {
          try {
            await callback(event);
          } catch (error) {
            this.logger.error(
              { eventName: event.eventName, pattern, error: String(error) },
              'Subscriber callback error'
            );
            this.emitError(error instanceof Error ? error : new Error(String(error)));
          }
        }
      }
    }
  }

  /**
   * Check if an event name matches a pattern.
   */
  private matchesPattern(eventName: string, pattern: string): boolean {
    if (pattern === '*') {
      return true;
    }
    const regex = new RegExp(`^${pattern.replace(/\*/g, '.*')}$`);
    return regex.test(eventName);
  }

  /**
   * Emit an error to registered callbacks.
   */
  private emitError(error: Error): void {
    for (const callback of this.errorCallbacks) {
      try {
        callback(error);
      } catch {
        // Ignore errors in error callbacks
      }
    }
  }
}

// ---------------------------------------------------------------------------
// Factory Functions
// ---------------------------------------------------------------------------

/**
 * Create a StepEventContext from step data.
 *
 * @param data - Raw step context data
 * @returns StepEventContext
 */
export function createStepEventContext(data: {
  taskUuid: string;
  stepUuid: string;
  stepName: string;
  namespace?: string;
  correlationId?: string;
  result?: Record<string, unknown>;
  metadata?: Record<string, unknown>;
}): StepEventContext {
  const base = {
    taskUuid: data.taskUuid,
    stepUuid: data.stepUuid,
    stepName: data.stepName,
    namespace: data.namespace ?? 'default',
    correlationId: data.correlationId ?? data.taskUuid,
    metadata: data.metadata ?? {},
  };

  // Handle optional result property correctly for exactOptionalPropertyTypes
  if (data.result !== undefined) {
    return { ...base, result: data.result };
  }

  return base;
}

/**
 * Create a DomainEvent from raw data.
 *
 * @param data - Raw event data
 * @returns DomainEvent
 */
export function createDomainEvent(data: {
  eventId: string;
  eventName: string;
  payload: Record<string, unknown>;
  metadata: Record<string, unknown>;
  executionResult: StepResult;
}): DomainEvent {
  return {
    eventId: data.eventId,
    eventName: data.eventName,
    payload: data.payload,
    metadata: data.metadata as DomainEventMetadata,
    executionResult: data.executionResult,
  };
}

// ---------------------------------------------------------------------------
// FFI Domain Event Adapter
// ---------------------------------------------------------------------------

/**
 * Transform an FfiDomainEvent to a DomainEvent.
 *
 * The FFI domain event has a simpler structure than the full DomainEvent.
 * This function converts between them, synthesizing missing fields.
 *
 * @param ffiEvent - Event from FFI poll_in_process_events
 * @returns DomainEvent suitable for subscribers
 */
export function ffiEventToDomainEvent(ffiEvent: FfiDomainEvent): DomainEvent {
  // Build metadata with conditional optional fields (exactOptionalPropertyTypes)
  const metadata: DomainEventMetadata = {
    taskUuid: ffiEvent.metadata.taskUuid,
    stepUuid: ffiEvent.metadata.stepUuid ?? '',
    namespace: ffiEvent.metadata.namespace,
    correlationId: ffiEvent.metadata.correlationId,
    firedAt: ffiEvent.metadata.firedAt,
    eventVersion: ffiEvent.eventVersion,
    // Conditionally spread optional fields
    ...(ffiEvent.metadata.stepName !== null ? { stepName: ffiEvent.metadata.stepName } : {}),
    ...(ffiEvent.metadata.firedBy !== null ? { publisher: ffiEvent.metadata.firedBy } : {}),
  };

  return {
    eventId: ffiEvent.eventId,
    eventName: ffiEvent.eventName,
    payload: ffiEvent.payload,
    metadata,
    // FFI events don't include execution result - synthesize a placeholder
    executionResult: {
      success: true,
      result: ffiEvent.payload,
    },
  };
}

/**
 * Create an FFI poll function adapter for InProcessDomainEventPoller.
 *
 * This function wraps the runtime's pollInProcessEvents() to return
 * DomainEvent objects that the poller expects.
 *
 * @param runtime - The FFI runtime instance
 * @returns A poll function that returns DomainEvent | null
 *
 * @example
 * ```typescript
 * const poller = new InProcessDomainEventPoller();
 * poller.setPollFunction(createFfiPollAdapter(runtime));
 * poller.start();
 * ```
 */
export function createFfiPollAdapter(runtime: TaskerRuntime): () => DomainEvent | null {
  return () => {
    const ffiEvent = runtime.pollInProcessEvents();
    if (ffiEvent === null) {
      return null;
    }
    return ffiEventToDomainEvent(ffiEvent);
  };
}
