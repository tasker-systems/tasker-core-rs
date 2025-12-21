# TAS-103: Specialized Handlers

**Priority**: Medium  
**Estimated Effort**: 1-2 days  
**Dependencies**: TAS-102 (Handler API and Registry)  
**Parent**: [TAS-100](./README.md)  
**Linear**: [TAS-103](https://linear.app/tasker-systems/issue/TAS-103)  
**Branch**: `jcoletaylor/tas-103-typescript-specialized-handlers`  
**Status**: Detailed Specification

---

## Objective

Implement specialized handler base classes that extend `StepHandler` for common patterns: HTTP API interactions (`ApiHandler`), workflow routing decisions (`DecisionHandler`), and batch processing (`Batchable` mixin).

**Key Goal**: Provide TypeScript developers with the same specialized handler capabilities available in Python/Ruby, following TAS-92 cross-language API standards.

---

## TAS-92 API Alignment Requirements

All specialized handlers **MUST** align with TAS-92 standards:

| Handler Type | Requirements |
|--------------|--------------|
| **ApiHandler** | HTTP methods: `get()`, `post()`, `put()`, `delete()` |
| **ApiHandler** | Auto error classification: 4xx → permanent, 5xx/429/408 → retryable |
| **ApiHandler** | Result helpers: `apiSuccess()`, `apiFailure()` |
| **DecisionHandler** | Simplified helper: `decisionSuccess(steps, routingContext?)` |
| **DecisionHandler** | Skip helper: `skipBranches(reason)` |
| **Batchable** | Standard fields: `items_processed`, `items_succeeded`, `items_failed` |
| **Batchable** | Cursor fields: `start_cursor`, `end_cursor`, `batch_size` |

---

## Reference Implementations

### Python (Most Recent - TAS-92/TAS-95)
- `workers/python/python/tasker_core/step_handler/api.py` - ApiHandler with httpx
- `workers/python/python/tasker_core/step_handler/decision.py` - DecisionHandler
- `workers/python/python/tasker_core/batch_processing/batchable.py` - Batchable mixin
- `workers/python/python/tasker_core/types.py` - Batch processing types (lines 1230-1450)

### Ruby (TAS-96)
- `workers/ruby/lib/tasker_core/step_handler/api.rb` - ApiHandler
- `workers/ruby/lib/tasker_core/step_handler/decision.rb` - DecisionHandler
- `workers/ruby/lib/tasker_core/step_handler/batchable.rb` - Batchable module

---

## Architecture Overview

### Specialized Handler Hierarchy

```
StepHandler (base)
    │
    ├── ApiHandler
    │   └── Uses fetch API for HTTP
    │
    ├── DecisionHandler
    │   └── Builds DecisionPointOutcome structures
    │
    └── Batchable (mixin)
        ├── Analyzer pattern
        └── Worker pattern
```

---

## Detailed Component Specifications

### 1. ApiHandler - HTTP API Integration

**File**: `src/handlers/ApiHandler.ts`

**Purpose**: Base class for handlers that interact with HTTP/REST APIs.

**Python reference**: `workers/python/python/tasker_core/step_handler/api.py` (lines 141-589)

```typescript
import { StepHandler } from './StepHandler';
import { StepContext } from '../types/StepContext';
import { StepHandlerResult } from '../types/StepHandlerResult';
import { ErrorType } from '../types/ErrorType';

/**
 * HTTP status code classifications for automatic error handling.
 */
const CLIENT_ERROR_CODES = new Set(
  Array.from({ length: 100 }, (_, i) => i + 400)
);

const SERVER_ERROR_CODES = new Set(
  Array.from({ length: 100 }, (_, i) => i + 500)
);

/**
 * Status codes that should not be retried (permanent errors).
 */
const NON_RETRYABLE_STATUS_CODES = new Set([
  400, // Bad Request
  401, // Unauthorized
  403, // Forbidden
  404, // Not Found
  405, // Method Not Allowed
  406, // Not Acceptable
  410, // Gone
  422, // Unprocessable Entity
]);

/**
 * Status codes that indicate temporary failures (should retry).
 */
const RETRYABLE_STATUS_CODES = new Set([
  408, // Request Timeout
  429, // Too Many Requests (rate limit)
  500, // Internal Server Error
  502, // Bad Gateway
  503, // Service Unavailable
  504, // Gateway Timeout
]);

/**
 * Response wrapper for API calls.
 * 
 * Provides convenient access to response data and error classification.
 */
export class ApiResponse {
  public readonly statusCode: number;
  public readonly headers: Record<string, string>;
  public readonly body: any;
  public readonly rawResponse: Response;

  constructor(response: Response, body?: any) {
    this.statusCode = response.status;
    this.headers = Object.fromEntries(response.headers.entries());
    this.rawResponse = response;
    this.body = body;
  }

  /**
   * Check if the response indicates success (2xx status).
   */
  get ok(): boolean {
    return this.statusCode >= 200 && this.statusCode < 300;
  }

  /**
   * Check if the response indicates a client error (4xx status).
   */
  get isClientError(): boolean {
    return CLIENT_ERROR_CODES.has(this.statusCode);
  }

  /**
   * Check if the response indicates a server error (5xx status).
   */
  get isServerError(): boolean {
    return SERVER_ERROR_CODES.has(this.statusCode);
  }

  /**
   * Check if the error should be retried.
   */
  get isRetryable(): boolean {
    return RETRYABLE_STATUS_CODES.has(this.statusCode);
  }

  /**
   * Get the Retry-After header value in seconds, if present.
   */
  get retryAfter(): number | null {
    const retryAfter = this.headers['retry-after'];
    if (!retryAfter) {
      return null;
    }
    const parsed = parseInt(retryAfter, 10);
    return isNaN(parsed) ? 60 : parsed; // Default to 60s if not a number
  }

  /**
   * Convert the response to a dictionary for result output.
   */
  toDict(): Record<string, any> {
    return {
      status_code: this.statusCode,
      headers: this.headers,
      body: this.body,
    };
  }
}

/**
 * Base class for HTTP API step handlers.
 * 
 * Provides HTTP client functionality with automatic error classification,
 * retry handling, and convenient methods for common HTTP operations.
 * 
 * Uses native fetch API (available in Bun and Node.js 18+).
 * 
 * @example
 * class PaymentApiHandler extends ApiHandler {
 *   static handlerName = 'process_payment';
 *   static baseUrl = 'https://payments.example.com/api/v1';
 *   static defaultHeaders = { 'X-API-Key': 'secret' };
 * 
 *   async call(context: StepContext): Promise<StepHandlerResult> {
 *     const paymentData = context.inputData['payment'];
 *     const response = await this.post('/payments', { body: paymentData });
 *     if (response.ok) {
 *       return this.apiSuccess(response);
 *     }
 *     return this.apiFailure(response);
 *   }
 * }
 */
export abstract class ApiHandler extends StepHandler {
  /** Base URL for API calls. Override in subclasses. */
  static baseUrl: string = '';

  /** Default request timeout in milliseconds. */
  static defaultTimeout: number = 30000;

  /** Default headers to include in all requests. */
  static defaultHeaders: Record<string, string> = {};

  get capabilities(): string[] {
    return ['process', 'http', 'api'];
  }

  /**
   * Get the base URL for this handler.
   */
  protected get baseUrl(): string {
    const ctor = this.constructor as typeof ApiHandler;
    return ctor.baseUrl;
  }

  /**
   * Get the default timeout for this handler.
   */
  protected get timeout(): number {
    const ctor = this.constructor as typeof ApiHandler;
    return ctor.defaultTimeout;
  }

  /**
   * Get the default headers for this handler.
   */
  protected get defaultHeaders(): Record<string, string> {
    const ctor = this.constructor as typeof ApiHandler;
    return ctor.defaultHeaders;
  }

  // =========================================================================
  // HTTP Methods
  // =========================================================================

  /**
   * Make a GET request.
   * 
   * @param path - URL path (appended to baseUrl)
   * @param params - Query parameters
   * @param headers - Additional headers
   * @returns ApiResponse wrapping the response
   * 
   * @example
   * const response = await this.get('/users', { page: 1 });
   * if (response.ok) {
   *   const users = response.body.data;
   * }
   */
  protected async get(
    path: string,
    params?: Record<string, any>,
    headers?: Record<string, string>
  ): Promise<ApiResponse> {
    const url = this.buildUrl(path, params);
    const response = await this.fetch(url, {
      method: 'GET',
      headers: this.mergeHeaders(headers),
    });
    return response;
  }

  /**
   * Make a POST request.
   * 
   * @param path - URL path (appended to baseUrl)
   * @param options - Request options
   * @returns ApiResponse wrapping the response
   * 
   * @example
   * const response = await this.post('/users', {
   *   body: { name: 'Alice' }
   * });
   */
  protected async post(
    path: string,
    options?: {
      body?: any;
      json?: boolean;
      headers?: Record<string, string>;
    }
  ): Promise<ApiResponse> {
    const url = this.buildUrl(path);
    const body = this.prepareBody(options?.body, options?.json !== false);
    const response = await this.fetch(url, {
      method: 'POST',
      headers: this.mergeHeaders(options?.headers, options?.json !== false),
      body,
    });
    return response;
  }

  /**
   * Make a PUT request.
   */
  protected async put(
    path: string,
    options?: {
      body?: any;
      json?: boolean;
      headers?: Record<string, string>;
    }
  ): Promise<ApiResponse> {
    const url = this.buildUrl(path);
    const body = this.prepareBody(options?.body, options?.json !== false);
    const response = await this.fetch(url, {
      method: 'PUT',
      headers: this.mergeHeaders(options?.headers, options?.json !== false),
      body,
    });
    return response;
  }

  /**
   * Make a PATCH request.
   */
  protected async patch(
    path: string,
    options?: {
      body?: any;
      json?: boolean;
      headers?: Record<string, string>;
    }
  ): Promise<ApiResponse> {
    const url = this.buildUrl(path);
    const body = this.prepareBody(options?.body, options?.json !== false);
    const response = await this.fetch(url, {
      method: 'PATCH',
      headers: this.mergeHeaders(options?.headers, options?.json !== false),
      body,
    });
    return response;
  }

  /**
   * Make a DELETE request.
   */
  protected async delete(
    path: string,
    headers?: Record<string, string>
  ): Promise<ApiResponse> {
    const url = this.buildUrl(path);
    const response = await this.fetch(url, {
      method: 'DELETE',
      headers: this.mergeHeaders(headers),
    });
    return response;
  }

  /**
   * Make an arbitrary HTTP request.
   */
  protected async request(
    method: string,
    path: string,
    options?: RequestInit
  ): Promise<ApiResponse> {
    const url = this.buildUrl(path);
    const response = await this.fetch(url, {
      ...options,
      method,
      headers: this.mergeHeaders(options?.headers as Record<string, string>),
    });
    return response;
  }

  // =========================================================================
  // Result Helpers
  // =========================================================================

  /**
   * Create a success result from an API response.
   * 
   * @param response - The API response
   * @param result - Optional custom result data
   * @param includeResponse - Whether to include response metadata
   * @returns A success StepHandlerResult
   * 
   * @example
   * const response = await this.get('/data');
   * if (response.ok) {
   *   return this.apiSuccess(response);
   * }
   */
  protected apiSuccess(
    response: ApiResponse,
    result?: Record<string, any>,
    includeResponse: boolean = true
  ): StepHandlerResult {
    const resultData = result || 
      (typeof response.body === 'object' ? response.body : { data: response.body });

    const metadata: Record<string, any> = {};
    if (includeResponse) {
      metadata.status_code = response.statusCode;
      metadata.headers = response.headers;
    }

    return this.success(resultData, metadata);
  }

  /**
   * Create a failure result from an API response.
   * 
   * Automatically classifies the error type based on HTTP status code
   * and determines if the error is retryable.
   * 
   * @param response - The API response
   * @param message - Optional custom error message
   * @returns A failure StepHandlerResult with appropriate classification
   * 
   * @example
   * const response = await this.post('/payments', { body: data });
   * if (!response.ok) {
   *   return this.apiFailure(response);
   * }
   */
  protected apiFailure(
    response: ApiResponse,
    message?: string
  ): StepHandlerResult {
    const errorType = this.classifyError(response);
    const errorMessage = message || this.formatErrorMessage(response);
    const retryable = response.isRetryable;

    const metadata: Record<string, any> = {
      status_code: response.statusCode,
      headers: response.headers,
    };

    if (response.retryAfter !== null) {
      metadata.retry_after_seconds = response.retryAfter;
    }

    if (response.body) {
      metadata.response_body = response.body;
    }

    return this.failure(errorMessage, errorType, retryable, metadata);
  }

  /**
   * Create a failure result from a connection error.
   * 
   * Connection errors are typically retryable.
   */
  protected connectionError(
    error: Error,
    context?: string
  ): StepHandlerResult {
    let message = `Connection error: ${error.message}`;
    if (context) {
      message = `Connection error while ${context}: ${error.message}`;
    }

    return this.failure(
      message,
      'connection_error',
      true,
      { exception_type: error.constructor.name }
    );
  }

  /**
   * Create a failure result from a timeout error.
   * 
   * Timeout errors are typically retryable.
   */
  protected timeoutError(
    error: Error,
    context?: string
  ): StepHandlerResult {
    let message = `Request timeout: ${error.message}`;
    if (context) {
      message = `Request timeout while ${context}: ${error.message}`;
    }

    return this.failure(
      message,
      ErrorType.TIMEOUT,
      true,
      { exception_type: error.constructor.name }
    );
  }

  // =========================================================================
  // Internal Helpers
  // =========================================================================

  private async fetch(url: string, options: RequestInit): Promise<ApiResponse> {
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), this.timeout);

    try {
      const response = await fetch(url, {
        ...options,
        signal: controller.signal,
      });

      // Parse body based on content-type
      const contentType = response.headers.get('content-type') || '';
      let body: any;

      if (contentType.includes('application/json')) {
        try {
          body = await response.json();
        } catch {
          body = await response.text();
        }
      } else {
        body = await response.text();
      }

      return new ApiResponse(response, body);
    } finally {
      clearTimeout(timeoutId);
    }
  }

  private buildUrl(path: string, params?: Record<string, any>): string {
    let url = this.baseUrl + path;

    if (params) {
      const searchParams = new URLSearchParams();
      for (const [key, value] of Object.entries(params)) {
        if (value !== null && value !== undefined) {
          searchParams.append(key, String(value));
        }
      }
      const queryString = searchParams.toString();
      if (queryString) {
        url += (url.includes('?') ? '&' : '?') + queryString;
      }
    }

    return url;
  }

  private mergeHeaders(
    additional?: Record<string, string>,
    isJson: boolean = false
  ): Record<string, string> {
    const headers = { ...this.defaultHeaders };

    if (isJson && !headers['Content-Type']) {
      headers['Content-Type'] = 'application/json';
    }

    if (additional) {
      Object.assign(headers, additional);
    }

    return headers;
  }

  private prepareBody(body: any, asJson: boolean): string | undefined {
    if (!body) {
      return undefined;
    }
    return asJson ? JSON.stringify(body) : String(body);
  }

  private classifyError(response: ApiResponse): string {
    const statusCode = response.statusCode;

    const errorTypeMap: Record<number, string> = {
      400: 'bad_request',
      401: 'unauthorized',
      403: 'forbidden',
      404: 'not_found',
      405: 'method_not_allowed',
      408: 'request_timeout',
      409: 'conflict',
      410: 'gone',
      422: 'unprocessable_entity',
      429: 'rate_limited',
      500: 'internal_server_error',
      502: 'bad_gateway',
      503: 'service_unavailable',
      504: 'gateway_timeout',
    };

    if (statusCode in errorTypeMap) {
      return errorTypeMap[statusCode];
    }

    if (CLIENT_ERROR_CODES.has(statusCode)) {
      return 'client_error';
    }

    if (SERVER_ERROR_CODES.has(statusCode)) {
      return 'server_error';
    }

    return 'http_error';
  }

  private formatErrorMessage(response: ApiResponse): string {
    const statusCode = response.statusCode;

    // Try to extract error message from response body
    if (typeof response.body === 'object' && response.body !== null) {
      const errorKeys = ['error', 'message', 'detail', 'error_message', 'msg'];
      for (const key of errorKeys) {
        if (key in response.body) {
          const errorDetail = response.body[key];
          if (typeof errorDetail === 'string') {
            return `HTTP ${statusCode}: ${errorDetail}`;
          } else if (typeof errorDetail === 'object' && 'message' in errorDetail) {
            return `HTTP ${statusCode}: ${errorDetail.message}`;
          }
        }
      }
    }

    // Generic message based on status code
    const statusMessages: Record<number, string> = {
      400: 'Bad Request',
      401: 'Unauthorized',
      403: 'Forbidden',
      404: 'Not Found',
      405: 'Method Not Allowed',
      408: 'Request Timeout',
      409: 'Conflict',
      410: 'Gone',
      422: 'Unprocessable Entity',
      429: 'Too Many Requests',
      500: 'Internal Server Error',
      502: 'Bad Gateway',
      503: 'Service Unavailable',
      504: 'Gateway Timeout',
    };

    const message = statusMessages[statusCode] || 'HTTP Error';
    return `HTTP ${statusCode}: ${message}`;
  }
}
```

---

### 2. DecisionHandler - Workflow Routing

**File**: `src/handlers/DecisionHandler.ts`

**Purpose**: Base class for handlers that make routing decisions in workflows.

**Python reference**: `workers/python/python/tasker_core/step_handler/decision.py`

```typescript
import { StepHandler } from './StepHandler';
import { StepContext } from '../types/StepContext';
import { StepHandlerResult } from '../types/StepHandlerResult';

/**
 * Type of decision point outcome.
 */
export enum DecisionType {
  CREATE_STEPS = 'create_steps',
  NO_BRANCHES = 'no_branches',
}

/**
 * Outcome from a decision point handler.
 * 
 * Decision handlers return this to indicate which branch(es) of a workflow
 * to execute.
 */
export interface DecisionPointOutcome {
  decisionType: DecisionType;
  nextStepNames: string[];
  dynamicSteps?: Array<Record<string, any>>;
  reason?: string;
  routingContext: Record<string, any>;
}

/**
 * Base class for decision point step handlers.
 * 
 * Decision handlers are used to make routing decisions in workflows.
 * They evaluate conditions and determine which steps should execute next.
 * 
 * @example
 * class CustomerTierRouter extends DecisionHandler {
 *   static handlerName = 'route_by_tier';
 * 
 *   async call(context: StepContext): Promise<StepHandlerResult> {
 *     const tier = context.inputData['customer_tier'];
 *     if (tier === 'enterprise') {
 *       return this.decisionSuccess(
 *         ['enterprise_validation', 'enterprise_processing'],
 *         { tier }
 *       );
 *     } else if (tier === 'premium') {
 *       return this.decisionSuccess(['premium_processing']);
 *     } else {
 *       return this.decisionSuccess(['standard_processing']);
 *     }
 *   }
 * }
 */
export abstract class DecisionHandler extends StepHandler {
  get capabilities(): string[] {
    return ['process', 'decision', 'routing'];
  }

  /**
   * Simplified decision success helper (cross-language standard API).
   * 
   * Use this when routing to one or more steps based on a decision.
   * This is the recommended method for most decision handlers.
   * 
   * @param steps - List of step names to activate
   * @param routingContext - Optional context for routing decisions
   * @param metadata - Optional additional metadata
   * @returns A success StepHandlerResult with the decision outcome
   * 
   * @example Simple routing
   * return this.decisionSuccess(['process_order']);
   * 
   * @example With routing context
   * return this.decisionSuccess(
   *   ['validate_premium', 'process_premium'],
   *   { tier: 'premium' }
   * );
   */
  protected decisionSuccess(
    steps: string[],
    routingContext?: Record<string, any>,
    metadata?: Record<string, any>
  ): StepHandlerResult {
    const outcome: DecisionPointOutcome = {
      decisionType: DecisionType.CREATE_STEPS,
      nextStepNames: steps,
      routingContext: routingContext || {},
    };

    return this.decisionSuccessWithOutcome(outcome, metadata);
  }

  /**
   * Create a success result with a DecisionPointOutcome.
   * 
   * Use this for complex decision outcomes that require dynamic steps
   * or advanced routing. For simple step routing, use `decisionSuccess()`.
   */
  protected decisionSuccessWithOutcome(
    outcome: DecisionPointOutcome,
    metadata?: Record<string, any>
  ): StepHandlerResult {
    // Build decision_point_outcome in format Rust expects
    const decisionPointOutcome: Record<string, any> = {
      type: outcome.decisionType,
      step_names: outcome.nextStepNames,
    };

    const result: Record<string, any> = {
      decision_point_outcome: decisionPointOutcome,
    };

    if (outcome.dynamicSteps) {
      result.dynamic_steps = outcome.dynamicSteps;
    }

    if (outcome.routingContext) {
      result.routing_context = outcome.routingContext;
    }

    const combinedMetadata = metadata || {};
    combinedMetadata['decision_handler'] = this.name;
    combinedMetadata['decision_version'] = this.version;

    return this.success(result, combinedMetadata);
  }

  /**
   * Create a success result for a decision with no branches.
   * 
   * Use this when the decision results in no additional steps being executed.
   * This is still a successful outcome - the decision was made correctly,
   * it just doesn't require any follow-up steps.
   * 
   * @example
   * const outcome: DecisionPointOutcome = {
   *   decisionType: DecisionType.NO_BRANCHES,
   *   nextStepNames: [],
   *   reason: 'No items match processing criteria',
   *   routingContext: {},
   * };
   * return this.decisionNoBranches(outcome);
   */
  protected decisionNoBranches(
    outcome: DecisionPointOutcome,
    metadata?: Record<string, any>
  ): StepHandlerResult {
    const decisionPointOutcome: Record<string, any> = {
      type: outcome.decisionType,
    };

    const result: Record<string, any> = {
      decision_point_outcome: decisionPointOutcome,
      reason: outcome.reason,
    };

    if (outcome.routingContext) {
      result.routing_context = outcome.routingContext;
    }

    const combinedMetadata = metadata || {};
    combinedMetadata['decision_handler'] = this.name;
    combinedMetadata['decision_version'] = this.version;

    return this.success(result, combinedMetadata);
  }

  /**
   * Convenience method to skip all branches.
   * 
   * @param reason - Human-readable reason for skipping branches
   * @param routingContext - Optional context data
   * @param metadata - Optional additional metadata
   * @returns A success StepHandlerResult indicating no branches
   * 
   * @example
   * if (!itemsToProcess.length) {
   *   return this.skipBranches('No items require processing');
   * }
   */
  protected skipBranches(
    reason: string,
    routingContext?: Record<string, any>,
    metadata?: Record<string, any>
  ): StepHandlerResult {
    const outcome: DecisionPointOutcome = {
      decisionType: DecisionType.NO_BRANCHES,
      nextStepNames: [],
      reason,
      routingContext: routingContext || {},
    };

    return this.decisionNoBranches(outcome, metadata);
  }

  /**
   * Create a failure result for a decision that could not be made.
   * 
   * Use this when the handler cannot determine the appropriate routing,
   * typically due to invalid input data or missing required information.
   * 
   * Decision failures are usually NOT retryable.
   * 
   * @example
   * if (!('order_type' in context.inputData)) {
   *   return this.decisionFailure(
   *     'Missing required field: order_type',
   *     'missing_field'
   *   );
   * }
   */
  protected decisionFailure(
    message: string,
    errorType: string = 'decision_error',
    retryable: boolean = false,
    metadata?: Record<string, any>
  ): StepHandlerResult {
    const combinedMetadata = metadata || {};
    combinedMetadata['decision_handler'] = this.name;
    combinedMetadata['decision_version'] = this.version;

    return this.failure(message, errorType, retryable, combinedMetadata);
  }
}
```

---

### 3. Batch Processing Types

**File**: `src/types/BatchTypes.ts`

**Purpose**: Type definitions for batch processing patterns.

**Python reference**: `workers/python/python/tasker_core/types.py` (lines 1230-1450)

```typescript
/**
 * Configuration for cursor-based batch processing.
 * 
 * Defines a range of items to process in a batch worker step.
 */
export interface CursorConfig {
  startCursor: number;
  endCursor: number;
  stepSize: number;
  metadata: Record<string, any>;
}

/**
 * Outcome from a batch analyzer handler.
 * 
 * Batch analyzers return this to define the cursor ranges that will
 * spawn parallel batch worker steps.
 */
export interface BatchAnalyzerOutcome {
  cursorConfigs: CursorConfig[];
  totalItems: number | null;
  batchMetadata: Record<string, any>;
}

/**
 * Context for a batch worker step.
 * 
 * Provides information about the specific batch this worker should process.
 */
export interface BatchWorkerContext {
  batchId: string;
  cursorConfig: CursorConfig;
  batchIndex: number;
  totalBatches: number;
  batchMetadata: Record<string, any>;

  get startCursor(): number;
  get endCursor(): number;
  get stepSize(): number;
}

/**
 * Outcome from a batch worker step.
 * 
 * Batch workers return this to report progress and results.
 */
export interface BatchWorkerOutcome {
  itemsProcessed: number;
  itemsSucceeded: number;
  itemsFailed: number;
  itemsSkipped: number;
  results: Array<Record<string, any>>;
  errors: Array<Record<string, any>>;
  lastCursor: number | null;
  batchMetadata: Record<string, any>;
}
```

---

### 4. Batchable Mixin

**File**: `src/handlers/Batchable.ts`

**Purpose**: Mixin for batch processing capabilities (analyzer and worker patterns).

**Python reference**: `workers/python/python/tasker_core/batch_processing/batchable.py`

```typescript
import { StepContext } from '../types/StepContext';
import { StepHandlerResult } from '../types/StepHandlerResult';
import {
  CursorConfig,
  BatchAnalyzerOutcome,
  BatchWorkerContext,
  BatchWorkerOutcome,
} from '../types/BatchTypes';

/**
 * Mixin interface for batch processing capabilities.
 * 
 * TypeScript implementation using interface + class pattern
 * (since TS doesn't have true mixins like Python/Ruby).
 * 
 * Add batch processing methods to your handler by implementing this interface
 * and extending BatchableMixin.
 */
export interface Batchable {
  createCursorConfig(
    start: number,
    end: number,
    stepSize?: number,
    metadata?: Record<string, any>
  ): CursorConfig;

  createCursorRanges(
    totalItems: number,
    batchSize: number,
    stepSize?: number,
    maxBatches?: number
  ): CursorConfig[];

  createBatchOutcome(
    totalItems: number,
    batchSize: number,
    stepSize?: number,
    batchMetadata?: Record<string, any>
  ): BatchAnalyzerOutcome;

  createWorkerOutcome(
    itemsProcessed: number,
    itemsSucceeded?: number,
    itemsFailed?: number,
    itemsSkipped?: number,
    results?: Array<Record<string, any>>,
    errors?: Array<Record<string, any>>,
    lastCursor?: number,
    batchMetadata?: Record<string, any>
  ): BatchWorkerOutcome;

  getBatchContext(context: StepContext): BatchWorkerContext | null;

  batchAnalyzerSuccess(
    outcome: BatchAnalyzerOutcome,
    metadata?: Record<string, any>
  ): StepHandlerResult;

  batchWorkerSuccess(
    outcome: BatchWorkerOutcome,
    metadata?: Record<string, any>
  ): StepHandlerResult;
}

/**
 * Implementation of Batchable methods.
 * 
 * Use this class to add batch processing capabilities to your handlers:
 * 
 * @example Analyzer
 * class ProductAnalyzer extends StepHandler implements Batchable {
 *   // Add mixin methods
 *   createCursorConfig = BatchableMixin.prototype.createCursorConfig;
 *   createBatchOutcome = BatchableMixin.prototype.createBatchOutcome;
 *   batchAnalyzerSuccess = BatchableMixin.prototype.batchAnalyzerSuccess;
 *   // ... other batchable methods
 * 
 *   async call(context: StepContext): Promise<StepHandlerResult> {
 *     const total = context.inputData['product_count'];
 *     const outcome = this.createBatchOutcome(total, 100);
 *     return this.batchAnalyzerSuccess(outcome);
 *   }
 * }
 * 
 * @example Worker
 * class ProductWorker extends StepHandler implements Batchable {
 *   getBatchContext = BatchableMixin.prototype.getBatchContext;
 *   createWorkerOutcome = BatchableMixin.prototype.createWorkerOutcome;
 *   batchWorkerSuccess = BatchableMixin.prototype.batchWorkerSuccess;
 * 
 *   async call(context: StepContext): Promise<StepHandlerResult> {
 *     const batchCtx = this.getBatchContext(context);
 *     if (!batchCtx) {
 *       return this.failure('No batch context found');
 *     }
 * 
 *     const results = [];
 *     for (let i = batchCtx.startCursor; i < batchCtx.endCursor; i++) {
 *       results.push(await this.processItem(i));
 *     }
 * 
 *     const outcome = this.createWorkerOutcome(
 *       results.length,
 *       results.length
 *     );
 *     return this.batchWorkerSuccess(outcome);
 *   }
 * }
 */
export class BatchableMixin implements Batchable {
  // Type assertion for accessing StepHandler methods
  private get handler(): any {
    return this;
  }

  createCursorConfig(
    start: number,
    end: number,
    stepSize: number = 1,
    metadata?: Record<string, any>
  ): CursorConfig {
    return {
      startCursor: start,
      endCursor: end,
      stepSize,
      metadata: metadata || {},
    };
  }

  createCursorRanges(
    totalItems: number,
    batchSize: number,
    stepSize: number = 1,
    maxBatches?: number
  ): CursorConfig[] {
    if (totalItems === 0) {
      return [];
    }

    // Adjust batch size if max_batches would create more batches
    if (maxBatches && maxBatches > 0) {
      const calculatedBatches = Math.ceil(totalItems / batchSize);
      if (calculatedBatches > maxBatches) {
        batchSize = Math.ceil(totalItems / maxBatches);
      }
    }

    const configs: CursorConfig[] = [];
    let start = 0;

    while (start < totalItems) {
      const end = Math.min(start + batchSize, totalItems);
      configs.push({
        startCursor: start,
        endCursor: end,
        stepSize,
        metadata: {},
      });
      start = end;
    }

    return configs;
  }

  createBatchOutcome(
    totalItems: number,
    batchSize: number,
    stepSize: number = 1,
    batchMetadata?: Record<string, any>
  ): BatchAnalyzerOutcome {
    const cursorConfigs = this.createCursorRanges(totalItems, batchSize, stepSize);

    return {
      cursorConfigs,
      totalItems,
      batchMetadata: batchMetadata || {},
    };
  }

  createWorkerOutcome(
    itemsProcessed: number,
    itemsSucceeded: number = 0,
    itemsFailed: number = 0,
    itemsSkipped: number = 0,
    results?: Array<Record<string, any>>,
    errors?: Array<Record<string, any>>,
    lastCursor?: number,
    batchMetadata?: Record<string, any>
  ): BatchWorkerOutcome {
    return {
      itemsProcessed,
      itemsSucceeded: itemsSucceeded || itemsProcessed,
      itemsFailed,
      itemsSkipped,
      results: results || [],
      errors: errors || [],
      lastCursor: lastCursor ?? null,
      batchMetadata: batchMetadata || {},
    };
  }

  getBatchContext(context: StepContext): BatchWorkerContext | null {
    // Look for batch context in step_config or input_data
    let batchData = context.stepConfig['batch_context'];
    if (!batchData) {
      batchData = context.inputData['batch_context'];
    }

    if (!batchData) {
      return null;
    }

    // Extract cursor config
    const cursorData = batchData.cursor_config || {};
    const cursorConfig: CursorConfig = {
      startCursor: cursorData.start_cursor || 0,
      endCursor: cursorData.end_cursor || 0,
      stepSize: cursorData.step_size || 1,
      metadata: cursorData.metadata || {},
    };

    return {
      batchId: batchData.batch_id || '',
      cursorConfig,
      batchIndex: batchData.batch_index || 0,
      totalBatches: batchData.total_batches || 1,
      batchMetadata: batchData.batch_metadata || {},
      get startCursor() {
        return this.cursorConfig.startCursor;
      },
      get endCursor() {
        return this.cursorConfig.endCursor;
      },
      get stepSize() {
        return this.cursorConfig.stepSize;
      },
    };
  }

  batchAnalyzerSuccess(
    outcome: BatchAnalyzerOutcome,
    metadata?: Record<string, any>
  ): StepHandlerResult {
    const result: Record<string, any> = {
      batch_analyzer_outcome: {
        cursor_configs: outcome.cursorConfigs.map(c => ({
          start_cursor: c.startCursor,
          end_cursor: c.endCursor,
          step_size: c.stepSize,
          metadata: c.metadata,
        })),
        total_items: outcome.totalItems,
        batch_metadata: outcome.batchMetadata,
      },
    };

    return this.handler.success(result, metadata);
  }

  batchWorkerSuccess(
    outcome: BatchWorkerOutcome,
    metadata?: Record<string, any>
  ): StepHandlerResult {
    const result: Record<string, any> = {
      batch_worker_outcome: {
        items_processed: outcome.itemsProcessed,
        items_succeeded: outcome.itemsSucceeded,
        items_failed: outcome.itemsFailed,
        items_skipped: outcome.itemsSkipped,
        results: outcome.results,
        errors: outcome.errors,
        last_cursor: outcome.lastCursor,
        batch_metadata: outcome.batchMetadata,
      },
    };

    return this.handler.success(result, metadata);
  }
}
```

---

## File Structure

```
workers/typescript/
├── src/
│   ├── handlers/
│   │   ├── StepHandler.ts        # From TAS-102
│   │   ├── ApiHandler.ts          # HTTP API integration
│   │   ├── DecisionHandler.ts     # Workflow routing
│   │   ├── Batchable.ts           # Batch processing mixin
│   │   └── index.ts               # Export all handlers
│   ├── types/
│   │   ├── BatchTypes.ts          # Batch processing types
│   │   └── index.ts               # Export all types
│   └── index.ts                   # Main package exports
└── tests/
    ├── handlers/
    │   ├── ApiHandler.test.ts
    │   ├── DecisionHandler.test.ts
    │   └── Batchable.test.ts
    └── integration/
        └── specialized-handlers.test.ts
```

---

## Example Handler Implementations

### Example: API Handler

```typescript
class GitHubApiHandler extends ApiHandler {
  static handlerName = 'fetch_github_user';
  static baseUrl = 'https://api.github.com';
  static defaultHeaders = {
    'Accept': 'application/vnd.github.v3+json',
  };

  async call(context: StepContext): Promise<StepHandlerResult> {
    const username = context.inputData['username'];
    if (!username) {
      return this.failure(
        'Missing required field: username',
        ErrorType.VALIDATION_ERROR,
        false
      );
    }

    try {
      const response = await this.get(`/users/${username}`);
      if (response.ok) {
        return this.apiSuccess(response);
      }
      return this.apiFailure(response);
    } catch (error) {
      if (error instanceof Error) {
        return this.connectionError(error, 'fetching GitHub user');
      }
      throw error;
    }
  }
}
```

### Example: Decision Handler

```typescript
class OrderRoutingHandler extends DecisionHandler {
  static handlerName = 'route_order';

  async call(context: StepContext): Promise<StepHandlerResult> {
    const orderType = context.inputData['order_type'];
    const orderValue = context.inputData['order_value'] || 0;

    if (!orderType) {
      return this.decisionFailure(
        'Missing required field: order_type',
        'missing_field'
      );
    }

    // Route based on order type and value
    if (orderType === 'enterprise' && orderValue > 10000) {
      return this.decisionSuccess(
        ['validate_enterprise', 'process_enterprise', 'notify_sales'],
        { order_type: orderType, tier: 'high_value' }
      );
    } else if (orderType === 'premium') {
      return this.decisionSuccess(
        ['validate_premium', 'process_premium']
      );
    } else if (orderValue === 0) {
      return this.skipBranches('Order value is zero, no processing needed');
    } else {
      return this.decisionSuccess(['process_standard']);
    }
  }
}
```

### Example: Batch Analyzer

```typescript
class ProductBatchAnalyzer extends StepHandler implements Batchable {
  static handlerName = 'analyze_products';

  // Add Batchable methods
  createBatchOutcome = BatchableMixin.prototype.createBatchOutcome;
  batchAnalyzerSuccess = BatchableMixin.prototype.batchAnalyzerSuccess;

  async call(context: StepContext): Promise<StepHandlerResult> {
    const productIds = context.inputData['product_ids'] || [];
    const totalProducts = productIds.length;

    if (totalProducts === 0) {
      return this.failure(
        'No products to process',
        ErrorType.VALIDATION_ERROR,
        false
      );
    }

    // Create batch outcome with 100 products per batch
    const outcome = this.createBatchOutcome(
      totalProducts,
      100,
      1,
      { source: 'database', category: context.inputData['category'] }
    );

    return this.batchAnalyzerSuccess(outcome);
  }
}
```

### Example: Batch Worker

```typescript
class ProductBatchWorker extends StepHandler implements Batchable {
  static handlerName = 'process_product_batch';

  // Add Batchable methods
  getBatchContext = BatchableMixin.prototype.getBatchContext;
  createWorkerOutcome = BatchableMixin.prototype.createWorkerOutcome;
  batchWorkerSuccess = BatchableMixin.prototype.batchWorkerSuccess;

  async call(context: StepContext): Promise<StepHandlerResult> {
    const batchCtx = this.getBatchContext(context);
    if (!batchCtx) {
      return this.failure(
        'No batch context found - is this a worker step?',
        ErrorType.HANDLER_ERROR,
        false
      );
    }

    const productIds = context.inputData['product_ids'] || [];
    const results: Array<Record<string, any>> = [];
    const errors: Array<Record<string, any>> = [];

    // Process items in cursor range
    for (let i = batchCtx.startCursor; i < batchCtx.endCursor; i++) {
      try {
        const productId = productIds[i];
        const result = await this.processProduct(productId);
        results.push({ product_id: productId, ...result });
      } catch (error) {
        errors.push({
          index: i,
          product_id: productIds[i],
          error: error instanceof Error ? error.message : String(error),
        });
      }
    }

    const outcome = this.createWorkerOutcome(
      batchCtx.endCursor - batchCtx.startCursor,
      results.length,
      errors.length,
      0,
      results,
      errors
    );

    return this.batchWorkerSuccess(outcome);
  }

  private async processProduct(productId: string): Promise<Record<string, any>> {
    // Simulate product processing
    return { status: 'processed', processed_at: new Date().toISOString() };
  }
}
```

---

## Success Criteria

- [ ] ApiHandler with HTTP methods (get, post, put, patch, delete)
- [ ] Automatic error classification by HTTP status code
- [ ] ApiHandler result helpers (apiSuccess, apiFailure)
- [ ] DecisionHandler with simplified routing helper (decisionSuccess)
- [ ] DecisionHandler skip helper (skipBranches)
- [ ] Batch processing types (CursorConfig, BatchAnalyzerOutcome, etc.)
- [ ] Batchable mixin with analyzer and worker patterns
- [ ] Example implementations for each specialized handler
- [ ] Unit tests for all specialized handlers (>80% coverage)
- [ ] Integration tests showing end-to-end workflows
- [ ] JSDoc documentation on all public APIs

---

## Dependencies

**Requires**:
- TAS-102: Handler API and Registry (StepHandler base class)

**Blocks**:
- TAS-104: Server and Bootstrap
- TAS-105: Testing and Examples

---

## Estimated Breakdown

| Task | Estimated Time |
|------|----------------|
| ApiHandler implementation | 2 hours |
| DecisionHandler implementation | 1 hour |
| Batch processing types | 1 hour |
| Batchable mixin implementation | 2 hours |
| Example handlers | 1 hour |
| Unit tests | 3 hours |
| Integration tests | 1 hour |
| Documentation | 1 hour |
| **Total** | **12 hours (~1.5 days)** |

---

## Implementation Notes

### TypeScript Mixin Pattern

TypeScript doesn't have native mixin support like Python/Ruby. We use the interface + manual method copying pattern:

```typescript
class MyHandler extends StepHandler implements Batchable {
  // Copy methods from BatchableMixin
  createBatchOutcome = BatchableMixin.prototype.createBatchOutcome;
  batchAnalyzerSuccess = BatchableMixin.prototype.batchAnalyzerSuccess;
  
  async call(context: StepContext): Promise<StepHandlerResult> {
    // Use mixin methods
    const outcome = this.createBatchOutcome(100, 10);
    return this.batchAnalyzerSuccess(outcome);
  }
}
```

### Fetch API Availability

- **Bun**: Native fetch API available
- **Node.js 18+**: Native fetch API available
- **Node.js <18**: Would need node-fetch polyfill (defer to TAS-105)

### Error Classification

HTTP status code classification follows industry standards:
- 4xx (except 408, 429) → Permanent errors, not retryable
- 408, 429 → Temporary errors, retryable
- 5xx → Server errors, retryable

---

## Next Steps

After TAS-103 completion:

1. **TAS-104**: Implement server bootstrap and lifecycle management
2. **TAS-105**: Add comprehensive tests and example handlers
3. **TAS-107**: Complete documentation with cross-language comparisons
