# TypeScript Worker

**Last Updated**: 2026-01-01
**Audience**: TypeScript/JavaScript Developers
**Status**: Active
**Package**: `tasker-worker-ts`
**Related Docs**: [Patterns and Practices](patterns-and-practices.md) | [Worker Event Systems](../worker-event-systems.md) | [API Convergence Matrix](api-convergence-matrix.md)
**Related Tickets**: TAS-112 (Domain Events, Mixin Pattern)

<- Back to [Worker Crates Overview](README.md)

---

The TypeScript worker provides a multi-runtime interface for integrating tasker-core workflow execution into TypeScript/JavaScript applications. It supports Bun, Node.js, and Deno runtimes with unified FFI bindings to the Rust worker foundation.

## Quick Start

### Installation

```bash
cd workers/typescript
bun install                     # Install dependencies
cargo build --release -p tasker-worker-ts  # Build FFI library
```

### Running the Server

```bash
# With Bun (recommended for production)
bun run bin/server.ts

# With Node.js
npx tsx bin/server.ts

# With Deno
deno run --allow-ffi --allow-env --allow-net bin/server.ts
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `DATABASE_URL` | PostgreSQL connection string | Required |
| `TASKER_ENV` | Environment (test/development/production) | development |
| `TASKER_CONFIG_PATH` | Path to TOML configuration | Auto-detected |
| `TASKER_TEMPLATE_PATH` | Path to task templates | Auto-detected |
| `TASKER_FFI_LIBRARY_PATH` | Path to `libtasker_worker` | Auto-detected |
| `RUST_LOG` | Log level (trace/debug/info/warn/error) | info |
| `PORT` | HTTP server port | 8081 |

---

## Architecture

### Server Mode

**Location**: `workers/typescript/bin/server.ts`

The server bootstraps the Rust foundation and manages TypeScript handler execution:

```typescript
import { createRuntime } from '../src/ffi/index.js';
import { EventEmitter } from '../src/events/event-emitter.js';
import { EventPoller } from '../src/events/event-poller.js';
import { HandlerRegistry } from '../src/handler/registry.js';
import { StepExecutionSubscriber } from '../src/subscriber/step-execution-subscriber.js';

// Create runtime for current environment (Bun/Node/Deno)
const runtime = createRuntime();
await runtime.load(libraryPath);

// Bootstrap Rust worker foundation
const result = runtime.bootstrapWorker({ namespace: 'my-app' });

// Create event system
const emitter = new EventEmitter();
const registry = new HandlerRegistry();

// Register handlers
registry.register('process_order', ProcessOrderHandler);

// Create step execution subscriber
const subscriber = new StepExecutionSubscriber(
  emitter,
  registry,
  runtime,
  { workerId: 'typescript-worker-001' }
);
subscriber.start();

// Start event poller (10ms polling)
const poller = new EventPoller(runtime, emitter, {
  pollingIntervalMs: 10
});
poller.start();

// Wait for shutdown signal
await shutdownSignal;

// Graceful shutdown
poller.stop();
await subscriber.waitForCompletion();
runtime.stopWorker();
```

### Headless/Embedded Mode

For embedding in existing TypeScript applications:

```typescript
import { createRuntime } from 'tasker-worker-ts';
import { EventEmitter, EventPoller, HandlerRegistry, StepExecutionSubscriber } from 'tasker-worker-ts';

// Bootstrap worker (headless mode via TOML: web.enabled = false)
const runtime = createRuntime();
await runtime.load('/path/to/libtasker_worker.dylib');
runtime.bootstrapWorker({ namespace: 'my-app' });

// Register handlers
const registry = new HandlerRegistry();
registry.register('process_data', ProcessDataHandler);

// Start event system
const emitter = new EventEmitter();
const subscriber = new StepExecutionSubscriber(emitter, registry, runtime, {});
subscriber.start();

