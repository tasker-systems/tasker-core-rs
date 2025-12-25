/**
 * Event poller coherence tests.
 *
 * Verifies that the EventPoller manages its lifecycle correctly
 * and interacts properly with the runtime interface.
 */

import { beforeEach, describe, expect, it, mock } from 'bun:test';
import { TaskerEventEmitter } from '../../../src/events/event-emitter.js';
import {
  createEventPoller,
  EventPoller,
  type EventPollerConfig,
} from '../../../src/events/event-poller.js';
import type { TaskerRuntime } from '../../../src/ffi/runtime-interface.js';
import type { FfiStepEvent } from '../../../src/ffi/types.js';

describe('EventPoller', () => {
  let mockRuntime: MockRuntime;
  let emitter: TaskerEventEmitter;

  beforeEach(() => {
    mockRuntime = createMockRuntime();
    emitter = new TaskerEventEmitter();
  });

  describe('construction', () => {
    it('creates a poller with default configuration', () => {
      const poller = new EventPoller(mockRuntime, emitter);
      expect(poller).toBeInstanceOf(EventPoller);
      expect(poller.getState()).toBe('stopped');
    });

    it('creates a poller with custom configuration', () => {
      const config: EventPollerConfig = {
        pollIntervalMs: 20,
        starvationCheckInterval: 50,
        cleanupInterval: 500,
        metricsInterval: 50,
        maxEventsPerCycle: 50,
      };
      const poller = new EventPoller(mockRuntime, emitter, config);
      expect(poller).toBeInstanceOf(EventPoller);
    });

    it('uses the provided event emitter', () => {
      const poller = new EventPoller(mockRuntime, emitter);
      const startedHandler = mock(() => {});
      emitter.on('poller.started', startedHandler);

      mockRuntime._setLoaded(true);
      poller.start();
      poller.stop();

      expect(startedHandler).toHaveBeenCalled();
    });
  });

  describe('lifecycle', () => {
    beforeEach(() => {
      mockRuntime._setLoaded(true);
    });

    it('starts in stopped state', () => {
      const poller = new EventPoller(mockRuntime, emitter);
      expect(poller.getState()).toBe('stopped');
      expect(poller.isRunning()).toBe(false);
    });

    it('transitions to running state on start', () => {
      const poller = new EventPoller(mockRuntime, emitter);
      poller.start();

      expect(poller.getState()).toBe('running');
      expect(poller.isRunning()).toBe(true);

      poller.stop();
    });

    it('transitions back to stopped state on stop', async () => {
      const poller = new EventPoller(mockRuntime, emitter);
      poller.start();
      await poller.stop();

      expect(poller.getState()).toBe('stopped');
      expect(poller.isRunning()).toBe(false);
    });

    it('throws error when starting with unloaded runtime', () => {
      mockRuntime._setLoaded(false);
      const poller = new EventPoller(mockRuntime, emitter);

      expect(() => poller.start()).toThrow('Runtime not loaded');
    });

    it('is idempotent when starting multiple times', () => {
      const poller = new EventPoller(mockRuntime, emitter);
      poller.start();
      poller.start(); // Should not throw

      expect(poller.getState()).toBe('running');
      poller.stop();
    });

    it('is idempotent when stopping multiple times', async () => {
      const poller = new EventPoller(mockRuntime, emitter);
      poller.start();
      await poller.stop();
      await poller.stop(); // Should not throw

      expect(poller.getState()).toBe('stopped');
    });
  });

  describe('event emission', () => {
    beforeEach(() => {
      mockRuntime._setLoaded(true);
    });

    it('emits poller.started when starting', () => {
      const handler = mock(() => {});
      emitter.on('poller.started', handler);

      const poller = new EventPoller(mockRuntime, emitter);
      poller.start();
      poller.stop();

      expect(handler).toHaveBeenCalledTimes(1);
    });

    it('emits poller.stopped when stopping', async () => {
      const handler = mock(() => {});
      emitter.on('poller.stopped', handler);

      const poller = new EventPoller(mockRuntime, emitter);
      poller.start();
      await poller.stop();

      expect(handler).toHaveBeenCalledTimes(1);
    });
  });

  describe('callbacks', () => {
    beforeEach(() => {
      mockRuntime._setLoaded(true);
    });

    it('supports onStepEvent callback registration', () => {
      const poller = new EventPoller(mockRuntime, emitter);
      const callback = mock(async () => {});

      const result = poller.onStepEvent(callback);
      expect(result).toBe(poller); // Fluent API
    });

    it('supports onError callback registration', () => {
      const poller = new EventPoller(mockRuntime, emitter);
      const callback = mock(() => {});

      const result = poller.onError(callback);
      expect(result).toBe(poller); // Fluent API
    });

    it('supports onMetrics callback registration', () => {
      const poller = new EventPoller(mockRuntime, emitter);
      const callback = mock(() => {});

      const result = poller.onMetrics(callback);
      expect(result).toBe(poller); // Fluent API
    });

    it('supports fluent callback chaining', () => {
      const poller = new EventPoller(mockRuntime, emitter);

      const result = poller
        .onStepEvent(async () => {})
        .onError(() => {})
        .onMetrics(() => {});

      expect(result).toBe(poller);
    });
  });

  describe('polling counters', () => {
    beforeEach(() => {
      mockRuntime._setLoaded(true);
    });

    it('initializes poll count to 0', () => {
      const poller = new EventPoller(mockRuntime, emitter);
      expect(poller.getPollCount()).toBe(0);
    });

    it('initializes cycle count to 0', () => {
      const poller = new EventPoller(mockRuntime, emitter);
      expect(poller.getCycleCount()).toBe(0);
    });

    it('resets counters on start', async () => {
      const poller = new EventPoller(mockRuntime, emitter, {
        pollIntervalMs: 5,
      });

      poller.start();
      // Wait for a few poll cycles
      await sleep(25);
      await poller.stop();

      const countAfterFirstRun = poller.getPollCount();
      expect(countAfterFirstRun).toBeGreaterThan(0);

      // Start again - counters should reset
      poller.start();
      // Poll count resets immediately on start
      expect(poller.getPollCount()).toBe(0);
      await poller.stop();
    });
  });

  describe('createEventPoller factory', () => {
    it('creates an EventPoller instance', () => {
      const poller = createEventPoller(mockRuntime, emitter);
      expect(poller).toBeInstanceOf(EventPoller);
    });

    it('passes configuration to the poller', () => {
      const config: EventPollerConfig = {
        pollIntervalMs: 50,
      };
      const poller = createEventPoller(mockRuntime, emitter, config);
      expect(poller).toBeInstanceOf(EventPoller);
    });
  });
});

