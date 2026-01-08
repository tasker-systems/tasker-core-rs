/**
 * TAS-93: HandlerDefinition tests.
 *
 * Verifies handler definition normalization and helper functions.
 */

import { describe, expect, it } from 'bun:test';
import {
  effectiveMethod,
  fromCallable,
  fromDto,
  type HandlerDefinition,
  type HandlerSpec,
  hasResolverHint,
  normalizeToDefinition,
  usesMethodDispatch,
} from '../../../src/registry/handler-definition.js';

describe('HandlerDefinition', () => {
  describe('fromCallable', () => {
    it('creates definition from string', () => {
      const definition = fromCallable('my_handler');

      expect(definition.callable).toBe('my_handler');
      expect(definition.method).toBeNull();
      expect(definition.resolver).toBeNull();
    });
  });

  describe('fromDto', () => {
    it('creates definition from DTO with all fields', () => {
      const dto = {
        callable: 'my_handler',
        method: 'process',
        resolver: 'explicit_mapping',
        initialization: { key: 'value' },
      };

      const definition = fromDto(dto);

      expect(definition.callable).toBe('my_handler');
      expect(definition.method).toBe('process');
      expect(definition.resolver).toBe('explicit_mapping');
      expect(definition.initialization).toEqual({ key: 'value' });
    });

    it('handles null fields', () => {
      const dto = {
        callable: 'my_handler',
        method: null,
        resolver: null,
        initialization: null,
      };

      const definition = fromDto(dto);

      expect(definition.callable).toBe('my_handler');
      expect(definition.method).toBeNull();
      expect(definition.resolver).toBeNull();
    });
  });

  describe('normalizeToDefinition', () => {
    it('normalizes string to definition', () => {
      const spec: HandlerSpec = 'my_handler';
      const definition = normalizeToDefinition(spec);

      expect(definition.callable).toBe('my_handler');
    });

    it('passes through HandlerDefinition', () => {
      const spec: HandlerDefinition = {
        callable: 'my_handler',
        method: 'process',
      };
      const definition = normalizeToDefinition(spec);

      expect(definition.callable).toBe('my_handler');
      expect(definition.method).toBe('process');
    });

    it('handles DTO-like objects', () => {
      const spec = {
        callable: 'my_handler',
        method: 'process',
        resolver: 'custom',
        initialization: { key: 'value' },
      };
      const definition = normalizeToDefinition(spec);

      expect(definition.callable).toBe('my_handler');
      expect(definition.method).toBe('process');
      expect(definition.resolver).toBe('custom');
      expect(definition.initialization).toEqual({ key: 'value' });
    });
  });

  describe('effectiveMethod', () => {
    it('returns method when specified', () => {
      const definition: HandlerDefinition = {
        callable: 'my_handler',
        method: 'process',
      };

      expect(effectiveMethod(definition)).toBe('process');
    });

    it('returns "call" when method is undefined', () => {
      const definition: HandlerDefinition = {
        callable: 'my_handler',
      };

      expect(effectiveMethod(definition)).toBe('call');
    });

    it('returns "call" when method is null', () => {
      const definition: HandlerDefinition = {
        callable: 'my_handler',
        method: null,
      };

      expect(effectiveMethod(definition)).toBe('call');
    });

    it('returns "call" when method is empty string', () => {
      const definition: HandlerDefinition = {
        callable: 'my_handler',
        method: '',
      };

      expect(effectiveMethod(definition)).toBe('call');
    });
  });

  describe('usesMethodDispatch', () => {
    it('returns true when method is specified and not "call"', () => {
      const definition: HandlerDefinition = {
        callable: 'my_handler',
        method: 'process',
      };

      expect(usesMethodDispatch(definition)).toBe(true);
    });

    it('returns false when method is "call"', () => {
      const definition: HandlerDefinition = {
        callable: 'my_handler',
        method: 'call',
      };

      expect(usesMethodDispatch(definition)).toBe(false);
    });

    it('returns false when method is undefined', () => {
      const definition: HandlerDefinition = {
        callable: 'my_handler',
      };

      expect(usesMethodDispatch(definition)).toBe(false);
    });

    it('returns false when method is null', () => {
      const definition: HandlerDefinition = {
        callable: 'my_handler',
        method: null,
      };

      expect(usesMethodDispatch(definition)).toBe(false);
    });
  });

  describe('hasResolverHint', () => {
    it('returns true when resolver is specified', () => {
      const definition: HandlerDefinition = {
        callable: 'my_handler',
        resolver: 'explicit_mapping',
      };

      expect(hasResolverHint(definition)).toBe(true);
    });

    it('returns false when resolver is undefined', () => {
      const definition: HandlerDefinition = {
        callable: 'my_handler',
      };

      expect(hasResolverHint(definition)).toBe(false);
    });

    it('returns false when resolver is null', () => {
      const definition: HandlerDefinition = {
        callable: 'my_handler',
        resolver: null,
      };

      expect(hasResolverHint(definition)).toBe(false);
    });

    it('returns false when resolver is empty string', () => {
      const definition: HandlerDefinition = {
        callable: 'my_handler',
        resolver: '',
      };

      expect(hasResolverHint(definition)).toBe(false);
    });
  });
});
