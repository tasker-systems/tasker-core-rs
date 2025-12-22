/**
 * FFI module for TypeScript/JavaScript workers.
 *
 * Provides runtime-specific FFI adapters for Bun, Node.js, and Deno.
 */

// Runtime detection
export {
  detectRuntime,
  isBun,
  isNode,
  isDeno,
  getRuntimeInfo,
  getLibraryPath,
  type RuntimeType,
  type RuntimeInfo,
} from './runtime.js';

// Runtime interface
export { type TaskerRuntime, BaseTaskerRuntime } from './runtime-interface.js';

// Runtime factory
export {
  getTaskerRuntime,
  createRuntime,
  clearRuntimeCache,
  hasRuntimeCached,
  getCachedRuntime,
} from './runtime-factory.js';

// Runtime implementations (for direct use if needed)
export { BunRuntime } from './bun-runtime.js';
export { NodeRuntime } from './node-runtime.js';
export { DenoRuntime } from './deno-runtime.js';

// FFI types
export type {
  Task,
  WorkflowStep,
  HandlerDefinition,
  RetryConfiguration,
  StepDefinition,
  StepExecutionError,
  DependencyResult,
  FfiStepEvent,
  BootstrapConfig,
  BootstrapResult,
  WorkerStatus,
  StopResult,
  FfiDispatchMetrics,
  StepExecutionResult,
  StepExecutionMetadata,
  OrchestrationMetadata,
  LogFields,
} from './types.js';
