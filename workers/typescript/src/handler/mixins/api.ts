/**
 * API mixin for HTTP functionality.
 *
 * TAS-112: Composition Pattern - API Mixin
 *
 * This module provides the APIMixin class for step handlers that need HTTP
 * functionality. Use via interface implementation with method binding.
 *
 * @example
 * ```typescript
 * class FetchUserHandler extends StepHandler implements APICapable {
 *   static handlerName = 'fetch_user';
 *   static baseUrl = 'https://api.example.com';
 *
 *   // Bind APIMixin methods
 *   get = APIMixin.prototype.get.bind(this);
 *   apiSuccess = APIMixin.prototype.apiSuccess.bind(this);
 *   apiFailure = APIMixin.prototype.apiFailure.bind(this);
 *   // ... other required methods
 *
 *   async call(context: StepContext): Promise<StepHandlerResult> {
 *     const response = await this.get('/users');
 *     if (response.ok) {
 *       return this.apiSuccess(response);
 *     }
 *     return this.apiFailure(response);
 *   }
 * }
 * ```
 *
 * @module handler/mixins/api
 */

import { ErrorType } from '../../types/error-type.js';
import { StepHandlerResult } from '../../types/step-handler-result.js';

/**
 * HTTP status codes that indicate client errors (4xx).
 */
const CLIENT_ERROR_MIN = 400;
const CLIENT_ERROR_MAX = 499;

/**
 * HTTP status codes that indicate server errors (5xx).
 */
const SERVER_ERROR_MIN = 500;
const SERVER_ERROR_MAX = 599;

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
 * Standard HTTP status code messages.
 */
