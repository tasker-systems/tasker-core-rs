/**
 * FFI module for TypeScript/JavaScript workers.
 *
 * Provides runtime-specific FFI adapters for Bun, Node.js, and Deno.
 */

// Runtime implementations (for direct use if needed)
export { BunRuntime } from './bun-runtime.js';
export { DenoRuntime } from './deno-runtime.js';
// FfiLayer - primary API for FFI management
export { FfiLayer, type FfiLayerConfig } from './ffi-layer.js';
export { NodeRuntime } from './node-runtime.js';
// Runtime detection
export {
  detectRuntime,
  getLibraryPath,
  getRuntimeInfo,
  isBun,
  isDeno,
  isNode,
  type RuntimeInfo,
  type RuntimeType,
} from './runtime.js';
// Runtime interface
export { BaseTaskerRuntime, type TaskerRuntime } from './runtime-interface.js';

// FFI types
export type {
  BootstrapConfig,
  BootstrapResult,
  DependencyResult,
  FfiDispatchMetrics,
  FfiDomainEvent,
  FfiDomainEventMetadata,
  FfiStepEvent,
  HandlerDefinition,
  LogFields,
  OrchestrationMetadata,
  RetryConfiguration,
  StepDefinition,
  StepExecutionError,
  StepExecutionMetadata,
  StepExecutionResult,
  StopResult,
  Task,
  WorkerStatus,
  WorkflowStep,
} from './types.js';
