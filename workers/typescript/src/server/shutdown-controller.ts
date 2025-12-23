/**
 * Shutdown controller for coordinating graceful shutdown.
 *
 * Provides a signal-based mechanism for triggering and awaiting
 * shutdown across async boundaries.
 */

import { createLogger } from '../logging/index.js';

const log = createLogger({ component: 'shutdown' });

/**
 * Shutdown signal handler type.
 */
export type ShutdownHandler = () => void | Promise<void>;

/**
 * Controller for coordinating graceful shutdown.
 *
 * Provides a promise-based mechanism for waiting on shutdown signals
 * and executing cleanup handlers in order.
 *
 * @example
 * ```typescript
 * const shutdown = new ShutdownController();
 *
 * process.on('SIGTERM', () => shutdown.trigger('SIGTERM'));
 * process.on('SIGINT', () => shutdown.trigger('SIGINT'));
 *
 * // Wait for shutdown signal
 * await shutdown.promise;
 *
 * // Or check if shutdown was requested
 * if (shutdown.isRequested) {
 *   await cleanup();
 * }
 * ```
 */
export class ShutdownController {
  private _shutdownRequested = false;
  private _resolver: (() => void) | null = null;
  private _signal: string | null = null;
  private readonly _handlers: ShutdownHandler[] = [];

  /**
   * Promise that resolves when shutdown is triggered.
   */
  readonly promise: Promise<void>;

  constructor() {
    this.promise = new Promise<void>((resolve) => {
      this._resolver = resolve;
    });
  }

  /**
   * Check if shutdown has been requested.
   */
  get isRequested(): boolean {
    return this._shutdownRequested;
  }

  /**
   * Get the signal that triggered shutdown, if any.
   */
  get signal(): string | null {
    return this._signal;
  }

  /**
   * Register a handler to be called during shutdown.
   *
   * Handlers are called in registration order.
   */
  onShutdown(handler: ShutdownHandler): void {
    this._handlers.push(handler);
  }

  /**
   * Trigger shutdown with the given signal.
   *
   * @param signal - The signal that triggered shutdown (e.g., 'SIGTERM', 'SIGINT')
   */
  trigger(signal: string): void {
    if (this._shutdownRequested) {
      log.warn(`Shutdown already requested, ignoring ${signal}`, {
        operation: 'shutdown',
        signal,
        original_signal: this._signal ?? 'unknown',
      });
      return;
    }

    log.info(`Received ${signal} signal, initiating shutdown...`, {
      operation: 'shutdown',
      signal,
    });

    this._shutdownRequested = true;
    this._signal = signal;
    this._resolver?.();
  }

  /**
   * Execute all registered shutdown handlers.
   *
   * Handlers are called in registration order. Errors are logged
   * but do not prevent subsequent handlers from running.
   */
  async executeHandlers(): Promise<void> {
    for (const handler of this._handlers) {
      try {
        await handler();
      } catch (error) {
        log.error(
          `Shutdown handler failed: ${error instanceof Error ? error.message : String(error)}`,
          {
            operation: 'shutdown',
            error_message: error instanceof Error ? error.message : String(error),
          }
        );
      }
    }
  }

  /**
   * Reset the controller for reuse (primarily for testing).
   */
  reset(): void {
    this._shutdownRequested = false;
    this._signal = null;
    this._handlers.length = 0;
  }
}
