/**
 * Handler system for the tasker-core TypeScript worker.
 *
 * Provides base classes and registry for step handlers,
 * aligned with Python and Ruby worker implementations (TAS-92).
 *
 * @module handler
 */

// Specialized handlers (TAS-103)
export { ApiHandler, ApiResponse } from './api';
export type { StepHandlerClass } from './base';
// Base handler class
export { StepHandler } from './base';
export type { Batchable } from './batchable';
export { applyBatchable, BatchableMixin } from './batchable';
export type { DecisionPointOutcome } from './decision';
export { DecisionHandler, DecisionType } from './decision';
// Domain events (TAS-112/TAS-122)
export {
  // Abstract base classes
  BasePublisher,
  BaseSubscriber,
  createDomainEvent,
  // FFI adapter for domain event polling
  createFfiPollAdapter,
  // Factory functions
  createStepEventContext,
  DefaultPublisher,
  type DomainEvent,
  type DomainEventCallback,
  type DomainEventErrorCallback,
  type DomainEventMetadata,
  type DomainEventPollerConfig,
  DuplicatePublisherError,
  type EventDeclaration,
  // Transform FFI events to domain events
  ffiEventToDomainEvent,
  // Event poller for FFI integration
  InProcessDomainEventPoller,
  type PollerStats,
  type PublishContext,
  // Error classes
  PublisherNotFoundError,
  // Registries
  PublisherRegistry,
  PublisherValidationError,
  RegistryFrozenError,
  // Types
  type StepEventContext,
  type StepResult,
  type SubscriberClass,
  SubscriberRegistry,
  type SubscriberStats,
} from './domain-events';
// Handler registry
export { HandlerRegistry } from './registry';