const poller = new EventPoller(runtime, emitter);
poller.start();
```

### FFI Bridge

TypeScript communicates with the Rust foundation via FFI polling:

```
┌────────────────────────────────────────────────────────────────┐
│                  TYPESCRIPT FFI BRIDGE                          │
└────────────────────────────────────────────────────────────────┘

   Rust Worker System
          │
          │ FFI (pollStepEvents)
          ▼
   ┌─────────────────────┐
   │    EventPoller      │
   │  (setInterval)      │──→ poll every 10ms
   └─────────────────────┘
          │
          │ emit to EventEmitter
          ▼
   ┌─────────────────────┐
   │ StepExecution       │
   │ Subscriber          │──→ route to handler
   └─────────────────────┘
          │
          │ handler.call(context)
          ▼
   ┌─────────────────────┐
   │  Handler Execution  │
   └─────────────────────┘
          │
          │ FFI (completeStepEvent)
          ▼
   Rust Completion Channel
```

### Multi-Runtime Support

| Runtime | FFI Library | Status |
|---------|-------------|--------|
| **Bun** | koffi | Production |
| **Node.js** | koffi | Production |
| **Deno** | Deno.dlopen | Production |

---

## Handler Development

### Base Handler

**Location**: `workers/typescript/src/handler/base.ts`

All handlers extend `StepHandler`:

```typescript
import { StepHandler } from 'tasker-worker-ts';
import type { StepContext, StepHandlerResult } from 'tasker-worker-ts';

export class ProcessOrderHandler extends StepHandler {
  static handlerName = 'process_order';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // Access input data
    const orderId = context.getInput<string>('order_id');
    const amount = context.getInput<number>('amount');

    // Business logic
    const result = await this.processOrder(orderId, amount);

    // Return success
    return this.success({
      order_id: orderId,
      status: 'processed',
      total: result.total
    });
  }

  private async processOrder(orderId: string, amount: number) {
    // Implementation
    return { total: amount * 1.1 };
  }
}
```

### Handler Signature

```typescript
async call(context: StepContext): Promise<StepHandlerResult>

// StepContext provides:
context.taskUuid          // Task identifier
context.stepUuid          // Step identifier
context.stepInputs        // Runtime inputs
context.stepConfig        // Handler configuration
context.dependencyResults // Results from parent steps
context.taskContext       // Full task context
context.retryCount        // Current retry attempt

// Type-safe accessors:
context.getInput<T>(key)              // Get single input
context.getDependencyResult(stepName) // Get dependency result
context.getAllDependencyResults(name) // Get all instances (batch workers)
```

### Result Methods

```typescript
// Success result (from base class)
return this.success(
  { key: 'value' },           // result
  { duration_ms: 100 }        // metadata (optional)
);

// Failure result (from base class)
return this.failure(
  'Payment declined',         // message
  'payment_error',            // errorType
  true,                       // retryable
  { card_last_four: '1234' }  // metadata (optional)
);
```

### Error Types

```typescript
import { ErrorType } from 'tasker-worker-ts';

ErrorType.PERMANENT_ERROR   // Non-retryable failures
ErrorType.RETRYABLE_ERROR   // Retryable failures
ErrorType.VALIDATION_ERROR  // Input validation failures
ErrorType.HANDLER_ERROR     // Handler execution failures
```

### Accessing Dependencies

```typescript
async call(context: StepContext): Promise<StepHandlerResult> {
  // Get result from a dependency step
  const validation = context.getDependencyResult('validate_order') as {
    valid: boolean;
    amount: number;
  } | null;

  if (!validation) {
    return this.failure('Missing validation result', 'dependency_error', false);
  }

  if (validation.valid) {
    return this.success({ processed: true, amount: validation.amount });
  }

  return this.failure('Validation failed', 'validation_error', false);
}
```

---

## Specialized Handlers

### Mixin Pattern (TAS-112)

TypeScript uses composition via mixins rather than inheritance. You can use either:
1. **Wrapper classes** (ApiHandler, DecisionHandler) - simpler, backward compatible
2. **Mixin functions** (applyAPI, applyDecision) - explicit composition

```typescript
import { StepHandler } from 'tasker-worker-ts';
import { applyAPI, APICapable } from 'tasker-worker-ts';

