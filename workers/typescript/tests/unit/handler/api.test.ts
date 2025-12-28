import { beforeEach, describe, expect, mock, test } from 'bun:test';
import { ApiHandler, ApiResponse } from '../../../src/handler/api';
import { StepContext } from '../../../src/types/step-context';
import type { StepHandlerResult } from '../../../src/types/step-handler-result';

// =============================================================================
// Test Fixtures
// =============================================================================

/**
 * Concrete implementation of ApiHandler for testing.
 */
class TestApiHandler extends ApiHandler {
  static handlerName = 'test_api_handler';
  static baseUrl = 'https://api.example.com';
  static defaultHeaders = { 'X-Api-Key': 'test-key' };
  static defaultTimeout = 5000;

  // Expose protected methods for testing
  async testGet(path: string, params?: Record<string, unknown>, headers?: Record<string, string>) {
    return this.get(path, params, headers);
  }

  async testPost(
    path: string,
    options?: { body?: unknown; json?: boolean; headers?: Record<string, string> }
  ) {
    return this.post(path, options);
  }

  async testPut(
    path: string,
    options?: { body?: unknown; json?: boolean; headers?: Record<string, string> }
  ) {
    return this.put(path, options);
  }

  async testPatch(
    path: string,
    options?: { body?: unknown; json?: boolean; headers?: Record<string, string> }
  ) {
    return this.patch(path, options);
  }

  async testDelete(path: string, headers?: Record<string, string>) {
    return this.delete(path, headers);
  }

  testApiSuccess(response: ApiResponse, result?: Record<string, unknown>, includeResponse = true) {
    return this.apiSuccess(response, result, includeResponse);
  }

  testApiFailure(response: ApiResponse, message?: string) {
    return this.apiFailure(response, message);
  }

  testConnectionError(error: Error, context?: string) {
    return this.connectionError(error, context);
  }

  testTimeoutError(error: Error, context?: string) {
    return this.timeoutError(error, context);
  }

  async call(_context: StepContext): Promise<StepHandlerResult> {
    // Simple implementation for testing
    const response = await this.get('/test');
    if (response.ok) {
      return this.apiSuccess(response);
    }
    return this.apiFailure(response);
  }
}

/**
 * Create a mock Response object.
 */
function createMockResponse(options: {
  status: number;
  body?: unknown;
  headers?: Record<string, string>;
}): Response {
  const { status, body, headers = {} } = options;

  const headersObj = new Headers(headers);
  if (body && typeof body === 'object' && !headersObj.has('content-type')) {
    headersObj.set('content-type', 'application/json');
  }

  return {
    status,
    ok: status >= 200 && status < 300,
    headers: headersObj,
    json: async () => body,
    text: async () => (typeof body === 'string' ? body : JSON.stringify(body)),
  } as Response;
}

// =============================================================================
// ApiResponse Tests
// =============================================================================

