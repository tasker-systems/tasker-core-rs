/**
 * HandlerSystem tests.
 *
 * Covers handler registration, unregistration, resolution delegation,
 * initialization, and file-based handler loading.
 */

import { afterEach, beforeEach, describe, expect, test } from 'bun:test';
import { mkdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { StepHandler } from '../../../src/handler/base.js';
import { HandlerSystem } from '../../../src/handler/handler-system.js';
import type { StepContext } from '../../../src/types/step-context.js';
import type { StepHandlerResult } from '../../../src/types/step-handler-result.js';

// =============================================================================
// Test Fixtures
// =============================================================================

class FixtureHandler extends StepHandler {
  static handlerName = 'fixture_handler';

  async call(_context: StepContext): Promise<StepHandlerResult> {
    return this.success({ processed: true });
  }
}

class AnotherHandler extends StepHandler {
  static handlerName = 'another_handler';

  async call(_context: StepContext): Promise<StepHandlerResult> {
    return this.success({ done: true });
  }
}

// =============================================================================
// Tests
// =============================================================================

describe('HandlerSystem', () => {
  let system: HandlerSystem;

  beforeEach(() => {
    system = new HandlerSystem();
  });

  describe('constructor', () => {
    test('should create with empty registry', () => {
      expect(system.handlerCount()).toBe(0);
      expect(system.listHandlers()).toEqual([]);
    });
  });

  describe('register / unregister / isRegistered', () => {
    test('should register a handler', () => {
      system.register('fixture_handler', FixtureHandler);

      expect(system.isRegistered('fixture_handler')).toBe(true);
      expect(system.handlerCount()).toBe(1);
    });

    test('should list registered handlers', () => {
      system.register('fixture_handler', FixtureHandler);
      system.register('another_handler', AnotherHandler);

      const handlers = system.listHandlers();

      expect(handlers).toContain('fixture_handler');
      expect(handlers).toContain('another_handler');
      expect(handlers).toHaveLength(2);
    });

    test('should unregister a handler', () => {
      system.register('fixture_handler', FixtureHandler);

      const removed = system.unregister('fixture_handler');

      expect(removed).toBe(true);
      expect(system.isRegistered('fixture_handler')).toBe(false);
      expect(system.handlerCount()).toBe(0);
    });

    test('should return false when unregistering non-existent handler', () => {
      const removed = system.unregister('nonexistent');

      expect(removed).toBe(false);
    });

    test('should report isRegistered=false for unknown handler', () => {
      expect(system.isRegistered('unknown')).toBe(false);
    });
  });

  describe('clear', () => {
    test('should clear all handlers', () => {
      system.register('fixture_handler', FixtureHandler);
      system.register('another_handler', AnotherHandler);
      expect(system.handlerCount()).toBe(2);

      system.clear();

      expect(system.handlerCount()).toBe(0);
      expect(system.listHandlers()).toEqual([]);
    });
  });

  describe('initialize', () => {
    test('should set initialized to true', async () => {
      expect(system.initialized).toBe(false);

      await system.initialize();

      expect(system.initialized).toBe(true);
    });

    test('should preserve pre-registered handlers after initialization', async () => {
      system.register('fixture_handler', FixtureHandler);

      await system.initialize();

      expect(system.isRegistered('fixture_handler')).toBe(true);
      expect(system.initialized).toBe(true);
    });
  });

  describe('resolve', () => {
    test('should resolve a registered handler', async () => {
      system.register('fixture_handler', FixtureHandler);
      await system.initialize();

      const handler = await system.resolve('fixture_handler');

      expect(handler).not.toBeNull();
    });

    test('should return null for unregistered handler', async () => {
      await system.initialize();

      const handler = await system.resolve('nonexistent');

      expect(handler).toBeNull();
    });
  });

  describe('loadFromPath', () => {
    test('should return 0 for non-existent path', async () => {
      const count = await system.loadFromPath('/nonexistent/path/that/does/not/exist');

      expect(count).toBe(0);
    });

    test('should return 0 for empty directory', async () => {
      const tempDir = join(tmpdir(), `handler-system-test-${Date.now()}`);
      mkdirSync(tempDir, { recursive: true });

      try {
        const count = await system.loadFromPath(tempDir);
        expect(count).toBe(0);
      } finally {
        rmSync(tempDir, { recursive: true, force: true });
      }
    });
  });

  describe('loadFromEnv', () => {
    const originalEnv = process.env.TYPESCRIPT_HANDLER_PATH;

    afterEach(() => {
      if (originalEnv === undefined) {
        delete process.env.TYPESCRIPT_HANDLER_PATH;
      } else {
        process.env.TYPESCRIPT_HANDLER_PATH = originalEnv;
      }
    });

    test('should return 0 when TYPESCRIPT_HANDLER_PATH is not set', async () => {
      delete process.env.TYPESCRIPT_HANDLER_PATH;

      const count = await system.loadFromEnv();

      expect(count).toBe(0);
    });

    test('should return 0 when TYPESCRIPT_HANDLER_PATH is set to non-existent path', async () => {
      process.env.TYPESCRIPT_HANDLER_PATH = '/nonexistent/path';

      const count = await system.loadFromEnv();

      expect(count).toBe(0);
    });
  });

  describe('getRegistry', () => {
    test('should return the underlying HandlerRegistry', () => {
      const registry = system.getRegistry();

      expect(registry).toBeDefined();
      expect(typeof registry.register).toBe('function');
      expect(typeof registry.resolve).toBe('function');
    });
  });

  describe('debugInfo', () => {
    test('should return debug info with component name', () => {
      const info = system.debugInfo();

      expect(info.component).toBe('HandlerSystem');
      expect(info.handlerCount).toBe(0);
    });

    test('should reflect registered handlers in debug info', () => {
      system.register('fixture_handler', FixtureHandler);

      const info = system.debugInfo();

      expect(info.handlerCount).toBe(1);
    });
  });
});

// =============================================================================
// isValidHandlerClass (tested indirectly via register behavior)
// =============================================================================

describe('HandlerSystem handler class validation', () => {
  let system: HandlerSystem;

  beforeEach(() => {
    system = new HandlerSystem();
  });

  test('should accept valid handler class with static handlerName', () => {
    system.register('test', FixtureHandler);
    expect(system.isRegistered('test')).toBe(true);
  });

  test('should reject registration with invalid name', () => {
    expect(() => system.register('', FixtureHandler)).toThrow();
  });
});
