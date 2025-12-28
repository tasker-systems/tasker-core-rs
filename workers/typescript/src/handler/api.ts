import { ErrorType } from '../types/error-type';
import type { StepHandlerResult } from '../types/step-handler-result';
import { StepHandler } from './base';

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
 * Base class for HTTP API step handlers.
 *
 * Provides HTTP client functionality with automatic error classification,
 * retry handling, and convenient methods for common HTTP operations.
 *
 * Uses native fetch API (available in Bun and Node.js 18+).
 *
 * Matches Python's ApiHandler and Ruby's ApiHandler (TAS-92 aligned).
 *
 * @example
 * ```typescript
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
 * ```
 */
export abstract class ApiHandler extends StepHandler {
  /** Base URL for API calls. Override in subclasses. */
  static baseUrl = '';

  /** Default request timeout in milliseconds. */
  static defaultTimeout = 30000;

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
   * ```typescript
   * const response = await this.get('/users', { page: 1 });
   * if (response.ok) {
   *   const users = response.body.data;
   * }
   * ```
   */
  protected async get(
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
   *
   * @param path - URL path (appended to baseUrl)
   * @param options - Request options
   * @returns ApiResponse wrapping the response
   *
   * @example
   * ```typescript
   * const response = await this.post('/users', {
   *   body: { name: 'Alice' }
   * });
   * ```
   */
  protected async post(
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
  protected async put(
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
  protected async patch(
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
  protected async delete(path: string, headers?: Record<string, string>): Promise<ApiResponse> {
    const url = this.buildUrl(path);
    return this.fetch(url, {
      method: 'DELETE',
      headers: this.mergeHeaders(headers),
    });
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
   *
   * @param response - The API response
   * @param result - Optional custom result data
   * @param includeResponse - Whether to include response metadata
   * @returns A success StepHandlerResult
   *
   * @example
   * ```typescript
   * const response = await this.get('/data');
   * if (response.ok) {
   *   return this.apiSuccess(response);
   * }
   * ```
   */
  protected apiSuccess(
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
   * ```typescript
   * const response = await this.post('/payments', { body: data });
   * if (!response.ok) {
   *   return this.apiFailure(response);
   * }
   * ```
   */
  protected apiFailure(response: ApiResponse, message?: string): StepHandlerResult {
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

    return this.failure(errorMessage, errorType, retryable, metadata);
  }

  /**
   * Create a failure result from a connection error.
   *
   * Connection errors are typically retryable.
   */
  protected connectionError(error: Error, context?: string): StepHandlerResult {
    let message = `Connection error: ${error.message}`;
    if (context) {
      message = `Connection error while ${context}: ${error.message}`;
    }

    return this.failure(message, 'connection_error', true, {
      exception_type: error.constructor.name,
    });
  }

  /**
   * Create a failure result from a timeout error.
   *
   * Timeout errors are typically retryable.
   */
  protected timeoutError(error: Error, context?: string): StepHandlerResult {
    let message = `Request timeout: ${error.message}`;
    if (context) {
      message = `Request timeout while ${context}: ${error.message}`;
    }

    return this.failure(message, ErrorType.TIMEOUT, true, {
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
