/**
 * Handler system for the tasker-core TypeScript worker.
 *
 * Provides base classes and registry for step handlers,
 * aligned with Python and Ruby worker implementations (TAS-92).
 *
 * @module handler
 */

// Specialized handlers (TAS-103)
export { ApiHandler, ApiResponse } from './api.js';
export type { StepHandlerClass } from './base.js';
// Base handler class
export { StepHandler } from './base.js';
export type { Batchable } from './batchable.js';
export { applyBatchable, BatchableMixin } from './batchable.js';
export type { DecisionPointOutcome } from './decision.js';
export { DecisionHandler, DecisionType } from './decision.js';
// TAS-112: Composition pattern mixins
export type { APICapable, DecisionCapable } from './mixins/index.js';
export { APIMixin, applyAPI, applyDecision, DecisionMixin } from './mixins/index.js';
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
