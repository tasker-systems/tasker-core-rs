/**
 * TAS-93 Phase 5: Resolver Tests Handlers
 *
 * Exports handlers for testing the resolver chain:
 * - MultiMethodHandler: Multi-method handler for method dispatch testing
 * - AlternateMethodHandler: Secondary handler for resolver chain testing
 *
 * @see docs/ticket-specs/TAS-93/implementation-plan.md
 */

export {
  AlternateMethodHandler,
  MultiMethodHandler,
} from './step_handlers/multi-method-handler.js';