// Using mixin pattern (recommended for new code)
class MyHandler extends StepHandler implements APICapable {
  constructor() {
    super();
    applyAPI(this);  // Adds get/post/put/delete methods
  }

  async call(context: StepContext): Promise<StepHandlerResult> {
    const response = await this.get('/api/data');
    return this.apiSuccess(response);
  }
}

// Or using wrapper class (simpler, backward compatible)
import { ApiHandler } from 'tasker-worker-ts';

class MyHandler extends ApiHandler {
  async call(context: StepContext): Promise<StepHandlerResult> {
    const response = await this.get('/api/data');
    return this.apiSuccess(response);
  }
}
```

### API Handler

**Location**: `workers/typescript/src/handler/api.ts`

For HTTP API integration with automatic error classification:

```typescript
import { ApiHandler } from 'tasker-worker-ts';
import type { StepContext, StepHandlerResult } from 'tasker-worker-ts';

export class FetchUserHandler extends ApiHandler {
  static handlerName = 'fetch_user';
  static handlerVersion = '1.0.0';

  protected baseUrl = 'https://api.example.com';

  async call(context: StepContext): Promise<StepHandlerResult> {
    const userId = context.getInput<string>('user_id');

    // Automatic error classification
    const response = await this.get(`/users/${userId}`);

    if (!response.ok) {
      return this.apiFailure(response);
    }

    return this.apiSuccess(response);
  }
}
```

**HTTP Methods**:

```typescript
// GET request
const response = await this.get('/path', {
  params: { key: 'value' },
  headers: { 'Authorization': 'Bearer token' }
});

// POST request
const response = await this.post('/path', {
  body: { key: 'value' },
  headers: {}
});

// PUT request
const response = await this.put('/path', { body: { key: 'value' } });

// DELETE request
const response = await this.delete('/path', { params: {} });
```

**ApiResponse Properties**:

```typescript
response.statusCode      // HTTP status code
response.headers         // Response headers
response.body            // Parsed body (object or string)
response.ok              // True if 2xx status
response.isClientError   // True if 4xx status
response.isServerError   // True if 5xx status
response.isRetryable     // True if should retry (408, 429, 500-504)
response.retryAfter      // Retry-After header value in seconds
```

**Error Classification**:

| Status | Classification | Behavior |
|--------|---------------|----------|
| 400, 401, 403, 404, 422 | Non-retryable | Permanent failure |
| 408, 429, 500-504 | Retryable | Standard retry |

### Decision Handler

**Location**: `workers/typescript/src/handler/decision.ts`

For dynamic workflow routing:

```typescript
import { DecisionHandler } from 'tasker-worker-ts';
import type { StepContext, StepHandlerResult } from 'tasker-worker-ts';

export class RoutingDecisionHandler extends DecisionHandler {
  static handlerName = 'routing_decision';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    const amount = context.getInput<number>('amount') ?? 0;

    if (amount < 1000) {
      // Auto-approve small amounts
      return this.decisionSuccess(['auto_approve'], {
        route_type: 'auto',
        amount
      });
    } else if (amount < 5000) {
      // Manager approval for medium amounts
      return this.decisionSuccess(['manager_approval'], {
        route_type: 'manager',
        amount
      });
    } else {
      // Dual approval for large amounts
      return this.decisionSuccess(['manager_approval', 'finance_review'], {
        route_type: 'dual',
        amount
      });
    }
  }
}
```

**Decision Methods**:

```typescript
// Activate specific steps
return this.decisionSuccess(
  ['step1', 'step2'],           // steps to activate
  { route_reason: 'threshold' } // routing context
);

