/**
 * API handler for HTTP interactions.
 *
 * TAS-112: Composition Pattern (DEPRECATED CLASS)
 *
 * This module provides the ApiHandler class for backward compatibility.
 * For new code, use the mixin pattern:
 *
 * @example Using APIMixin
 * ```typescript
 * import { StepHandler } from './base';
 * import { APIMixin, APICapable, applyAPI } from './mixins/api';
 *
 * class FetchUserHandler extends StepHandler implements APICapable {
 *   static handlerName = 'fetch_user';
 *   static baseUrl = 'https://api.example.com';
 *
 *   constructor() {
 *     super();
 *     applyAPI(this);
 *   }
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
 * @module handler/api
 */

import type { StepHandlerResult } from '../types/step-handler-result.js';
import { StepHandler } from './base.js';
import { APIMixin, ApiResponse } from './mixins/api.js';

// Re-export ApiResponse for convenience
export { ApiResponse } from './mixins/api.js';

/**
 * Base class for HTTP API step handlers.
 *
 * TAS-112: This class is provided for backward compatibility.
 * For new code, prefer using APIMixin directly with applyAPI().
 *
 * Provides HTTP client functionality with automatic error classification,
 * retry handling, and convenient methods for common HTTP operations.
 *
 * Uses native fetch API (available in Bun and Node.js 18+).
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

  private readonly _apiMixin = new APIMixin();

  constructor() {
    super();
    // We need to bind the mixin methods to this instance
    // so they can access the correct baseUrl, timeout, and defaultHeaders
  }

  get capabilities(): string[] {
    return ['process', 'http', 'api'];
  }

  /**
   * Get the base URL for this handler.
   */
  get baseUrl(): string {
    const ctor = this.constructor as typeof ApiHandler;
    return ctor.baseUrl;
  }

  /**
   * Get the default timeout for this handler.
   */
  get timeout(): number {
    const ctor = this.constructor as typeof ApiHandler;
    return ctor.defaultTimeout;
  }

  /**
   * Get the default headers for this handler.
   */
  get defaultHeaders(): Record<string, string> {
    const ctor = this.constructor as typeof ApiHandler;
    return ctor.defaultHeaders;
  }

  // =========================================================================
  // HTTP Methods - Delegate to mixin
  // =========================================================================

  protected get(
    path: string,
    params?: Record<string, unknown>,
    headers?: Record<string, string>
  ): Promise<ApiResponse> {
    return this._apiMixin.get.call(this, path, params, headers);
  }

  protected post(
    path: string,
    options?: {
      body?: unknown;
      json?: boolean;
      headers?: Record<string, string>;
    }
  ): Promise<ApiResponse> {
    return this._apiMixin.post.call(this, path, options);
  }

  protected put(
    path: string,
    options?: {
      body?: unknown;
      json?: boolean;
      headers?: Record<string, string>;
    }
  ): Promise<ApiResponse> {
    return this._apiMixin.put.call(this, path, options);
  }

  protected patch(
    path: string,
    options?: {
      body?: unknown;
      json?: boolean;
      headers?: Record<string, string>;
    }
  ): Promise<ApiResponse> {
    return this._apiMixin.patch.call(this, path, options);
  }

  protected delete(path: string, headers?: Record<string, string>): Promise<ApiResponse> {
    return this._apiMixin.delete.call(this, path, headers);
  }

  protected request(method: string, path: string, options?: RequestInit): Promise<ApiResponse> {
    return this._apiMixin.request.call(this, method, path, options);
  }

  // =========================================================================
  // Result Helpers - Delegate to mixin
  // =========================================================================

  protected apiSuccess(
    response: ApiResponse,
    result?: Record<string, unknown>,
    includeResponse = true
  ): StepHandlerResult {
    return this._apiMixin.apiSuccess.call(this, response, result, includeResponse);
  }

  protected apiFailure(response: ApiResponse, message?: string): StepHandlerResult {
    return this._apiMixin.apiFailure.call(this, response, message);
  }

  protected connectionError(error: Error, context?: string): StepHandlerResult {
    return this._apiMixin.connectionError.call(this, error, context);
  }

  protected timeoutError(error: Error, context?: string): StepHandlerResult {
    return this._apiMixin.timeoutError.call(this, error, context);
  }
}
