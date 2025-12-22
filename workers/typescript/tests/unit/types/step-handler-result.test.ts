/**
 * StepHandlerResult class tests.
 *
 * Verifies result creation, factory methods, and JSON serialization.
 */

import { describe, expect, it } from 'bun:test';
import { ErrorType } from '../../../src/types/error-type.js';
import { StepHandlerResult } from '../../../src/types/step-handler-result.js';

describe('StepHandlerResult', () => {
  describe('constructor', () => {
    it('creates a success result with defaults', () => {
      const result = new StepHandlerResult({
        success: true,
        result: { value: 42 },
      });

      expect(result.success).toBe(true);
      expect(result.result).toEqual({ value: 42 });
      expect(result.errorMessage).toBeNull();
      expect(result.errorType).toBeNull();
      expect(result.errorCode).toBeNull();
      expect(result.retryable).toBe(true);
      expect(result.metadata).toEqual({});
    });

    it('creates a failure result with all fields', () => {
      const result = new StepHandlerResult({
        success: false,
        errorMessage: 'Something went wrong',
        errorType: 'handler_error',
        errorCode: 'ERR_001',
        retryable: false,
        metadata: { attempt: 3 },
      });

      expect(result.success).toBe(false);
      expect(result.result).toBeNull();
      expect(result.errorMessage).toBe('Something went wrong');
      expect(result.errorType).toBe('handler_error');
      expect(result.errorCode).toBe('ERR_001');
      expect(result.retryable).toBe(false);
      expect(result.metadata).toEqual({ attempt: 3 });
    });

    it('defaults null result correctly', () => {
      const result = new StepHandlerResult({
        success: true,
      });

      expect(result.result).toBeNull();
    });
  });

  describe('success factory', () => {
    it('creates a success result with result data', () => {
      const result = StepHandlerResult.success({ processed: 100, skipped: 5 });

      expect(result.success).toBe(true);
      expect(result.result).toEqual({ processed: 100, skipped: 5 });
      expect(result.errorMessage).toBeNull();
      expect(result.errorType).toBeNull();
    });

    it('creates a success result with metadata', () => {
      const result = StepHandlerResult.success({ value: 'output' }, { execution_time_ms: 150 });

      expect(result.success).toBe(true);
      expect(result.result).toEqual({ value: 'output' });
      expect(result.metadata).toEqual({ execution_time_ms: 150 });
    });

    it('defaults metadata to empty object', () => {
      const result = StepHandlerResult.success({ data: 'test' });

      expect(result.metadata).toEqual({});
    });

    it('creates success result with empty result object', () => {
      const result = StepHandlerResult.success({});

      expect(result.success).toBe(true);
      expect(result.result).toEqual({});
    });
  });

  describe('failure factory', () => {
    it('creates a failure result with message only', () => {
      const result = StepHandlerResult.failure('Something went wrong');

      expect(result.success).toBe(false);
      expect(result.errorMessage).toBe('Something went wrong');
      expect(result.errorType).toBe('handler_error'); // default
      expect(result.retryable).toBe(true); // default
    });

    it('creates a failure with ErrorType enum', () => {
      const result = StepHandlerResult.failure('Invalid input', ErrorType.VALIDATION_ERROR, false);

      expect(result.success).toBe(false);
      expect(result.errorMessage).toBe('Invalid input');
      expect(result.errorType).toBe('validation_error');
      expect(result.retryable).toBe(false);
    });

    it('creates a failure with string error type', () => {
      const result = StepHandlerResult.failure('Custom error', 'custom_error_type', true);

      expect(result.errorType).toBe('custom_error_type');
    });

    it('creates a failure with all parameters', () => {
      const result = StepHandlerResult.failure(
        'Gateway timeout',
        ErrorType.TIMEOUT,
        true,
        { gateway: 'stripe', duration_ms: 30000 },
        'GATEWAY_TIMEOUT'
      );

      expect(result.success).toBe(false);
      expect(result.errorMessage).toBe('Gateway timeout');
      expect(result.errorType).toBe('timeout');
      expect(result.retryable).toBe(true);
      expect(result.metadata).toEqual({ gateway: 'stripe', duration_ms: 30000 });
      expect(result.errorCode).toBe('GATEWAY_TIMEOUT');
    });

    it('defaults error code to null', () => {
      const result = StepHandlerResult.failure('Error', ErrorType.HANDLER_ERROR, true, {
        key: 'value',
      });

      expect(result.errorCode).toBeNull();
    });

    it('sets result to null for failures', () => {
      const result = StepHandlerResult.failure('Error');

      expect(result.result).toBeNull();
    });
  });

  describe('isSuccess', () => {
    it('returns true for success result', () => {
      const result = StepHandlerResult.success({ data: 'test' });
      expect(result.isSuccess()).toBe(true);
    });

    it('returns false for failure result', () => {
      const result = StepHandlerResult.failure('Error');
      expect(result.isSuccess()).toBe(false);
    });
  });

  describe('isFailure', () => {
    it('returns false for success result', () => {
      const result = StepHandlerResult.success({ data: 'test' });
      expect(result.isFailure()).toBe(false);
    });

    it('returns true for failure result', () => {
      const result = StepHandlerResult.failure('Error');
      expect(result.isFailure()).toBe(true);
    });
  });

  describe('toJSON', () => {
    it('serializes success result with snake_case keys', () => {
      const result = StepHandlerResult.success({ output: 'data' }, { timing: 100 });
      const json = result.toJSON();

      expect(json).toEqual({
        success: true,
        result: { output: 'data' },
        error_message: null,
        error_type: null,
        error_code: null,
        retryable: true,
        metadata: { timing: 100 },
      });
    });

    it('serializes failure result with snake_case keys', () => {
      const result = StepHandlerResult.failure(
        'Validation failed',
        ErrorType.VALIDATION_ERROR,
        false,
        { field: 'email' },
        'INVALID_EMAIL'
      );
      const json = result.toJSON();

      expect(json).toEqual({
        success: false,
        result: null,
        error_message: 'Validation failed',
        error_type: 'validation_error',
        error_code: 'INVALID_EMAIL',
        retryable: false,
        metadata: { field: 'email' },
      });
    });

    it('produces valid JSON string when stringified', () => {
      const result = StepHandlerResult.success({ key: 'value' });
      const jsonString = JSON.stringify(result);
      const parsed = JSON.parse(jsonString);

      expect(parsed.success).toBe(true);
      expect(parsed.result).toEqual({ key: 'value' });
    });
  });

  describe('error type values', () => {
    it('supports all standard error types', () => {
      const types = [
        ErrorType.PERMANENT_ERROR,
        ErrorType.RETRYABLE_ERROR,
        ErrorType.VALIDATION_ERROR,
        ErrorType.TIMEOUT,
        ErrorType.HANDLER_ERROR,
      ];

      for (const errorType of types) {
        const result = StepHandlerResult.failure('Error', errorType);
        expect(result.errorType).toBe(errorType);
      }
    });
  });

  describe('readonly properties', () => {
    it('has readonly properties accessible', () => {
      const result = StepHandlerResult.success({ data: 'test' });

      // Properties should be readonly at compile time
      expect(result.success).toBe(true);
      expect(result.result).toEqual({ data: 'test' });
    });

    it('stores metadata reference (caller should not mutate)', () => {
      const metadata = { key: 'value' };
      const result = StepHandlerResult.success({ data: 'test' }, metadata);

      // Note: metadata is passed by reference (matching Python/Ruby workers)
      // Callers should not mutate the passed objects
      expect(result.metadata).toBe(metadata);
    });
  });
});
