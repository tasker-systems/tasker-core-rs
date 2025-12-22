/**
 * Bootstrap module for TypeScript workers.
 *
 * Provides worker lifecycle management including initialization,
 * status checks, and graceful shutdown.
 */

// Types
export type { BootstrapConfig, BootstrapResult, WorkerStatus, StopResult } from './types.js';

// Bootstrap API
export {
  bootstrapWorker,
  stopWorker,
  getWorkerStatus,
  transitionToGracefulShutdown,
  isWorkerRunning,
  getVersion,
  getRustVersion,
  healthCheck,
} from './bootstrap.js';