// No branches needed
return this.decisionNoBranches('condition not met');
```

### BatchableStepHandler

**Location**: `workers/typescript/src/handler/batchable.ts`

For processing large datasets in chunks. Cross-language aligned with Ruby and Python implementations.

**Analyzer Handler** (creates batch configurations):

```typescript
import { BatchableStepHandler } from 'tasker-worker-ts';
import type { StepContext, BatchableResult } from 'tasker-worker-ts';

export class CsvAnalyzerHandler extends BatchableStepHandler {
  static handlerName = 'csv_analyzer';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<BatchableResult> {
    const csvPath = context.getInput<string>('csv_path');
    const rowCount = await this.countCsvRows(csvPath);

    if (rowCount === 0) {
      // No data to process - use cross-language standard
      return this.noBatchesResult('empty_dataset', {
        csv_path: csvPath,
        analyzed_at: new Date().toISOString()
      });
    }

    // Create cursor configs using Ruby-style helper
    // Divides rowCount into 5 roughly equal batches
    const batchConfigs = this.createCursorConfigs(rowCount, 5);

    return this.batchSuccess('process_csv_batch', batchConfigs, {
      csv_path: csvPath,
      total_rows: rowCount,
      analyzed_at: new Date().toISOString()
    });
  }
}
```

**Worker Handler** (processes a batch):

```typescript
export class CsvBatchProcessorHandler extends BatchableStepHandler {
  static handlerName = 'csv_batch_processor';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // Cross-language standard: check for no-op worker first
    const noOpResult = this.handleNoOpWorker(context);
    if (noOpResult) {
      return noOpResult;
    }

    // Get batch worker inputs from Rust
    const batchInputs = this.getBatchWorkerInputs(context);
    const cursor = batchInputs?.cursor;

    if (!cursor) {
      return this.failure('Missing batch cursor', 'batch_error', false);
    }

    // Process the batch
    const results = await this.processCsvBatch(
      cursor.start_cursor,
      cursor.end_cursor
    );

    return this.success({
      batch_id: cursor.batch_id,
      rows_processed: results.count,
      items_succeeded: results.success,
      items_failed: results.failed
    });
  }
}
```

**Aggregator Handler** (combines results):

```typescript
export class CsvAggregatorHandler extends StepHandler {
  static handlerName = 'csv_aggregator';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // Get all batch worker results
    const workerResults = context.getAllDependencyResults('process_csv_batch') as Array<{
      rows_processed: number;
      items_succeeded: number;
      items_failed: number;
    } | null>;

    // Aggregate results
    let totalProcessed = 0;
    let totalSucceeded = 0;
    let totalFailed = 0;

    for (const result of workerResults) {
      if (result) {
        totalProcessed += result.rows_processed ?? 0;
        totalSucceeded += result.items_succeeded ?? 0;
        totalFailed += result.items_failed ?? 0;
      }
    }

    return this.success({
      total_processed: totalProcessed,
      total_succeeded: totalSucceeded,
      total_failed: totalFailed,
      worker_count: workerResults.length
    });
  }
}
```

**BatchableStepHandler Methods (Cross-Language Aligned)**:

| Method | Ruby Equivalent | Purpose |
|--------|-----------------|---------|
| `batchSuccess(template, configs, metadata)` | `batch_success` | Create batch workers |
| `noBatchesResult(reason, metadata)` | `no_batches_outcome` | Empty dataset handling |
| `createCursorConfigs(total, workers)` | `create_cursor_configs` | Divide work by worker count |
| `handleNoOpWorker(context)` | `handle_no_op_worker` | Detect no-op placeholders |
| `getBatchWorkerInputs(context)` | `get_batch_context` | Access Rust batch inputs |
| `aggregateWorkerResults(results)` | `aggregate_batch_worker_results` | Static aggregation helper |

---

## Handler Registry

### Registration

**Location**: `workers/typescript/src/handler/registry.ts`

```typescript
import { HandlerRegistry } from 'tasker-worker-ts';

const registry = new HandlerRegistry();

// Manual registration
registry.register('process_order', ProcessOrderHandler);

