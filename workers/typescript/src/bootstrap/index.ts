/**
 * Bootstrap module for TypeScript workers.
 *
 * Provides worker lifecycle management including initialization,
 * status checks, and graceful shutdown.
 */

// Bootstrap API
export {
  bootstrapWorker,
  getRustVersion,
  getVersion,
  getWorkerStatus,
  healthCheck,
  isWorkerRunning,
  stopWorker,
  transitionToGracefulShutdown,
} from './bootstrap.js';
// Types
export type { BootstrapConfig, BootstrapResult, StopResult, WorkerStatus } from './types.js';