describe('ApiResponse', () => {
  describe('constructor', () => {
    test('should create from Response with body', () => {
      const response = createMockResponse({ status: 200, body: { data: 'test' } });
      const apiResponse = new ApiResponse(response, { data: 'test' });

      expect(apiResponse.statusCode).toBe(200);
      expect(apiResponse.body).toEqual({ data: 'test' });
      expect(apiResponse.rawResponse).toBe(response);
    });

    test('should extract headers as plain object', () => {
      const response = createMockResponse({
        status: 200,
        headers: { 'Content-Type': 'application/json', 'X-Request-Id': 'abc123' },
      });
      const apiResponse = new ApiResponse(response);

      expect(apiResponse.headers['content-type']).toBe('application/json');
      expect(apiResponse.headers['x-request-id']).toBe('abc123');
    });
  });

  describe('ok', () => {
    test('should return true for 2xx status codes', () => {
      for (const status of [200, 201, 204, 299]) {
        const response = createMockResponse({ status });
        const apiResponse = new ApiResponse(response);
        expect(apiResponse.ok).toBe(true);
      }
    });

    test('should return false for non-2xx status codes', () => {
      for (const status of [400, 401, 500, 503]) {
        const response = createMockResponse({ status });
        const apiResponse = new ApiResponse(response);
        expect(apiResponse.ok).toBe(false);
      }
    });
  });

  describe('isClientError', () => {
    test('should return true for 4xx status codes', () => {
      for (const status of [400, 401, 403, 404, 429, 499]) {
        const response = createMockResponse({ status });
        const apiResponse = new ApiResponse(response);
        expect(apiResponse.isClientError).toBe(true);
      }
    });

    test('should return false for non-4xx status codes', () => {
      for (const status of [200, 301, 500, 503]) {
        const response = createMockResponse({ status });
        const apiResponse = new ApiResponse(response);
        expect(apiResponse.isClientError).toBe(false);
      }
    });
  });

  describe('isServerError', () => {
    test('should return true for 5xx status codes', () => {
      for (const status of [500, 502, 503, 504, 599]) {
        const response = createMockResponse({ status });
        const apiResponse = new ApiResponse(response);
        expect(apiResponse.isServerError).toBe(true);
      }
    });

    test('should return false for non-5xx status codes', () => {
      for (const status of [200, 301, 400, 404]) {
        const response = createMockResponse({ status });
        const apiResponse = new ApiResponse(response);
        expect(apiResponse.isServerError).toBe(false);
      }
    });
  });

  describe('isRetryable', () => {
    test('should return true for retryable status codes', () => {
      const retryableCodes = [408, 429, 500, 502, 503, 504];
      for (const status of retryableCodes) {
        const response = createMockResponse({ status });
        const apiResponse = new ApiResponse(response);
        expect(apiResponse.isRetryable).toBe(true);
      }
    });

    test('should return false for non-retryable status codes', () => {
      const nonRetryableCodes = [400, 401, 403, 404, 501];
      for (const status of nonRetryableCodes) {
        const response = createMockResponse({ status });
        const apiResponse = new ApiResponse(response);
        expect(apiResponse.isRetryable).toBe(false);
      }
    });
  });

  describe('retryAfter', () => {
    test('should return parsed Retry-After header in seconds', () => {
      const response = createMockResponse({
        status: 429,
        headers: { 'retry-after': '120' },
      });
      const apiResponse = new ApiResponse(response);
      expect(apiResponse.retryAfter).toBe(120);
    });

    test('should return null when Retry-After header is missing', () => {
      const response = createMockResponse({ status: 429 });
      const apiResponse = new ApiResponse(response);
      expect(apiResponse.retryAfter).toBeNull();
    });

    test('should return 60 for unparseable Retry-After values', () => {
      const response = createMockResponse({
        status: 429,
        headers: { 'retry-after': 'Wed, 21 Oct 2025 07:28:00 GMT' },
      });
      const apiResponse = new ApiResponse(response);
      expect(apiResponse.retryAfter).toBe(60);
    });
  });

  describe('toDict', () => {
    test('should convert response to dictionary format', () => {
      const response = createMockResponse({
        status: 200,
        body: { result: 'success' },
        headers: { 'content-type': 'application/json' },
      });
      const apiResponse = new ApiResponse(response, { result: 'success' });

      const dict = apiResponse.toDict();
      expect(dict.status_code).toBe(200);
      expect(dict.body).toEqual({ result: 'success' });
      expect(dict.headers).toBeDefined();
    });
  });
});

// =============================================================================
// ApiHandler Tests
// =============================================================================

