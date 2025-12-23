/**
 * Server module.
 *
 * Provides the WorkerServer class for managing worker lifecycle
 * and the ShutdownController for coordinating graceful shutdown.
 */

export { ShutdownController, type ShutdownHandler } from './shutdown-controller.js';
export type {
  HealthCheckResult,
  ServerComponents,
  ServerState,
  ServerStatus,
  WorkerServerConfig,
} from './types.js';
export { WorkerServer } from './worker-server.js';
