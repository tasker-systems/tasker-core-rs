/**
 * Tasker TypeScript Worker
 *
 * FFI-based worker for tasker-core supporting Bun, Node.js, and Deno runtimes.
 *
 * @packageDocumentation
 */

// =============================================================================
// Bootstrap module (TAS-104) - Worker lifecycle management
// =============================================================================
export {
  bootstrapWorker,
  getRustVersion,
  getVersion,
  getWorkerStatus,
  healthCheck,
  isWorkerRunning,
  stopWorker,
  transitionToGracefulShutdown,
} from './bootstrap/index.js';

// Export bootstrap types (user-friendly camelCase versions)
export type {
  BootstrapConfig,
  BootstrapResult,
  StopResult,
  WorkerStatus,
} from './bootstrap/types.js';

// =============================================================================
// Events module
// =============================================================================
export * from './events/index.js';

// =============================================================================
// FFI module - Runtime and low-level types
// =============================================================================
export {
  // Runtime detection
  detectRuntime,
  getLibraryPath,
  getRuntimeInfo,
  isBun,
  isDeno,
  isNode,
  type RuntimeInfo,
  type RuntimeType,
  // Runtime factory
  clearRuntimeCache,
  createRuntime,
  getCachedRuntime,
  getTaskerRuntime,
  hasRuntimeCached,
  // Runtime interface
  BaseTaskerRuntime,
  type TaskerRuntime,
  // Runtime implementations
  BunRuntime,
  DenoRuntime,
  NodeRuntime,
} from './ffi/index.js';

// Export FFI types under Ffi prefix to avoid conflicts
export type {
  BootstrapConfig as FfiBootstrapConfig,
  BootstrapResult as FfiBootstrapResult,
  DependencyResult,
  FfiDispatchMetrics,
  FfiStepEvent,
  HandlerDefinition,
  LogFields as FfiLogFields,
  OrchestrationMetadata,
  RetryConfiguration,
  StepDefinition,
  StepExecutionError,
  StepExecutionMetadata,
  StepExecutionResult,
  StopResult as FfiStopResult,
  Task,
  WorkerStatus as FfiWorkerStatus,
  WorkflowStep,
} from './ffi/types.js';

// =============================================================================
// Handler module (TAS-102/103)
// =============================================================================
export * from './handler/index.js';

// =============================================================================
// Logging module (TAS-104)
// =============================================================================
export {
  createLogger,
  logDebug,
  logError,
  logInfo,
  logTrace,
  logWarn,
  type LogFields,
} from './logging/index.js';

// =============================================================================
// Subscriber module (TAS-104)
// =============================================================================
export * from './subscriber/index.js';

// =============================================================================
// Types module (TAS-102)
// =============================================================================
export * from './types/index.js';