describe('ApiHandler', () => {
  let handler: TestApiHandler;
  let originalFetch: typeof global.fetch;

  beforeEach(() => {
    handler = new TestApiHandler();
    originalFetch = global.fetch;
  });

  describe('static properties', () => {
    test('should have correct handler name', () => {
      expect(TestApiHandler.handlerName).toBe('test_api_handler');
    });

    test('should have correct base URL', () => {
      expect(TestApiHandler.baseUrl).toBe('https://api.example.com');
    });

    test('should have correct default headers', () => {
      expect(TestApiHandler.defaultHeaders).toEqual({ 'X-Api-Key': 'test-key' });
    });

    test('should have correct default timeout', () => {
      expect(TestApiHandler.defaultTimeout).toBe(5000);
    });
  });

  describe('capabilities', () => {
    test('should include http and api capabilities', () => {
      const caps = handler.capabilities;
      expect(caps).toContain('process');
      expect(caps).toContain('http');
      expect(caps).toContain('api');
    });
  });

  describe('HTTP methods', () => {
    test('GET should make request with correct parameters', async () => {
      const mockFetch = mock(async () =>
        createMockResponse({ status: 200, body: { data: 'test' } })
      );
      global.fetch = mockFetch;

      await handler.testGet('/users', { page: 1, limit: 10 });

      expect(mockFetch).toHaveBeenCalled();
      const [url, options] = mockFetch.mock.calls[0] as [string, RequestInit];
      expect(url).toBe('https://api.example.com/users?page=1&limit=10');
      expect(options.method).toBe('GET');
      expect((options.headers as Record<string, string>)['X-Api-Key']).toBe('test-key');

      global.fetch = originalFetch;
    });

    test('POST should make request with JSON body', async () => {
      const mockFetch = mock(async () => createMockResponse({ status: 201, body: { id: 1 } }));
      global.fetch = mockFetch;

      await handler.testPost('/users', { body: { name: 'Alice' } });

      expect(mockFetch).toHaveBeenCalled();
      const [url, options] = mockFetch.mock.calls[0] as [string, RequestInit];
      expect(url).toBe('https://api.example.com/users');
      expect(options.method).toBe('POST');
      expect(options.body).toBe('{"name":"Alice"}');
      expect((options.headers as Record<string, string>)['Content-Type']).toBe('application/json');

      global.fetch = originalFetch;
    });

    test('PUT should make request with correct method', async () => {
      const mockFetch = mock(async () => createMockResponse({ status: 200 }));
      global.fetch = mockFetch;

      await handler.testPut('/users/1', { body: { name: 'Bob' } });

      const [_, options] = mockFetch.mock.calls[0] as [string, RequestInit];
      expect(options.method).toBe('PUT');

      global.fetch = originalFetch;
    });

    test('PATCH should make request with correct method', async () => {
      const mockFetch = mock(async () => createMockResponse({ status: 200 }));
      global.fetch = mockFetch;

      await handler.testPatch('/users/1', { body: { name: 'Updated' } });

      const [_, options] = mockFetch.mock.calls[0] as [string, RequestInit];
      expect(options.method).toBe('PATCH');

      global.fetch = originalFetch;
    });

    test('DELETE should make request with correct method', async () => {
      const mockFetch = mock(async () => createMockResponse({ status: 204 }));
      global.fetch = mockFetch;

      await handler.testDelete('/users/1');

      const [url, options] = mockFetch.mock.calls[0] as [string, RequestInit];
      expect(url).toBe('https://api.example.com/users/1');
      expect(options.method).toBe('DELETE');

      global.fetch = originalFetch;
    });
  });

  describe('apiSuccess', () => {
    test('should create success result from response', () => {
      const response = createMockResponse({ status: 200, body: { data: 'test' } });
      const apiResponse = new ApiResponse(response, { data: 'test' });

      const result = handler.testApiSuccess(apiResponse);

      expect(result.success).toBe(true);
      expect(result.result).toEqual({ data: 'test' });
      expect(result.metadata?.status_code).toBe(200);
    });

    test('should use custom result when provided', () => {
      const response = createMockResponse({ status: 200 });
      const apiResponse = new ApiResponse(response);

      const result = handler.testApiSuccess(apiResponse, { custom: 'data' });

      expect(result.result).toEqual({ custom: 'data' });
    });

    test('should exclude response metadata when includeResponse is false', () => {
      const response = createMockResponse({ status: 200 });
      const apiResponse = new ApiResponse(response);

      const result = handler.testApiSuccess(apiResponse, undefined, false);

      expect(result.metadata?.status_code).toBeUndefined();
    });
  });

  describe('apiFailure', () => {
    test('should create failure result with error classification', () => {
      const response = createMockResponse({ status: 404, body: { error: 'Not found' } });
      const apiResponse = new ApiResponse(response, { error: 'Not found' });

      const result = handler.testApiFailure(apiResponse);

      expect(result.success).toBe(false);
      expect(result.errorType).toBe('not_found');
      expect(result.retryable).toBe(false);
      expect(result.metadata?.status_code).toBe(404);
    });

    test('should mark retryable errors appropriately', () => {
      const response = createMockResponse({ status: 503 });
      const apiResponse = new ApiResponse(response);

      const result = handler.testApiFailure(apiResponse);

      expect(result.retryable).toBe(true);
      expect(result.errorType).toBe('service_unavailable');
    });

    test('should include retry_after_seconds when present', () => {
      const response = createMockResponse({
        status: 429,
        headers: { 'retry-after': '60' },
      });
      const apiResponse = new ApiResponse(response);

      const result = handler.testApiFailure(apiResponse);

      expect(result.metadata?.retry_after_seconds).toBe(60);
    });

    test('should classify various HTTP status codes correctly', () => {
      const statusErrorMap: Record<number, string> = {
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

      for (const [status, expectedType] of Object.entries(statusErrorMap)) {
        const response = createMockResponse({ status: Number(status) });
        const apiResponse = new ApiResponse(response);
        const result = handler.testApiFailure(apiResponse);
        expect(result.errorType).toBe(expectedType);
      }
    });

    test('should use custom error message when provided', () => {
      const response = createMockResponse({ status: 500 });
      const apiResponse = new ApiResponse(response);

      const result = handler.testApiFailure(apiResponse, 'Custom error message');

      expect(result.errorMessage).toBe('Custom error message');
    });

    test('should extract error message from response body', () => {
      const response = createMockResponse({
        status: 400,
        body: { error: 'Validation failed' },
      });
      const apiResponse = new ApiResponse(response, { error: 'Validation failed' });

      const result = handler.testApiFailure(apiResponse);

      expect(result.errorMessage).toBe('HTTP 400: Validation failed');
    });

    test('should handle various error body formats', () => {
      const testCases = [
        { body: { error: 'Error 1' }, expected: 'Error 1' },
        { body: { message: 'Error 2' }, expected: 'Error 2' },
        { body: { detail: 'Error 3' }, expected: 'Error 3' },
        { body: { error_message: 'Error 4' }, expected: 'Error 4' },
        { body: { msg: 'Error 5' }, expected: 'Error 5' },
        { body: { error: { message: 'Nested error' } }, expected: 'Nested error' },
      ];

      for (const { body, expected } of testCases) {
        const response = createMockResponse({ status: 400, body });
        const apiResponse = new ApiResponse(response, body);
        const result = handler.testApiFailure(apiResponse);
        expect(result.errorMessage).toBe(`HTTP 400: ${expected}`);
      }
    });
  });

  describe('connectionError', () => {
    test('should create retryable failure result', () => {
      const error = new Error('ECONNREFUSED');

      const result = handler.testConnectionError(error);

      expect(result.success).toBe(false);
      expect(result.errorType).toBe('connection_error');
      expect(result.retryable).toBe(true);
      expect(result.errorMessage).toContain('Connection error');
    });

    test('should include context in message when provided', () => {
      const error = new Error('ECONNREFUSED');

      const result = handler.testConnectionError(error, 'fetching user data');

      expect(result.errorMessage).toBe('Connection error while fetching user data: ECONNREFUSED');
    });
  });

  describe('timeoutError', () => {
    test('should create retryable failure result', () => {
      const error = new Error('Request timed out');

      const result = handler.testTimeoutError(error);

      expect(result.success).toBe(false);
      expect(result.errorType).toBe('timeout');
      expect(result.retryable).toBe(true);
      expect(result.errorMessage).toContain('Request timeout');
    });

    test('should include context in message when provided', () => {
      const error = new Error('Request timed out');

      const result = handler.testTimeoutError(error, 'processing payment');

      expect(result.errorMessage).toBe(
        'Request timeout while processing payment: Request timed out'
      );
    });
  });

  describe('call method', () => {
    test('should execute and return result', async () => {
      const mockFetch = mock(async () =>
        createMockResponse({ status: 200, body: { success: true } })
      );
      global.fetch = mockFetch;

      const context = new StepContext({
        taskUuid: 'task-1',
        stepUuid: 'step-1',
      });

      const result = await handler.call(context);

      expect(result.success).toBe(true);

      global.fetch = originalFetch;
    });
  });
});

// =============================================================================
// Edge Cases and Integration Tests
// =============================================================================

describe('ApiHandler edge cases', () => {
  let handler: TestApiHandler;
  let originalFetch: typeof global.fetch;

  beforeEach(() => {
    handler = new TestApiHandler();
    originalFetch = global.fetch;
  });

  test('should handle JSON parse errors gracefully', async () => {
    const mockFetch = mock(async () => ({
      status: 200,
      ok: true,
      headers: new Headers({ 'content-type': 'application/json' }),
      json: async () => {
        throw new Error('Invalid JSON');
      },
      text: async () => 'Invalid JSON content',
    }));
    global.fetch = mockFetch as typeof fetch;

    const response = await handler.testGet('/test');

    expect(response.ok).toBe(true);
    expect(response.body).toBe('Invalid JSON content');

    global.fetch = originalFetch;
  });

  test('should handle non-JSON content types', async () => {
    const mockFetch = mock(async () => ({
      status: 200,
      ok: true,
      headers: new Headers({ 'content-type': 'text/plain' }),
      text: async () => 'Plain text response',
    }));
    global.fetch = mockFetch as typeof fetch;

    const response = await handler.testGet('/test');

    expect(response.body).toBe('Plain text response');

    global.fetch = originalFetch;
  });

  test('should build URL with null/undefined params excluded', async () => {
    const mockFetch = mock(async () => createMockResponse({ status: 200 }));
    global.fetch = mockFetch;

    await handler.testGet('/users', { page: 1, filter: null, sort: undefined });

    const [url] = mockFetch.mock.calls[0] as [string];
    expect(url).toBe('https://api.example.com/users?page=1');

    global.fetch = originalFetch;
  });

  test('should merge custom headers with defaults', async () => {
    const mockFetch = mock(async () => createMockResponse({ status: 200 }));
    global.fetch = mockFetch;

    await handler.testGet('/users', undefined, { 'X-Custom': 'value' });

    const [_, options] = mockFetch.mock.calls[0] as [string, RequestInit];
    const headers = options.headers as Record<string, string>;
    expect(headers['X-Api-Key']).toBe('test-key');
    expect(headers['X-Custom']).toBe('value');

    global.fetch = originalFetch;
  });

  test('should handle generic client errors', () => {
    const response = createMockResponse({ status: 418 }); // I'm a teapot
    const apiResponse = new ApiResponse(response);

    const result = handler.testApiFailure(apiResponse);

    expect(result.errorType).toBe('client_error');
  });

  test('should handle generic server errors', () => {
    const response = createMockResponse({ status: 599 });
    const apiResponse = new ApiResponse(response);

    const result = handler.testApiFailure(apiResponse);

    expect(result.errorType).toBe('server_error');
  });
});