// Mock runtime implementation for testing

interface MockRuntime extends TaskerRuntime {
  _setLoaded(loaded: boolean): void;
  _setEvents(events: FfiStepEvent[]): void;
}

function createMockRuntime(): MockRuntime {
  let loaded = false;
  let events: FfiStepEvent[] = [];
  let eventIndex = 0;

  return {
    name: 'mock',

    get isLoaded() {
      return loaded;
    },

    _setLoaded(value: boolean) {
      loaded = value;
    },

    _setEvents(newEvents: FfiStepEvent[]) {
      events = newEvents;
      eventIndex = 0;
    },

    async load(_libraryPath: string) {
      loaded = true;
    },

    unload() {
      loaded = false;
    },

    getVersion() {
      return '1.0.0-mock';
    },

    getRustVersion() {
      return '1.0.0-mock';
    },

    healthCheck() {
      return true;
    },

    bootstrapWorker(_config) {
      return {
        success: true,
        status: 'started',
        message: 'Mock worker started',
      };
    },

    isWorkerRunning() {
      return true;
    },

    getWorkerStatus() {
      return { success: true, running: true };
    },

    stopWorker() {
      return {
        success: true,
        status: 'stopped',
        message: 'Mock worker stopped',
      };
    },

    transitionToGracefulShutdown() {
      return {
        success: true,
        status: 'stopped',
        message: 'Mock graceful shutdown',
      };
    },

    pollStepEvents() {
      if (eventIndex < events.length) {
        return events[eventIndex++];
      }
      return null;
    },

    completeStepEvent(_eventId, _result) {
      return true;
    },

    getFfiDispatchMetrics() {
      return {
        pending_count: events.length - eventIndex,
        starvation_detected: false,
        starving_event_count: 0,
        oldest_pending_age_ms: null,
        newest_pending_age_ms: null,
      };
    },

    checkStarvationWarnings() {},

    cleanupTimeouts() {},

    logError(_message, _fields) {},
    logWarn(_message, _fields) {},
    logInfo(_message, _fields) {},
    logDebug(_message, _fields) {},
    logTrace(_message, _fields) {},
  };
}

function sleep(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