const STATUS_MESSAGES: Record<number, string> = {
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

/**
 * Response wrapper for API calls.
 *
 * Provides convenient access to response data and error classification.
 */
export class ApiResponse {
  public readonly statusCode: number;
  public readonly headers: Record<string, string>;
  public readonly body: unknown;
  public readonly rawResponse: Response;

  constructor(response: Response, body?: unknown) {
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
    return this.statusCode >= CLIENT_ERROR_MIN && this.statusCode <= CLIENT_ERROR_MAX;
  }

  /**
   * Check if the response indicates a server error (5xx status).
   */
  get isServerError(): boolean {
    return this.statusCode >= SERVER_ERROR_MIN && this.statusCode <= SERVER_ERROR_MAX;
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
    const parsed = Number.parseInt(retryAfter, 10);
    return Number.isNaN(parsed) ? 60 : parsed;
  }

  /**
   * Convert the response to a dictionary for result output.
   */
  toDict(): Record<string, unknown> {
    return {
      status_code: this.statusCode,
      headers: this.headers,
      body: this.body,
    };
  }
}

/**
 * Interface for API-capable handlers.
 *
 * Implement this interface and bind APIMixin methods to get HTTP functionality.
 */
export interface APICapable {
  /** Base URL for API calls */
  baseUrl: string;
  /** Default request timeout in milliseconds */
  timeout: number;
  /** Default headers to include in all requests */
  defaultHeaders: Record<string, string>;

  // HTTP methods
  get(
    path: string,
    params?: Record<string, unknown>,
    headers?: Record<string, string>
  ): Promise<ApiResponse>;

  post(
    path: string,
    options?: {
      body?: unknown;
      json?: boolean;
      headers?: Record<string, string>;
    }
  ): Promise<ApiResponse>;

  put(
    path: string,
    options?: {
      body?: unknown;
      json?: boolean;
      headers?: Record<string, string>;
    }
  ): Promise<ApiResponse>;

  patch(
    path: string,
    options?: {
      body?: unknown;
      json?: boolean;
      headers?: Record<string, string>;
    }
  ): Promise<ApiResponse>;

  delete(path: string, headers?: Record<string, string>): Promise<ApiResponse>;

  request(method: string, path: string, options?: RequestInit): Promise<ApiResponse>;

  // Result helpers
  apiSuccess(
    response: ApiResponse,
    result?: Record<string, unknown>,
    includeResponse?: boolean
  ): StepHandlerResult;

  apiFailure(response: ApiResponse, message?: string): StepHandlerResult;

  connectionError(error: Error, context?: string): StepHandlerResult;

  timeoutError(error: Error, context?: string): StepHandlerResult;
}

/**
 * Implementation of API methods.
 *
 * TAS-112: Use via interface implementation with method binding.
 *
 * Provides HTTP client functionality with automatic error classification,
 * retry handling, and convenient methods for common HTTP operations.
 */
export class APIMixin implements APICapable {
  // These should be defined on the handler class as static properties
  static baseUrl = '';
  static defaultTimeout = 30000;
  static defaultHeaders: Record<string, string> = {};

  get baseUrl(): string {
    const ctor = this.constructor as typeof APIMixin;
    return ctor.baseUrl;
  }

  get timeout(): number {
    const ctor = this.constructor as typeof APIMixin;
    return ctor.defaultTimeout;
  }

  get defaultHeaders(): Record<string, string> {
    const ctor = this.constructor as typeof APIMixin;
    return ctor.defaultHeaders;
  }

  // =========================================================================
  // HTTP Methods
  // =========================================================================

  /**
   * Make a GET request.
   */
  async get(
    path: string,
    params?: Record<string, unknown>,
    headers?: Record<string, string>
  ): Promise<ApiResponse> {
    const url = this.buildUrl(path, params);
    return this.fetch(url, {
      method: 'GET',
      headers: this.mergeHeaders(headers),
    });
  }

  /**
   * Make a POST request.
   */
  async post(
    path: string,
    options?: {
      body?: unknown;
      json?: boolean;
      headers?: Record<string, string>;
    }
  ): Promise<ApiResponse> {
    const url = this.buildUrl(path);
    const body = this.prepareBody(options?.body, options?.json !== false);
    return this.fetch(url, {
      method: 'POST',
      headers: this.mergeHeaders(options?.headers, options?.json !== false),
      body,
    });
  }

  /**
   * Make a PUT request.
   */
  async put(
    path: string,
    options?: {
      body?: unknown;
      json?: boolean;
      headers?: Record<string, string>;
    }
  ): Promise<ApiResponse> {
    const url = this.buildUrl(path);
    const body = this.prepareBody(options?.body, options?.json !== false);
    return this.fetch(url, {
      method: 'PUT',
      headers: this.mergeHeaders(options?.headers, options?.json !== false),
      body,
    });
  }

  /**
   * Make a PATCH request.
   */
  async patch(
    path: string,
    options?: {
      body?: unknown;
      json?: boolean;
      headers?: Record<string, string>;
    }
  ): Promise<ApiResponse> {
    const url = this.buildUrl(path);
    const body = this.prepareBody(options?.body, options?.json !== false);
    return this.fetch(url, {
      method: 'PATCH',
      headers: this.mergeHeaders(options?.headers, options?.json !== false),
      body,
    });
  }

  /**
   * Make a DELETE request.
   */
  async delete(path: string, headers?: Record<string, string>): Promise<ApiResponse> {
    const url = this.buildUrl(path);
    return this.fetch(url, {
      method: 'DELETE',
      headers: this.mergeHeaders(headers),
    });
  }

  /**
   * Make an arbitrary HTTP request.
   */
  async request(method: string, path: string, options?: RequestInit): Promise<ApiResponse> {
    const url = this.buildUrl(path);
    return this.fetch(url, {
      ...options,
      method,
      headers: this.mergeHeaders(options?.headers as Record<string, string>),
    });
  }

  // =========================================================================
  // Result Helpers
  // =========================================================================

  /**
   * Create a success result from an API response.
   */
  apiSuccess(
    response: ApiResponse,
    result?: Record<string, unknown>,
    includeResponse = true
  ): StepHandlerResult {
    const resultData =
      result ||
      (typeof response.body === 'object' && response.body !== null
        ? (response.body as Record<string, unknown>)
        : { data: response.body });

    const metadata: Record<string, unknown> = {};
    if (includeResponse) {
      metadata.status_code = response.statusCode;
      metadata.headers = response.headers;
    }

    return StepHandlerResult.success(resultData, metadata);
  }

  /**
   * Create a failure result from an API response.
   */
  apiFailure(response: ApiResponse, message?: string): StepHandlerResult {
    const errorType = this.classifyError(response);
    const errorMessage = message || this.formatErrorMessage(response);
    const retryable = response.isRetryable;

    const metadata: Record<string, unknown> = {
      status_code: response.statusCode,
      headers: response.headers,
    };

    if (response.retryAfter !== null) {
      metadata.retry_after_seconds = response.retryAfter;
    }

    if (response.body) {
      metadata.response_body = response.body;
    }

    return StepHandlerResult.failure(errorMessage, errorType, retryable, metadata);
  }

  /**
   * Create a failure result from a connection error.
   */
  connectionError(error: Error, context?: string): StepHandlerResult {
    let message = `Connection error: ${error.message}`;
    if (context) {
      message = `Connection error while ${context}: ${error.message}`;
    }

    return StepHandlerResult.failure(message, 'connection_error', true, {
      exception_type: error.constructor.name,
    });
  }

  /**
   * Create a failure result from a timeout error.
   */
  timeoutError(error: Error, context?: string): StepHandlerResult {
    let message = `Request timeout: ${error.message}`;
    if (context) {
      message = `Request timeout while ${context}: ${error.message}`;
    }

    return StepHandlerResult.failure(message, ErrorType.TIMEOUT, true, {
      exception_type: error.constructor.name,
    });
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
      let body: unknown;

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

  private buildUrl(path: string, params?: Record<string, unknown>): string {
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
    isJson = false
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

  private prepareBody(body: unknown, asJson: boolean): string | undefined {
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

    const mapped = errorTypeMap[statusCode];
    if (mapped !== undefined) {
      return mapped;
    }

    if (response.isClientError) {
      return 'client_error';
    }

    if (response.isServerError) {
      return 'server_error';
    }

    return 'http_error';
  }

  private formatErrorMessage(response: ApiResponse): string {
    const statusCode = response.statusCode;

    // Try to extract error message from response body
    const bodyMessage = this.extractBodyErrorMessage(response.body);
    if (bodyMessage) {
      return `HTTP ${statusCode}: ${bodyMessage}`;
    }

    // Generic message based on status code
    return `HTTP ${statusCode}: ${STATUS_MESSAGES[statusCode] || 'HTTP Error'}`;
  }

  private extractBodyErrorMessage(body: unknown): string | null {
    if (typeof body !== 'object' || body === null) {
      return null;
    }

    const bodyObj = body as Record<string, unknown>;
    const errorKeys = ['error', 'message', 'detail', 'error_message', 'msg'];

    for (const key of errorKeys) {
      if (!(key in bodyObj)) continue;

      const errorDetail = bodyObj[key];
      if (typeof errorDetail === 'string') {
        return errorDetail;
      }
      if (typeof errorDetail === 'object' && errorDetail !== null && 'message' in errorDetail) {
        return String((errorDetail as Record<string, unknown>).message);
      }
    }

    return null;
  }
}

/**
 * Helper function to apply API methods to a handler instance.
 *
 * @example
 * ```typescript
 * class MyApiHandler extends StepHandler {
 *   constructor() {
 *     super();
 *     applyAPI(this);
 *   }
 * }
 * ```
 */
export function applyAPI<T extends object>(target: T): T & APICapable {
  const mixin = new APIMixin();

  // Bind all API methods
  Object.defineProperty(target, 'baseUrl', {
    get: () => {
      const ctor = target.constructor as typeof APIMixin;
      return ctor.baseUrl || '';
    },
  });
  Object.defineProperty(target, 'timeout', {
    get: () => {
      const ctor = target.constructor as typeof APIMixin;
      return ctor.defaultTimeout || 30000;
    },
  });
  Object.defineProperty(target, 'defaultHeaders', {
    get: () => {
      const ctor = target.constructor as typeof APIMixin;
      return ctor.defaultHeaders || {};
    },
  });

  (target as T & APICapable).get = mixin.get.bind(target);
  (target as T & APICapable).post = mixin.post.bind(target);
  (target as T & APICapable).put = mixin.put.bind(target);
  (target as T & APICapable).patch = mixin.patch.bind(target);
  (target as T & APICapable).delete = mixin.delete.bind(target);
  (target as T & APICapable).request = mixin.request.bind(target);
  (target as T & APICapable).apiSuccess = mixin.apiSuccess.bind(target);
  (target as T & APICapable).apiFailure = mixin.apiFailure.bind(target);
  (target as T & APICapable).connectionError = mixin.connectionError.bind(target);
  (target as T & APICapable).timeoutError = mixin.timeoutError.bind(target);

  return target as T & APICapable;
}
