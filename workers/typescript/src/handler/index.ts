/**
 * Handler system for the tasker-core TypeScript worker.
 *
 * Provides base classes and registry for step handlers,
 * aligned with Python and Ruby worker implementations (TAS-92).
 *
 * @module handler
 */

// Base handler class
export { StepHandler } from './base';
export type { StepHandlerClass } from './base';

// Handler registry
export { HandlerRegistry } from './registry';