// Check if registered
registry.isRegistered('process_order'); // true

// Resolve and instantiate
const handler = registry.resolve('process_order');
if (handler) {
  const result = await handler.call(context);
}

// List all handlers
registry.listHandlers(); // ['process_order', ...]

// Handler count
registry.handlerCount(); // 1
```

### Bulk Registration

```typescript
import { registerExampleHandlers } from './handlers/examples/index.js';

// Register multiple handlers at once
registerExampleHandlers(registry);
```

---

## Type System

### Core Types

```typescript
import type {
  StepContext,
  StepHandlerResult,
  BatchableResult,
  FfiStepEvent,
  BootstrapConfig,
  WorkerStatus,
} from 'tasker-worker-ts';

// StepContext - created from FFI event
const context = StepContext.fromFfiEvent(event, 'handler_name');
context.taskUuid;      // string
context.stepUuid;      // string
context.stepInputs;    // Record<string, unknown>
context.retryCount;    // number

// StepHandlerResult - handler output
result.success;        // boolean
result.result;         // Record<string, unknown>
result.errorMessage;   // string | undefined
result.retryable;      // boolean
```

### Configuration Types

```typescript
import type { BootstrapConfig } from 'tasker-worker-ts';

const config: BootstrapConfig = {
  namespace: 'my-app',
  environment: 'production',
  configPath: '/path/to/config.toml'
};
```

---

## Event System

### EventEmitter

**Location**: `workers/typescript/src/events/event-emitter.ts`

```typescript
import { EventEmitter } from 'tasker-worker-ts';
import { StepEventNames } from 'tasker-worker-ts';

const emitter = new EventEmitter();

// Subscribe to events
emitter.on(StepEventNames.STEP_EXECUTION_RECEIVED, (payload) => {
  console.log(`Processing step: ${payload.event.step_uuid}`);
});

emitter.on(StepEventNames.STEP_EXECUTION_COMPLETED, (payload) => {
  console.log(`Step completed: ${payload.stepUuid}`);
});

// Emit events
emitter.emit(StepEventNames.STEP_EXECUTION_RECEIVED, {
  event: ffiStepEvent
});
```

### Event Names

```typescript
import { StepEventNames } from 'tasker-worker-ts';

StepEventNames.STEP_EXECUTION_RECEIVED  // Step event received from FFI
StepEventNames.STEP_EXECUTION_STARTED   // Handler execution started
StepEventNames.STEP_EXECUTION_COMPLETED // Handler execution completed
StepEventNames.STEP_EXECUTION_FAILED    // Handler execution failed
StepEventNames.STEP_COMPLETION_SENT     // Result sent to FFI
```

### EventPoller

**Location**: `workers/typescript/src/events/event-poller.ts`

```typescript
import { EventPoller } from 'tasker-worker-ts';

const poller = new EventPoller(runtime, emitter, {
  pollingIntervalMs: 10,        // Poll every 10ms
  starvationCheckInterval: 100, // Check every 1 second
  cleanupInterval: 1000         // Cleanup every 10 seconds
});

// Start polling
poller.start();

// Get metrics
const metrics = poller.getMetrics();
console.log(`Pending: ${metrics.pendingCount}`);

// Stop polling
poller.stop();
```

---

## Domain Events (TAS-112)

TypeScript has full domain event support, matching Ruby and Python capabilities. The domain events module provides BasePublisher, BaseSubscriber, and registries for custom event handling.

**Location**: `workers/typescript/src/handler/domain-events.ts`

### BasePublisher

Publishers transform step execution context into domain-specific events:

```typescript
import { BasePublisher, StepEventContext, DomainEvent } from 'tasker-worker-ts';

export class PaymentEventPublisher extends BasePublisher {
  static publisherName = 'payment_events';

  // Required: which steps trigger this publisher
  publishesFor(): string[] {
    return ['process_payment', 'refund_payment'];
  }

