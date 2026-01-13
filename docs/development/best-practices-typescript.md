# TypeScript Best Practices for Tasker Core

**Purpose**: Codify TypeScript-specific coding standards for the tasker-core workers/typescript project.

---

## Code Style

### Formatting & Linting
- Use `biome` for linting and formatting (enforced via `cargo make check-typescript`)
- Use `tsc` for type checking
- Configuration in `biome.json` and `tsconfig.json`

### Naming Conventions
```typescript
// Classes/Types/Interfaces: PascalCase
class StepHandler {}
interface StepContext {}
type HandlerResult = Success | Failure;

// Functions/methods/variables: camelCase
function processStep(context: StepContext): HandlerResult {}
const orderHandler = new OrderHandler();

// Constants: SCREAMING_SNAKE_CASE or camelCase
const DEFAULT_TIMEOUT = 30;
const maxRetries = 3;

// Private members: leading underscore or #
class Handler {
  private _config: Config;
  #internalState: State;
}

// Type parameters: T, K, V or descriptive
function process<T extends Handler>(handler: T): Result<T> {}
function map<Key, Value>(entries: Map<Key, Value>): void {}
```

### Module Organization
```typescript
// handlers/order-handler.ts

/**
 * Order processing step handler.
 * @module handlers/order-handler
 */

// 1. External imports
import { z } from 'zod';

// 2. Internal imports (absolute paths preferred)
import { StepHandler } from '@/handler/step-handler';
import { applyAPI } from '@/handler/mixins/api';
import type { StepContext, StepHandlerResult } from '@/types';

// 3. Type definitions
interface OrderData {
  orderId: string;
  items: OrderItem[];
}

// 4. Constants
const DEFAULT_TIMEOUT = 30_000;

// 5. Main exports
export class OrderHandler extends StepHandler {
  // ...
}

// 6. Helper functions (if needed)
function validateOrder(data: unknown): OrderData {
  // ...
}
```

---

## Type Safety

### Prefer Strict Types
```typescript
// BAD: Using any
function process(data: any): any {
  return data.result;
}

// GOOD: Explicit types
function process(data: StepInput): ProcessResult {
  return { result: data.payload };
}

// GOOD: Use unknown for truly unknown data
function parseResponse(data: unknown): OrderData {
  return OrderDataSchema.parse(data);
}
```

### Discriminated Unions
```typescript
// Use discriminated unions for result types
type StepHandlerResult =
  | { success: true; result: Record<string, unknown>; metadata?: Record<string, unknown> }
  | { success: false; error: StepError };

// Exhaustive checking
function handleResult(result: StepHandlerResult): void {
  if (result.success) {
    console.log('Success:', result.result);
  } else {
    console.error('Error:', result.error.message);
  }
}
```

### Zod for Runtime Validation
```typescript
import { z } from 'zod';

// Define schemas for external data
const OrderInputSchema = z.object({
  orderId: z.string().uuid(),
  items: z.array(z.object({
    sku: z.string(),
    quantity: z.number().positive(),
  })),
});

type OrderInput = z.infer<typeof OrderInputSchema>;

// Validate at boundaries
function call(context: StepContext): StepHandlerResult {
  const input = OrderInputSchema.safeParse(context.inputData);
  if (!input.success) {
    return this.failure(
      `Invalid input: ${input.error.message}`,
      'ValidationError',
    );
  }
  // input.data is now typed as OrderInput
  return this.processOrder(input.data);
}
```

---

## Handler Patterns

### Base Handler Structure
```typescript
import { StepHandler } from '@/handler/step-handler';
import { applyAPI, applyDecision } from '@/handler/mixins';
import type { StepContext, StepHandlerResult } from '@/types';

/**
 * Processes order operations via external API.
 *
 * @example
 * ```typescript
 * const handler = new OrderHandler();
 * const result = await handler.call(context);
 * ```
 */
export class OrderHandler extends StepHandler {
  constructor() {
    super();
    // Apply mixins (TAS-112 composition pattern)
    applyAPI(this);
    applyDecision(this);
  }

  async call(context: StepContext): Promise<StepHandlerResult> {
    try {
      const orderId = context.inputData.orderId as string;
      if (!orderId) {
        return this.failure('Missing order_id', 'ValidationError');
      }

      const response = await this.get(`/api/orders/${orderId}`);

      if (response.ok) {
        return this.success(await response.json(), {
          fetchedAt: new Date().toISOString(),
        });
      }

      return this.failure(
        `API error: ${response.status}`,
        'APIError',
        undefined,
        response.status >= 500, // retryable for server errors
      );
    } catch (error) {
      return this.failure(
        error instanceof Error ? error.message : 'Unknown error',
        'UnexpectedError',
        undefined,
        true,
      );
    }
  }
}
```

