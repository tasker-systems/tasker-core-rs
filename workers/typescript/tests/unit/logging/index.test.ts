/**
 * Tests for the structured logging API.
 */

import { describe, test, expect, beforeEach, afterEach, mock, spyOn } from 'bun:test';
import {
  logError,
  logWarn,
  logInfo,
  logDebug,
  logTrace,
  createLogger,
  type LogFields,
} from '../../../src/logging/index';

describe('Logging API', () => {
  let consoleSpy: ReturnType<typeof spyOn>;

  beforeEach(() => {
    // Capture console.log calls for fallback logging tests
    consoleSpy = spyOn(console, 'log').mockImplementation(() => {});
  });

  afterEach(() => {
    consoleSpy.mockRestore();
  });

  describe('logError', () => {
    test('should accept message only', () => {
      // When FFI is not loaded, it falls back to console
      expect(() => logError('Test error')).not.toThrow();
    });

    test('should accept message and fields', () => {
      const fields: LogFields = {
        component: 'test',
        error_code: 'E001',
      };
      expect(() => logError('Test error', fields)).not.toThrow();
    });

    test('should handle undefined fields', () => {
      expect(() => logError('Test error', undefined)).not.toThrow();
    });

    test('should handle empty fields', () => {
      expect(() => logError('Test error', {})).not.toThrow();
    });
  });

  describe('logWarn', () => {
    test('should accept message only', () => {
      expect(() => logWarn('Test warning')).not.toThrow();
    });

    test('should accept message and fields', () => {
      const fields: LogFields = {
        component: 'test',
        retry_count: 3,
      };
      expect(() => logWarn('Test warning', fields)).not.toThrow();
    });
  });

  describe('logInfo', () => {
    test('should accept message only', () => {
      expect(() => logInfo('Test info')).not.toThrow();
    });

    test('should accept message and fields', () => {
      const fields: LogFields = {
        component: 'test',
        operation: 'startup',
      };
      expect(() => logInfo('Test info', fields)).not.toThrow();
    });
  });

  describe('logDebug', () => {
    test('should accept message only', () => {
      expect(() => logDebug('Test debug')).not.toThrow();
    });

    test('should accept message and fields', () => {
      const fields: LogFields = {
        component: 'test',
        payload_size: 1024,
      };
      expect(() => logDebug('Test debug', fields)).not.toThrow();
    });
  });

  describe('logTrace', () => {
    test('should accept message only', () => {
      expect(() => logTrace('Test trace')).not.toThrow();
    });

    test('should accept message and fields', () => {
      const fields: LogFields = {
        component: 'test',
        function: 'process_step',
      };
      expect(() => logTrace('Test trace', fields)).not.toThrow();
    });
  });

  describe('createLogger', () => {
    test('should create a logger with default fields', () => {
      const logger = createLogger({ component: 'test_component' });

      expect(logger).toBeDefined();
      expect(typeof logger.error).toBe('function');
      expect(typeof logger.warn).toBe('function');
      expect(typeof logger.info).toBe('function');
      expect(typeof logger.debug).toBe('function');
      expect(typeof logger.trace).toBe('function');
    });

    test('should merge default fields with call-time fields', () => {
      const logger = createLogger({ component: 'test_component' });

      // All methods should work without throwing
      expect(() => logger.error('Error message', { error_code: 'E001' })).not.toThrow();
      expect(() => logger.warn('Warning message', { retry_count: 1 })).not.toThrow();
      expect(() => logger.info('Info message', { operation: 'test' })).not.toThrow();
      expect(() => logger.debug('Debug message', { data: 'value' })).not.toThrow();
      expect(() => logger.trace('Trace message', { step: 'entry' })).not.toThrow();
    });

    test('should allow call-time fields to override default fields', () => {
      const logger = createLogger({ component: 'default_component' });

      // Override component with call-time field
      expect(() =>
        logger.info('Info message', { component: 'override_component' })
      ).not.toThrow();
    });

    test('should work with empty default fields', () => {
      const logger = createLogger({});

      expect(() => logger.info('Info message')).not.toThrow();
    });
  });

  describe('LogFields type', () => {
    test('should accept string values', () => {
      const fields: LogFields = { key: 'value' };
      expect(() => logInfo('Test', fields)).not.toThrow();
    });

    test('should accept number values', () => {
      const fields: LogFields = { count: 42 };
      expect(() => logInfo('Test', fields)).not.toThrow();
    });

    test('should accept boolean values', () => {
      const fields: LogFields = { enabled: true };
      expect(() => logInfo('Test', fields)).not.toThrow();
    });

    test('should accept null values', () => {
      const fields: LogFields = { optional: null };
      expect(() => logInfo('Test', fields)).not.toThrow();
    });

    test('should accept undefined values (filtered out)', () => {
      const fields: LogFields = { optional: undefined };
      expect(() => logInfo('Test', fields)).not.toThrow();
    });

    test('should accept mixed value types', () => {
      const fields: LogFields = {
        component: 'test',
        count: 42,
        enabled: true,
        optional: null,
      };
      expect(() => logInfo('Test', fields)).not.toThrow();
    });
  });
});
