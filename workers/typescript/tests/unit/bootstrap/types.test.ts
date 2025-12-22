/**
 * Tests for bootstrap types and conversion functions.
 */

import { describe, test, expect } from 'bun:test';
import {
  toFfiBootstrapConfig,
  fromFfiBootstrapResult,
  fromFfiWorkerStatus,
  fromFfiStopResult,
  type BootstrapConfig,
  type FfiBootstrapResult,
  type FfiWorkerStatus,
  type FfiStopResult,
} from '../../../src/bootstrap/types';

describe('Bootstrap Types', () => {
  describe('toFfiBootstrapConfig', () => {
    test('should return empty object for undefined config', () => {
      const result = toFfiBootstrapConfig(undefined);
      expect(result).toEqual({});
    });

    test('should convert TypeScript config to FFI format', () => {
      const config: BootstrapConfig = {
        workerId: 'worker-1',
        namespace: 'payments',
        configPath: '/path/to/config.toml',
        logLevel: 'debug',
        databaseUrl: 'postgresql://localhost/test',
      };

      const result = toFfiBootstrapConfig(config);

      expect(result.worker_id).toBe('worker-1');
      expect(result.namespace).toBe('payments');
      expect(result.config_path).toBe('/path/to/config.toml');
      expect(result.log_level).toBe('debug');
      expect(result.database_url).toBe('postgresql://localhost/test');
    });

    test('should handle partial config', () => {
      const config: BootstrapConfig = {
        namespace: 'default',
      };

      const result = toFfiBootstrapConfig(config);

      expect(result.namespace).toBe('default');
      expect(result.worker_id).toBeUndefined();
      expect(result.log_level).toBeUndefined();
    });

    test('should handle empty config', () => {
      const config: BootstrapConfig = {};
      const result = toFfiBootstrapConfig(config);
      expect(result).toBeDefined();
    });
  });

  describe('fromFfiBootstrapResult', () => {
    test('should convert successful FFI result', () => {
      const ffiResult: FfiBootstrapResult = {
        success: true,
        status: 'started',
        message: 'Worker started successfully',
        worker_id: 'worker-123',
      };

      const result = fromFfiBootstrapResult(ffiResult);

      expect(result.success).toBe(true);
      expect(result.status).toBe('started');
      expect(result.message).toBe('Worker started successfully');
      expect(result.workerId).toBe('worker-123');
      expect(result.error).toBeUndefined();
    });

    test('should convert failed FFI result', () => {
      const ffiResult: FfiBootstrapResult = {
        success: false,
        status: 'error',
        message: 'Database connection failed',
        error: 'Connection timeout',
      };

      const result = fromFfiBootstrapResult(ffiResult);

      expect(result.success).toBe(false);
      expect(result.status).toBe('error');
      expect(result.message).toBe('Database connection failed');
      expect(result.error).toBe('Connection timeout');
    });

    test('should convert already_running FFI result', () => {
      const ffiResult: FfiBootstrapResult = {
        success: true,
        status: 'already_running',
        message: 'Worker is already running',
        worker_id: 'existing-worker',
      };

      const result = fromFfiBootstrapResult(ffiResult);

      expect(result.success).toBe(true);
      expect(result.status).toBe('already_running');
      expect(result.workerId).toBe('existing-worker');
    });
  });

  describe('fromFfiWorkerStatus', () => {
    test('should convert running worker status', () => {
      const ffiStatus: FfiWorkerStatus = {
        success: true,
        running: true,
        worker_id: 'worker-123',
        environment: 'production',
        worker_core_status: 'healthy',
        web_api_enabled: true,
        supported_namespaces: ['default', 'payments'],
        database_pool_size: 10,
        database_pool_idle: 5,
      };

      const result = fromFfiWorkerStatus(ffiStatus);

      expect(result.success).toBe(true);
      expect(result.running).toBe(true);
      expect(result.workerId).toBe('worker-123');
      expect(result.environment).toBe('production');
      expect(result.workerCoreStatus).toBe('healthy');
      expect(result.webApiEnabled).toBe(true);
      expect(result.supportedNamespaces).toEqual(['default', 'payments']);
      expect(result.databasePoolSize).toBe(10);
      expect(result.databasePoolIdle).toBe(5);
    });

    test('should convert stopped worker status', () => {
      const ffiStatus: FfiWorkerStatus = {
        success: true,
        running: false,
        status: 'stopped',
      };

      const result = fromFfiWorkerStatus(ffiStatus);

      expect(result.success).toBe(true);
      expect(result.running).toBe(false);
      expect(result.status).toBe('stopped');
    });

    test('should handle minimal status', () => {
      const ffiStatus: FfiWorkerStatus = {
        success: false,
        running: false,
      };

      const result = fromFfiWorkerStatus(ffiStatus);

      expect(result.success).toBe(false);
      expect(result.running).toBe(false);
      expect(result.workerId).toBeUndefined();
    });
  });

  describe('fromFfiStopResult', () => {
    test('should convert successful stop result', () => {
      const ffiResult: FfiStopResult = {
        success: true,
        status: 'stopped',
        message: 'Worker stopped successfully',
        worker_id: 'worker-123',
      };

      const result = fromFfiStopResult(ffiResult);

      expect(result.success).toBe(true);
      expect(result.status).toBe('stopped');
      expect(result.message).toBe('Worker stopped successfully');
      expect(result.workerId).toBe('worker-123');
    });

    test('should convert not_running stop result', () => {
      const ffiResult: FfiStopResult = {
        success: true,
        status: 'not_running',
        message: 'Worker was not running',
      };

      const result = fromFfiStopResult(ffiResult);

      expect(result.success).toBe(true);
      expect(result.status).toBe('not_running');
    });

    test('should convert failed stop result', () => {
      const ffiResult: FfiStopResult = {
        success: false,
        status: 'error',
        message: 'Failed to stop worker',
        error: 'Timeout waiting for handlers',
      };

      const result = fromFfiStopResult(ffiResult);

      expect(result.success).toBe(false);
      expect(result.status).toBe('error');
      expect(result.error).toBe('Timeout waiting for handlers');
    });
  });
});

describe('Bootstrap Config Type', () => {
  test('should accept all valid log levels', () => {
    const levels: Array<'trace' | 'debug' | 'info' | 'warn' | 'error'> = [
      'trace',
      'debug',
      'info',
      'warn',
      'error',
    ];

    for (const level of levels) {
      const config: BootstrapConfig = { logLevel: level };
      expect(config.logLevel).toBe(level);
    }
  });

  test('should allow all fields to be optional', () => {
    const config: BootstrapConfig = {};
    expect(config.workerId).toBeUndefined();
    expect(config.namespace).toBeUndefined();
    expect(config.configPath).toBeUndefined();
    expect(config.logLevel).toBeUndefined();
    expect(config.databaseUrl).toBeUndefined();
  });
});