  // Transform step context into domain event
  async transformPayload(ctx: StepEventContext): Promise<Record<string, unknown>> {
    return {
      payment_id: ctx.result?.payment_id,
      amount: ctx.result?.amount,
      currency: ctx.result?.currency,
      status: ctx.result?.status
    };
  }

  // Lifecycle hooks (optional)
  async beforePublish(ctx: StepEventContext): Promise<void> {
    console.log(`Publishing payment event for step: ${ctx.stepName}`);
  }

  async afterPublish(ctx: StepEventContext, event: DomainEvent): Promise<void> {
    console.log(`Published event: ${event.eventName}`);
  }

  async onPublishError(ctx: StepEventContext, error: Error): Promise<void> {
    console.error(`Failed to publish: ${error.message}`);
  }

  // Inject custom metadata
  async additionalMetadata(ctx: StepEventContext): Promise<Record<string, unknown>> {
    return { payment_processor: 'stripe' };
  }
}
```

### BaseSubscriber

Subscribers react to domain events matching specific patterns:

```typescript
import { BaseSubscriber, InProcessDomainEvent, SubscriberResult } from 'tasker-worker-ts';

export class AuditLoggingSubscriber extends BaseSubscriber {
  static subscriberName = 'audit_logger';

  // Which events to handle (glob patterns supported)
  subscribesTo(): string[] {
    return ['payment.*', 'order.completed'];
  }

  // Handle matching events
  async handle(event: InProcessDomainEvent): Promise<SubscriberResult> {
    await this.logToAuditTrail(event);
    return { success: true };
  }

  // Lifecycle hooks (optional)
  async beforeHandle(event: InProcessDomainEvent): Promise<void> {
    console.log(`Handling: ${event.eventName}`);
  }

  async afterHandle(event: InProcessDomainEvent, result: SubscriberResult): Promise<void> {
    console.log(`Handled successfully: ${result.success}`);
  }

  async onHandleError(event: InProcessDomainEvent, error: Error): Promise<void> {
    console.error(`Handler error: ${error.message}`);
  }
}
```

### Registries

Manage publishers and subscribers with singleton registries:

```typescript
import { PublisherRegistry, SubscriberRegistry } from 'tasker-worker-ts';

// Publisher Registry
const pubRegistry = PublisherRegistry.getInstance();
pubRegistry.register(PaymentEventPublisher);
pubRegistry.register(OrderEventPublisher);
pubRegistry.freeze(); // Prevent further registrations

// Get publisher for a step
const publisher = pubRegistry.getForStep('process_payment');

// Subscriber Registry
const subRegistry = SubscriberRegistry.getInstance();
subRegistry.register(AuditLoggingSubscriber);
subRegistry.register(MetricsSubscriber);

// Start all subscribers
subRegistry.startAll();

// Stop all subscribers
subRegistry.stopAll();
```

### FFI Integration

Domain events integrate with the Rust FFI layer for cross-language event flow:

```typescript
import { createFfiPollAdapter, InProcessDomainEventPoller } from 'tasker-worker-ts';

// Create poller connected to Rust broadcast channel
const poller = new InProcessDomainEventPoller();

// Set the FFI poll function
poller.setPollFunction(createFfiPollAdapter(runtime));

// Start polling for events
poller.start((event) => {
  // Route to appropriate subscriber
  const subscribers = subRegistry.getMatchingSubscribers(event.eventName);
  for (const sub of subscribers) {
    sub.handle(event);
  }
});
```

---

## Signal Handling

The TypeScript worker handles signals for graceful shutdown:

| Signal | Behavior |
|--------|----------|
| `SIGTERM` | Graceful shutdown |
| `SIGINT` | Graceful shutdown (Ctrl+C) |

```typescript
import { ShutdownController } from 'tasker-worker-ts';

const shutdown = new ShutdownController();

// Register signal handlers
shutdown.registerSignalHandlers();

// Wait for shutdown signal
await shutdown.waitForShutdown();

