/**
 * TAS-93: ClassLookupResolver tests.
 *
 * Verifies dynamic import-based handler resolution.
 */

import { describe, expect, it, spyOn } from 'bun:test';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { HandlerDefinition } from '../../../src/registry/handler-definition.js';
import { ClassLookupResolver } from '../../../src/registry/resolvers/class-lookup.js';

// Get the directory of this test file
const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

// Relative paths to test fixtures (from resolver's location at src/registry/resolvers/)
// These use ../ prefix which is allowed by IMPORTABLE_PATTERN
// Dynamic imports resolve relative to the importing module, not the CWD
const DEFAULT_EXPORT_HANDLER = '../../../tests/fixtures/handlers/default-export-handler.ts';
const NAMED_EXPORT_HANDLER = '../../../tests/fixtures/handlers/named-export-handler.ts';
const MULTI_EXPORT_HANDLER = '../../../tests/fixtures/handlers/multi-export-handler.ts';
const NO_HANDLER_MODULE = '../../../tests/fixtures/handlers/no-handler.ts';
const FAILING_CONSTRUCTOR_HANDLER =
  '../../../tests/fixtures/handlers/failing-constructor-handler.ts';

// Absolute path for security tests (to verify rejection)
const FIXTURES_DIR = resolve(__dirname, '../../fixtures/handlers');
const ABSOLUTE_PATH_HANDLER = resolve(FIXTURES_DIR, 'default-export-handler.ts');