### Result Factory Methods
```typescript
// Success result
this.success(
  { orderId: '123', status: 'processed' },
  { durationMs: 150 },
);

// Failure result
this.failure(
  'Order validation failed',
  'ValidationError',
  'INVALID_QUANTITY',
  false, // not retryable
);

// Decision result (for decision handlers)
this.decisionSuccess(
  ['shipOrder', 'sendConfirmation'],
  { decision: 'standardFlow' },
);

// Skip branches
this.skipBranches(
  'No items require processing',
  { skipReason: 'emptyCart' },
);
```

---

## Mixin Pattern (TAS-112)

### Using Mixins
```typescript
import { StepHandler } from '@/handler/step-handler';
import { applyAPI, applyDecision, applyBatchable } from '@/handler/mixins';

class MultiCapabilityHandler extends StepHandler {
  constructor() {
    super();
    // Apply capabilities via mixins
    applyAPI(this);
    applyDecision(this);
  }

  async call(context: StepContext): Promise<StepHandlerResult> {
    // API mixin provides HTTP methods
    const response = await this.get('/api/status');
    const data = await response.json();

    if (data.needsDecision) {
      // Decision mixin provides routing methods
      return this.decisionSuccess(
        ['routeA', 'routeB'],
        { type: 'conditional' },
      );
    }

    return this.success(data);
  }
}
```

### Available Mixins
| Mixin | Purpose | Methods Provided |
|-------|---------|------------------|
| `applyAPI` | HTTP requests | `get`, `post`, `put`, `delete` |
| `applyDecision` | Decision points | `decisionSuccess`, `skipBranches`, `decisionFailure` |
| `BatchableHandler` | Batch processing | `getBatchContext`, `batchWorkerComplete`, `handleNoOpWorker` |

---

## Error Handling

### Use Custom Error Classes
```typescript
// errors/handler-errors.ts
export class HandlerError extends Error {
  constructor(
    message: string,
    public readonly errorType: string,
    public readonly retryable: boolean = false,
  ) {
    super(message);
    this.name = 'HandlerError';
  }
}

export class ValidationError extends HandlerError {
  constructor(message: string) {
    super(message, 'ValidationError', false);
    this.name = 'ValidationError';
  }
}

export class APIError extends HandlerError {
  constructor(
    message: string,
    public readonly statusCode: number,
  ) {
    super(message, 'APIError', statusCode >= 500);
    this.name = 'APIError';
  }
}
```

### Handle Errors Appropriately
```typescript
async call(context: StepContext): Promise<StepHandlerResult> {
  try {
    return await this.processOrder(context);
  } catch (error) {
    if (error instanceof ValidationError) {
      return this.failure(error.message, 'ValidationError', undefined, false);
    }
    if (error instanceof APIError) {
      return this.failure(
        error.message,
        'APIError',
        String(error.statusCode),
        error.retryable,
      );
    }
    // Unknown errors are retryable by default
    const message = error instanceof Error ? error.message : 'Unknown error';
    return this.failure(message, 'UnexpectedError', undefined, true);
  }
}
```

---

## Documentation

### JSDoc Comments
```typescript
/**
 * Handles order processing operations.
 *
 * This handler coordinates order validation, inventory checks,
 * and fulfillment initiation via external APIs.
 *
 * @example
 * ```typescript
 * const handler = new OrderHandler();
 * const context = createContext({ orderId: '123' });
 * const result = await handler.call(context);
 *
 * if (result.success) {
 *   console.log('Order processed:', result.result);
 * }
 * ```
 */
export class OrderHandler extends StepHandler {
  /**
   * Process an order step.
   *
   * @param context - Execution context containing order data and config
   * @returns Promise resolving to success or failure result
   *
   * @throws {ValidationError} If order_id is missing or invalid
   *
   * @example
   * ```typescript
   * const result = await handler.call(context);
   * ```
   */
  async call(context: StepContext): Promise<StepHandlerResult> {
    // ...
  }

  /**
   * Validate order input data.
   *
   * @param data - Raw input data from context
   * @returns Validated order data
   * @throws {ValidationError} If validation fails
   */
  private validateInput(data: unknown): OrderInput {
    // ...
  }
}
```

---

## Testing