// Or check if shutdown requested
if (shutdown.isShutdownRequested()) {
  // Begin cleanup
}
```

---

## Error Handling

### Using Failure Results

```typescript
async call(context: StepContext): Promise<StepHandlerResult> {
  try {
    const result = await this.processData(context);
    return this.success(result);
  } catch (error) {
    if (error instanceof NetworkError) {
      // Retryable error
      return this.failure(
        error.message,
        ErrorType.RETRYABLE_ERROR,
        true,
        { endpoint: error.endpoint }
      );
    }

    // Non-retryable error
    return this.failure(
      error instanceof Error ? error.message : 'Unknown error',
      ErrorType.HANDLER_ERROR,
      false
    );
  }
}
```

---

## Logging

### Structured Logging

```typescript
import { logInfo, logError, logWarn, logDebug } from 'tasker-worker-ts';

// Simple logging
logInfo('Processing started', { component: 'handler' });
logError('Failed to connect', { component: 'database' });

// With additional context
logInfo('Order processed', {
  component: 'handler',
  order_id: '123',
  amount: '100.00'
});
```

### Pino Integration

The worker uses [pino](https://github.com/pinojs/pino) for structured logging:

```typescript
import pino from 'pino';

const logger = pino({
  name: 'my-handler',
  level: process.env.RUST_LOG ?? 'info'
});

logger.info({ orderId: '123' }, 'Processing order');
```

---

## File Structure

```
workers/typescript/
├── bin/
│   └── server.ts               # Production server
├── src/
│   ├── index.ts                # Package exports
│   ├── bootstrap/
│   │   └── bootstrap.ts        # Worker initialization
│   ├── events/
│   │   ├── event-emitter.ts    # Event pub/sub
│   │   ├── event-poller.ts     # FFI polling
│   │   └── event-system.ts     # Combined event system
│   ├── ffi/
│   │   ├── bun-runtime.ts      # Bun FFI adapter
│   │   ├── node-runtime.ts     # Node.js FFI adapter
│   │   ├── deno-runtime.ts     # Deno FFI adapter
│   │   ├── runtime-interface.ts # Common interface
│   │   └── types.ts            # FFI types
│   ├── handler/
│   │   ├── base.ts             # Base handler class
│   │   ├── api.ts              # API handler
│   │   ├── decision.ts         # Decision handler
│   │   ├── batchable.ts        # Batchable handler
│   │   ├── domain-events.ts    # Domain events module (TAS-112)
│   │   ├── registry.ts         # Handler registry
│   │   └── mixins/             # Mixin modules (TAS-112)
│   │       ├── index.ts        # Mixin exports
│   │       ├── api.ts          # APIMixin, applyAPI
│   │       └── decision.ts     # DecisionMixin, applyDecision
│   ├── server/
│   │   ├── worker-server.ts    # Server implementation
│   │   └── types.ts            # Server types
│   ├── subscriber/
│   │   └── step-execution-subscriber.ts
│   └── types/
│       ├── step-context.ts     # Step context
│       └── step-handler-result.ts
├── tests/
│   ├── unit/                   # Unit tests
│   ├── integration/            # Integration tests
│   └── handlers/examples/      # Example handlers
├── src-rust/                   # Rust FFI extension
├── package.json
├── tsconfig.json
└── biome.json                  # Linting config
```

---

## Testing

### Unit Tests

```bash
cd workers/typescript
bun test                        # Run all tests
bun test tests/unit/            # Run unit tests only
```

### Integration Tests

```bash
bun test tests/integration/     # Run integration tests
```

### With Coverage

```bash
bun test --coverage
```

### Linting

```bash
bun run check                   # Biome lint + format check
bun run check:fix               # Auto-fix issues
```

### Type Checking

```bash
bunx tsc --noEmit               # Type check without emit
```

---

## Example Handlers

### Linear Workflow

```typescript
export class DoubleHandler extends StepHandler {
  static handlerName = 'double_value';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    const value = context.getInput<number>('value') ?? 0;
    return this.success({
      result: value * 2,
      operation: 'double'
    });
  }
}