describe('ClassLookupResolver', () => {
  describe('properties', () => {
    it('has correct name', () => {
      const resolver = new ClassLookupResolver();
      expect(resolver.name).toBe('class_lookup');
    });

    it('has priority 100', () => {
      const resolver = new ClassLookupResolver();
      expect(resolver.priority).toBe(100);
    });
  });

  describe('canResolve', () => {
    const resolver = new ClassLookupResolver();

    describe('relative paths', () => {
      it('returns true for ./ relative path', () => {
        const definition: HandlerDefinition = { callable: './handlers/payment.js' };
        expect(resolver.canResolve(definition)).toBe(true);
      });

      it('returns true for ../ parent path', () => {
        const definition: HandlerDefinition = { callable: '../handlers/payment.js' };
        expect(resolver.canResolve(definition)).toBe(true);
      });

      it('returns true for nested relative path', () => {
        const definition: HandlerDefinition = { callable: './foo/bar/baz.ts' };
        expect(resolver.canResolve(definition)).toBe(true);
      });
    });

    describe('absolute paths (blocked for security)', () => {
      it('returns false for absolute path', () => {
        const definition: HandlerDefinition = { callable: '/home/user/handlers/payment.js' };
        expect(resolver.canResolve(definition)).toBe(false);
      });

      it('returns false for root path', () => {
        const definition: HandlerDefinition = { callable: '/handlers.js' };
        expect(resolver.canResolve(definition)).toBe(false);
      });
    });

    describe('package paths', () => {
      it('returns true for scoped package', () => {
        const definition: HandlerDefinition = { callable: '@tasker/handlers/payment' };
        expect(resolver.canResolve(definition)).toBe(true);
      });

      it('returns true for scoped package with hyphen', () => {
        const definition: HandlerDefinition = { callable: '@my-scope/package/handler' };
        expect(resolver.canResolve(definition)).toBe(true);
      });
    });

    describe('non-importable paths', () => {
      it('returns false for simple handler name', () => {
        const definition: HandlerDefinition = { callable: 'payment_handler' };
        expect(resolver.canResolve(definition)).toBe(false);
      });

      it('returns false for underscore name', () => {
        const definition: HandlerDefinition = { callable: 'process_payment' };
        expect(resolver.canResolve(definition)).toBe(false);
      });

      it('returns false for PascalCase name', () => {
        const definition: HandlerDefinition = { callable: 'PaymentHandler' };
        expect(resolver.canResolve(definition)).toBe(false);
      });

      it('returns false for namespace path without scope', () => {
        const definition: HandlerDefinition = { callable: 'handlers/payment' };
        expect(resolver.canResolve(definition)).toBe(false);
      });
    });
  });

  describe('resolve', () => {
    describe('with default export', () => {
      it('instantiates handler from default export', async () => {
        const resolver = new ClassLookupResolver();
        const definition: HandlerDefinition = { callable: DEFAULT_EXPORT_HANDLER };

        const handler = await resolver.resolve(definition);

        expect(handler).not.toBeNull();
        expect(handler?.name).toBe('default_export_handler');
      });
    });

    describe('with named export', () => {
      it('instantiates handler from named export', async () => {
        const resolver = new ClassLookupResolver();
        const definition: HandlerDefinition = { callable: NAMED_EXPORT_HANDLER };

        const handler = await resolver.resolve(definition);

        expect(handler).not.toBeNull();
        expect(handler?.name).toBe('named_export_handler');
      });
    });

    describe('error handling', () => {
      it('returns null when module not found', async () => {
        const resolver = new ClassLookupResolver();
        const debugSpy = spyOn(console, 'debug').mockImplementation(() => {});

        const definition: HandlerDefinition = {
          callable: '/nonexistent/path/to/module-xyz123.js',
        };
        const handler = await resolver.resolve(definition);

        expect(handler).toBeNull();
        debugSpy.mockRestore();
      });

      it('returns null when module has no handler class', async () => {
        const resolver = new ClassLookupResolver();
        const definition: HandlerDefinition = { callable: NO_HANDLER_MODULE };

        const handler = await resolver.resolve(definition);

        expect(handler).toBeNull();
      });

      it('returns null when constructor throws', async () => {
        const resolver = new ClassLookupResolver();
        const errorSpy = spyOn(console, 'error').mockImplementation(() => {});

        const definition: HandlerDefinition = { callable: FAILING_CONSTRUCTOR_HANDLER };
        const handler = await resolver.resolve(definition);

        expect(handler).toBeNull();
        errorSpy.mockRestore();
      });
    });

    describe('handler selection', () => {
      it('prefers default export over named exports', async () => {
        const resolver = new ClassLookupResolver();
        const definition: HandlerDefinition = { callable: MULTI_EXPORT_HANDLER };

        const handler = await resolver.resolve(definition);

        // Primary handler is the default export
        expect(handler?.name).toBe('primary_handler');
      });
    });
  });

  describe('pattern edge cases', () => {
    const resolver = new ClassLookupResolver();

    it('rejects empty callable', () => {
      const definition: HandlerDefinition = { callable: '' };
      expect(resolver.canResolve(definition)).toBe(false);
    });

    it('rejects whitespace-only callable', () => {
      const definition: HandlerDefinition = { callable: '   ' };
      expect(resolver.canResolve(definition)).toBe(false);
    });

    it('accepts path with query parameters', () => {
      // While unusual, this pattern is valid for some import systems
      const definition: HandlerDefinition = { callable: './handler.js?version=2' };
      expect(resolver.canResolve(definition)).toBe(true);
    });

    it('accepts TypeScript extensions', () => {
      const definition: HandlerDefinition = { callable: './handler.ts' };
      expect(resolver.canResolve(definition)).toBe(true);
    });

    it('accepts paths without extensions', () => {
      const definition: HandlerDefinition = { callable: './handler' };
      expect(resolver.canResolve(definition)).toBe(true);
    });
  });

  describe('integration', () => {
    it('rejects absolute paths for security', async () => {
      const resolver = new ClassLookupResolver();
      // Absolute paths are blocked to prevent loading arbitrary code
      const definition: HandlerDefinition = { callable: ABSOLUTE_PATH_HANDLER };

      // Should reject absolute paths
      expect(resolver.canResolve(definition)).toBe(false);

      // resolve() still returns null for unsupported paths
      const handler = await resolver.resolve(definition);
      expect(handler).toBeNull();
    });

    it('rejects then does not attempt resolve for non-importable', async () => {
      const resolver = new ClassLookupResolver();
      const debugSpy = spyOn(console, 'debug').mockImplementation(() => {});

      const definition: HandlerDefinition = { callable: 'simple_handler' };

      // Should reject
      expect(resolver.canResolve(definition)).toBe(false);

      // If resolve is called anyway, it should still work (return null)
      const handler = await resolver.resolve(definition);
      expect(handler).toBeNull();

      debugSpy.mockRestore();
    });

    it('can be used with resolver chain', async () => {
      // This test validates the resolver works in a chain context
      const resolver = new ClassLookupResolver();

      // Should not interfere with ExplicitMappingResolver's domain
      const explicitDef: HandlerDefinition = { callable: 'order_handler' };
      expect(resolver.canResolve(explicitDef)).toBe(false);

      // Should claim importable paths
      const importableDef: HandlerDefinition = { callable: './handlers/order.ts' };
      expect(resolver.canResolve(importableDef)).toBe(true);
    });
  });
});
