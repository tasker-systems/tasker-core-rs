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
// Handler registry
export { HandlerRegistry } from './registry';