export class AddHandler extends StepHandler {
  static handlerName = 'add_constant';
  static handlerVersion = '1.0.0';

  async call(context: StepContext): Promise<StepHandlerResult> {
    const prev = context.getDependencyResult('double_value') as { result: number } | null;
    const value = prev?.result ?? 0;
    return this.success({
      result: value + 10,
      operation: 'add'
    });
  }
}
```

### Diamond Workflow (Parallel Branches)

```typescript
export class DiamondStartHandler extends StepHandler {
  static handlerName = 'diamond_start';

  async call(context: StepContext): Promise<StepHandlerResult> {
    const input = context.getInput<number>('value') ?? 0;
    return this.success({ squared: input * input });
  }
}

export class BranchBHandler extends StepHandler {
  static handlerName = 'branch_b';

  async call(context: StepContext): Promise<StepHandlerResult> {
    const start = context.getDependencyResult('diamond_start') as { squared: number };
    return this.success({ result: start.squared + 25 });
  }
}

export class BranchCHandler extends StepHandler {
  static handlerName = 'branch_c';

  async call(context: StepContext): Promise<StepHandlerResult> {
    const start = context.getDependencyResult('diamond_start') as { squared: number };
    return this.success({ result: start.squared * 2 });
  }
}

export class DiamondEndHandler extends StepHandler {
  static handlerName = 'diamond_end';

  async call(context: StepContext): Promise<StepHandlerResult> {
    const branchB = context.getDependencyResult('branch_b') as { result: number };
    const branchC = context.getDependencyResult('branch_c') as { result: number };
    return this.success({
      final: (branchB.result + branchC.result) / 2
    });
  }
}
```

### Error Handling

```typescript
export class RetryableErrorHandler extends StepHandler {
  static handlerName = 'retryable_error';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // Simulate a retryable error (e.g., network timeout)
    return this.failure(
      'Connection timeout - will be retried',
      ErrorType.RETRYABLE_ERROR,
      true,
      { attempt: context.retryCount }
    );
  }
}

export class PermanentErrorHandler extends StepHandler {
  static handlerName = 'permanent_error';

  async call(context: StepContext): Promise<StepHandlerResult> {
    // Simulate a permanent error (e.g., validation failure)
    return this.failure(
      'Invalid input - no retry allowed',
      ErrorType.PERMANENT_ERROR,
      false
    );
  }
}
```

---

## Docker Deployment

### Dockerfile

```dockerfile
FROM oven/bun:1.1.38 AS runtime

WORKDIR /app

# Copy built artifacts
COPY workers/typescript/dist/ ./dist/
COPY workers/typescript/package.json ./
COPY target/release/libtasker_worker.dylib ./lib/

# Install production dependencies
RUN bun install --production

# Set environment
ENV TASKER_FFI_LIBRARY_PATH=/app/lib/libtasker_worker.dylib
ENV PORT=8081

EXPOSE 8081

CMD ["bun", "run", "dist/bin/server.js"]
```

### Docker Compose

```yaml
typescript-worker:
  build:
    context: .
    dockerfile: docker/build/typescript-worker.Dockerfile
  environment:
    DATABASE_URL: postgresql://tasker:tasker@postgres:5432/tasker
    TASKER_ENV: production
    TASKER_TEMPLATE_PATH: /app/templates
    PORT: 8081
  ports:
    - "8084:8081"
  healthcheck:
    test: ["CMD", "curl", "-f", "http://localhost:8081/health"]
    interval: 10s
    timeout: 5s
    retries: 3
```

---

## See Also

- [Worker Crates Overview](README.md) - High-level introduction
- [Patterns and Practices](patterns-and-practices.md) - Common patterns
- [Python Worker](python.md) - Python implementation
- [Ruby Worker](ruby.md) - Ruby implementation
- [Worker Event Systems](../worker-event-systems.md) - Architecture details
