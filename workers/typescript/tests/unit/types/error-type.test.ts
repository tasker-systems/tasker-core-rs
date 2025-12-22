/**
 * ErrorType enum and helper function tests.
 *
 * Verifies TAS-92 aligned error type classification.
 */

import { describe, expect, it } from 'bun:test';
import {
  ErrorType,
  isStandardErrorType,
  isTypicallyRetryable,
} from '../../../src/types/error-type.js';

describe('ErrorType', () => {
  describe('enum values', () => {
    it('has permanent_error value', () => {
      expect(ErrorType.PERMANENT_ERROR).toBe('permanent_error');
    });

    it('has retryable_error value', () => {
      expect(ErrorType.RETRYABLE_ERROR).toBe('retryable_error');
    });

    it('has validation_error value', () => {
      expect(ErrorType.VALIDATION_ERROR).toBe('validation_error');
    });

    it('has timeout value', () => {
      expect(ErrorType.TIMEOUT).toBe('timeout');
    });

    it('has handler_error value', () => {
      expect(ErrorType.HANDLER_ERROR).toBe('handler_error');
    });

    it('has exactly 5 error types', () => {
      const values = Object.values(ErrorType);
      expect(values).toHaveLength(5);
    });
  });

  describe('string representation', () => {
    it('enum values can be used as strings', () => {
      const errorType: string = ErrorType.PERMANENT_ERROR;
      expect(errorType).toBe('permanent_error');
    });

    it('supports template literal usage', () => {
      const message = `Error: ${ErrorType.TIMEOUT}`;
      expect(message).toBe('Error: timeout');
    });
  });
});

describe('isStandardErrorType', () => {
  it('returns true for PERMANENT_ERROR', () => {
    expect(isStandardErrorType('permanent_error')).toBe(true);
  });

  it('returns true for RETRYABLE_ERROR', () => {
    expect(isStandardErrorType('retryable_error')).toBe(true);
  });

  it('returns true for VALIDATION_ERROR', () => {
    expect(isStandardErrorType('validation_error')).toBe(true);
  });

  it('returns true for TIMEOUT', () => {
    expect(isStandardErrorType('timeout')).toBe(true);
  });

  it('returns true for HANDLER_ERROR', () => {
    expect(isStandardErrorType('handler_error')).toBe(true);
  });

  it('returns false for custom error types', () => {
    expect(isStandardErrorType('custom_error')).toBe(false);
  });

  it('returns false for empty string', () => {
    expect(isStandardErrorType('')).toBe(false);
  });

  it('returns false for close but incorrect values', () => {
    expect(isStandardErrorType('PERMANENT_ERROR')).toBe(false);
    expect(isStandardErrorType('Timeout')).toBe(false);
    expect(isStandardErrorType('permanent-error')).toBe(false);
  });
});

describe('isTypicallyRetryable', () => {
  it('returns true for RETRYABLE_ERROR', () => {
    expect(isTypicallyRetryable('retryable_error')).toBe(true);
  });

  it('returns true for TIMEOUT', () => {
    expect(isTypicallyRetryable('timeout')).toBe(true);
  });

  it('returns false for PERMANENT_ERROR', () => {
    expect(isTypicallyRetryable('permanent_error')).toBe(false);
  });

  it('returns false for VALIDATION_ERROR', () => {
    expect(isTypicallyRetryable('validation_error')).toBe(false);
  });

  it('returns false for HANDLER_ERROR', () => {
    expect(isTypicallyRetryable('handler_error')).toBe(false);
  });

  it('returns false for custom error types', () => {
    expect(isTypicallyRetryable('custom_error')).toBe(false);
  });

  it('returns false for empty string', () => {
    expect(isTypicallyRetryable('')).toBe(false);
  });
});