### Vitest Patterns
```typescript
// tests/handlers/order-handler.test.ts
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { OrderHandler } from '@/handlers/order-handler';
import { createTestContext } from '@/testing/helpers';

describe('OrderHandler', () => {
  let handler: OrderHandler;

  beforeEach(() => {
    handler = new OrderHandler();
  });

  describe('call', () => {
    it('returns success with valid order', async () => {
      const context = createTestContext({
        inputData: { orderId: '12345' },
      });

      vi.spyOn(handler, 'get').mockResolvedValue(
        new Response(JSON.stringify({ id: '12345', status: 'pending' }), {
          status: 200,
        }),
      );

      const result = await handler.call(context);

      expect(result.success).toBe(true);
      expect(result.result).toEqual({ id: '12345', status: 'pending' });
    });

    it('returns failure when order_id is missing', async () => {
      const context = createTestContext({ inputData: {} });

      const result = await handler.call(context);

      expect(result.success).toBe(false);
      expect(result.error?.errorType).toBe('ValidationError');
      expect(result.error?.retryable).toBe(false);
    });

    it.each([
      [400, false],
      [404, false],
      [500, true],
      [502, true],
      [503, true],
    ])('handles %i status with retryable=%s', async (status, retryable) => {
      const context = createTestContext({ inputData: { orderId: '123' } });
      vi.spyOn(handler, 'get').mockResolvedValue(
        new Response(null, { status }),
      );

      const result = await handler.call(context);

      expect(result.success).toBe(false);
      expect(result.error?.retryable).toBe(retryable);
    });
  });
});
```

### Test Helpers
```typescript
// testing/helpers.ts
import type { StepContext } from '@/types';

export function createTestContext(
  overrides: Partial<StepContext> = {},
): StepContext {
  return {
    taskUuid: 'test-task-uuid',
    stepUuid: 'test-step-uuid',
    inputData: {},
    stepConfig: {},
    dependencyResults: {},
    retryCount: 0,
    maxRetries: 3,
    ...overrides,
  };
}
```

---

## FFI Considerations

### Working with Rust Extensions
```typescript
// FFI bindings use koffi for native library loading
import { loadFfiLibrary } from '@/ffi/loader';

// Types are converted automatically:
// TypeScript object <-> Rust HashMap
// TypeScript array <-> Rust Vec
// TypeScript string <-> Rust String

// Be mindful of async boundaries
async function processWithFfi(data: unknown): Promise<Result> {
  // FFI calls may block, wrap appropriately
  const result = await ffiFunction(data);
  return result;
}
```

### Multi-Runtime Support
```typescript
// The worker supports Bun, Node.js, and Deno
// Use runtime-agnostic APIs where possible

// BAD: Node-specific
import { readFile } from 'fs/promises';

// GOOD: Use Bun-compatible APIs or abstractions
const file = await Bun.file(path).text();

// Or use runtime detection
const runtime = detectRuntime();
if (runtime === 'bun') {
  // Bun-specific
} else {
  // Node/Deno fallback
}
```

---

## Project-Specific Patterns

### Registry Usage
```typescript
import { Registry } from '@/registry';

// Register handlers
Registry.getInstance().register('orderHandler', OrderHandler);

// Check availability
Registry.getInstance().isRegistered('orderHandler');

// Resolve handler
const handler = Registry.getInstance().resolve('orderHandler');
```

### Domain Events (TAS-112)
```typescript
import { BasePublisher, StepEventContext } from '@/events';

export class OrderCompletedPublisher extends BasePublisher {
  subscribesTo(): string {
    return 'order.completed';
  }

  async publish(ctx: StepEventContext): Promise<void> {
    await this.notifyDownstream(ctx);
  }

  // Lifecycle hooks
  beforePublish(ctx: StepEventContext): void {
    console.log(`Publishing order event for ${ctx.stepUuid}`);
  }

  afterPublish(ctx: StepEventContext, result: unknown): void {
    metrics.increment('order.completed.published');
  }

  onPublishError(ctx: StepEventContext, error: Error): void {
    console.error(`Failed to publish: ${error.message}`);
  }
}
```

---

## References

- [TypeScript Handbook](https://www.typescriptlang.org/docs/handbook/)
- [Biome Documentation](https://biomejs.dev/)
- [API Convergence Matrix](../../workers/api-convergence-matrix.md)
- [TypeScript Worker Documentation](../../workers/typescript.md)
- [TypeScript Worker AGENTS.md](../../../workers/typescript/AGENTS.md)
- [Composition Over Inheritance](../../principles/composition-over-inheritance.md)
