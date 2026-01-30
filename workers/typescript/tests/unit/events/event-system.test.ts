/**
 * EventSystem lifecycle tests.
 *
 * Tests EventSystem start/stop/isRunning/getEmitter/getStats using
 * mock runtime and mock registry. The EventSystem constructor creates
 * an emitter, poller, and subscriber that share the same emitter instance.
 */

import { afterEach, describe, expect, test } from 'bun:test';
import { EventSystem } from '../../../src/events/event-system.js';
import type { TaskerRuntime } from '../../../src/ffi/runtime-interface.js';
import type {
  BootstrapConfig,
  BootstrapResult,
  CheckpointYieldData,
  FfiDispatchMetrics,
  FfiDomainEvent,
  FfiStepEvent,
  LogFields,
  StepExecutionResult,
  StopResult,
  WorkerStatus,
} from '../../../src/ffi/types.js';
import type { HandlerRegistryInterface } from '../../../src/subscriber/step-execution-subscriber.js';

// =============================================================================
// Mock Runtime
// =============================================================================

class MockTaskerRuntime implements TaskerRuntime {
  readonly name = 'mock';
  private _isLoaded = true;

  get isLoaded(): boolean {
    return this._isLoaded;
  }

  _setLoaded(value: boolean): void {
    this._isLoaded = value;
  }

  async load(_libraryPath: string): Promise<void> {
    this._isLoaded = true;
  }

  unload(): void {
    this._isLoaded = false;
  }

  getVersion(): string {
    return '0.1.0-mock';
  }

  getRustVersion(): string {
    return '0.1.0-mock-rust';
  }

  healthCheck(): boolean {
    return true;
  }

  bootstrapWorker(_config?: BootstrapConfig): BootstrapResult {
    return { success: true, worker_id: 'mock-worker', message: 'mock bootstrap' };
  }

  isWorkerRunning(): boolean {
    return true;
  }

  getWorkerStatus(): WorkerStatus {
    return {
      running: true,
      worker_id: 'mock-worker',
      active_tasks: 0,
      uptime_seconds: 0,
    };
  }

  stopWorker(): StopResult {
    return { success: true, message: 'stopped' };
  }

  transitionToGracefulShutdown(): StopResult {
    return { success: true, message: 'transitioning' };
  }

  pollStepEvents(): FfiStepEvent | null {
    return null;
  }

  pollInProcessEvents(): FfiDomainEvent | null {
    return null;
  }

  completeStepEvent(_eventId: string, _result: StepExecutionResult): boolean {
    return true;
  }

  checkpointYieldStepEvent(_eventId: string, _checkpointData: CheckpointYieldData): boolean {
    return true;
  }

  getFfiDispatchMetrics(): FfiDispatchMetrics {
    return {
      total_dispatched: 0,
      total_completed: 0,
      total_errors: 0,
      pending_count: 0,
      events_per_second: 0,
    };
  }

  checkStarvationWarnings(): void {}
  cleanupTimeouts(): void {}

  logError(_message: string, _fields?: LogFields): void {}
  logWarn(_message: string, _fields?: LogFields): void {}
  logInfo(_message: string, _fields?: LogFields): void {}
  logDebug(_message: string, _fields?: LogFields): void {}
  logTrace(_message: string, _fields?: LogFields): void {}
}

// =============================================================================
// Mock Registry
// =============================================================================

class MockHandlerRegistry implements HandlerRegistryInterface {
  async resolve(_name: string) {
    return null;
  }
}

// =============================================================================
// Tests
// =============================================================================

describe('EventSystem', () => {
  let runtime: MockTaskerRuntime;
  let registry: MockHandlerRegistry;
  let eventSystem: EventSystem;

  afterEach(async () => {
    // Ensure cleanup even if test fails
    if (eventSystem?.isRunning()) {
      await eventSystem.stop();
    }
  });

  function createEventSystem(config = {}): EventSystem {
    runtime = new MockTaskerRuntime();
    registry = new MockHandlerRegistry();
    eventSystem = new EventSystem(runtime, registry, config);
    return eventSystem;
  }

  describe('constructor', () => {
    test('should create in stopped state', () => {
      const system = createEventSystem();

      expect(system.isRunning()).toBe(false);
    });

    test('should accept configuration', () => {
      const system = createEventSystem({
        poller: { pollIntervalMs: 50 },
        subscriber: { workerId: 'test-worker', maxConcurrent: 5 },
      });

      expect(system.isRunning()).toBe(false);
    });
  });

  describe('getEmitter', () => {
    test('should return the emitter instance', () => {
      const system = createEventSystem();
      const emitter = system.getEmitter();

      expect(emitter).toBeDefined();
      expect(typeof emitter.emit).toBe('function');
      expect(typeof emitter.on).toBe('function');
    });

    test('should return same emitter on multiple calls', () => {
      const system = createEventSystem();
      const emitter1 = system.getEmitter();
      const emitter2 = system.getEmitter();

      expect(emitter1).toBe(emitter2);
    });
  });

  describe('isRunning', () => {
    test('should be false before start', () => {
      const system = createEventSystem();
      expect(system.isRunning()).toBe(false);
    });

    test('should be true after start', () => {
      const system = createEventSystem();
      system.start();
      expect(system.isRunning()).toBe(true);
    });

    test('should be false after stop', async () => {
      const system = createEventSystem();
      system.start();
      await system.stop();
      expect(system.isRunning()).toBe(false);
    });
  });

  describe('start', () => {
    test('should set running to true', () => {
      const system = createEventSystem();
      system.start();

      expect(system.isRunning()).toBe(true);
    });

    test('should be idempotent (calling start when running)', () => {
      const system = createEventSystem();
      system.start();
      system.start(); // Should not throw

      expect(system.isRunning()).toBe(true);
    });
  });

  describe('stop', () => {
    test('should set running to false', async () => {
      const system = createEventSystem();
      system.start();
      expect(system.isRunning()).toBe(true);

      await system.stop();

      expect(system.isRunning()).toBe(false);
    });

    test('should be safe to call when not running', async () => {
      const system = createEventSystem();

      // Should not throw
      await system.stop();

      expect(system.isRunning()).toBe(false);
    });

    test('should be idempotent', async () => {
      const system = createEventSystem();
      system.start();

      await system.stop();
      await system.stop(); // Second stop should be safe

      expect(system.isRunning()).toBe(false);
    });
  });

  describe('getStats', () => {
    test('should return stats with running=false before start', () => {
      const system = createEventSystem();
      const stats = system.getStats();

      expect(stats.running).toBe(false);
      expect(stats.processedCount).toBe(0);
      expect(stats.errorCount).toBe(0);
      expect(stats.activeHandlers).toBe(0);
      expect(stats.pollCount).toBe(0);
    });

    test('should return stats with running=true after start', () => {
      const system = createEventSystem();
      system.start();
      const stats = system.getStats();

      expect(stats.running).toBe(true);
    });

    test('should return stats with running=false after stop', async () => {
      const system = createEventSystem();
      system.start();
      await system.stop();
      const stats = system.getStats();

      expect(stats.running).toBe(false);
    });
  });
});
