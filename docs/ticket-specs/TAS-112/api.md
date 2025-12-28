# TAS-112 Phase 3: API Handler Pattern Analysis

**Phase**: 3 of 9  
**Created**: 2025-12-27  
**Status**: Analysis Complete  
**Prerequisites**: Phase 2 (Base Handler APIs)

---

## Executive Summary

This analysis compares HTTP/REST API integration patterns across all four languages. **Critical Finding**: Rust has **no dedicated API handler** while Ruby, Python, and TypeScript have mature, feature-complete implementations. The existing implementations are remarkably well-aligned, with only minor divergence in HTTP client libraries and method signatures.

**Key Findings**:
1. **Rust Gap**: No API handler pattern exists - needs implementation
2. **HTTP Clients**: Ruby (Faraday), Python (httpx), TypeScript (native fetch) - all modern choices
3. **Error Classification**: Highly consistent across 400/429/500 status codes
4. **Retry-After Headers**: Universally supported
5. **Architectural Pattern**: All use **subclass inheritance** (contradicts composition goal!)

**Guiding Principle** (Zen of Python): *"There should be one-- and preferably only one --obvious way to do it."*

**Project Context**: Pre-alpha greenfield. Breaking changes encouraged to achieve composition-over-inheritance architecture.

---

## Comparison Matrix

### Handler Class Definition

| Language | Class Name | Inheritance | HTTP Client | File Location |
|----------|-----------|-------------|-------------|---------------|
| **Rust** | ❌ **None** | N/A | N/A | - |
| **Ruby** | `Api` | Subclass of `Base` | Faraday | `lib/tasker_core/step_handler/api.rb` |
| **Python** | `ApiHandler` | Subclass of `StepHandler` | httpx | `python/tasker_core/step_handler/api.py` |
| **TypeScript** | `ApiHandler` | Subclass of `StepHandler` | Native fetch | `src/handler/api.ts` |

**Analysis**: 
- ❌ **All use subclass inheritance** - violates composition-over-inheritance goal
- ✅ HTTP client choices are modern and appropriate
- ❌ Rust has **no API support** - critical gap

**Recommendation**:
- ✅ **Create Rust API mixin/trait** (not subclass)
- ✅ **Migrate Ruby/Python/TypeScript to mixin pattern**
- 📝 **Zen Alignment**: "Flat is better than nested" - mixins over inheritance

---

## HTTP Client Libraries

### Ruby: Faraday

**Library**: [Faraday](https://lfi.github.io/faraday/) - HTTP client abstraction layer

**Key Features**:
- Middleware-based architecture
- Multiple adapter support (Net::HTTP, Typhoeus, etc.)
- Built-in retry, logging, authentication middleware
- Connection pooling

**Example Configuration**:
```ruby
Faraday.new(base_url) do |conn|
  conn.request :json                # Encode body as JSON
  conn.response :json               # Parse response as JSON
  conn.request :authorization, 'Bearer', token
  conn.options.timeout = 30
  conn.adapter Faraday.default_adapter
end
```

**Pros**:
- ✅ Mature, battle-tested (10+ years)
- ✅ Excellent middleware ecosystem
- ✅ Adapter flexibility

**Cons**:
- ⚠️ Verbose configuration
- ⚠️ Middleware ordering matters (footgun potential)

### Python: httpx

**Library**: [httpx](https://www.python-httpx.org/) - Modern async HTTP client

**Key Features**:
- HTTP/2 support
- Async/sync API (we use sync)
- Request/response hooks
- Connection pooling

**Example Configuration**:
```python
httpx.Client(
    base_url="https://api.example.com",
    timeout=30.0,
    headers={"X-API-Key": "secret"}
)
```

**Pros**:
- ✅ Modern, Pythonic API
- ✅ HTTP/2 support
- ✅ Excellent async support (if needed later)
- ✅ Simple, clean API

**Cons**:
- ⚠️ Newer library (less mature than requests)
- ⚠️ Smaller ecosystem

### TypeScript: Native Fetch

**Library**: Native `fetch` API (Bun, Node 18+, browsers)

**Key Features**:
- Standard Web API
- Promise-based
- Built-in to runtime (no dependencies!)
- AbortController for timeouts

**Example Usage**:
```typescript
const response = await fetch(url, {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify(data),
  signal: abortController.signal
});
```

**Pros**:
- ✅ Zero dependencies
- ✅ Standard API (portable across runtimes)
- ✅ Native performance

**Cons**:
- ⚠️ Lower-level (no built-in retry, auth middleware)
- ⚠️ More boilerplate required

### Rust: Recommendation

**Proposed Library**: [reqwest](https://docs.rs/reqwest/) - Async HTTP client

**Why reqwest**:
- Most popular Rust HTTP client (9M+ downloads/month)
- Excellent async support (Tokio-based)
- Middleware support via `tower` layers
- Connection pooling built-in
- Works with our existing async handler architecture

**Alternative**: [ureq](https://docs.rs/ureq/) - Synchronous, simpler but less flexible

**Recommendation**: ✅ **Use reqwest** for feature parity with Python/TypeScript

---

## HTTP Methods

### Method Availability

| Method | Ruby | Python | TypeScript | Rust (Proposed) |
|--------|------|--------|------------|-----------------|
| **GET** | ✅ `get(path, params:, headers:)` | ✅ `get(path, params, headers)` | ✅ `get(path, params, headers)` | ✅ (reqwest) |
| **POST** | ✅ `post(path, data:, headers:)` | ✅ `post(path, json, data, headers)` | ✅ `post(path, {body, headers})` | ✅ (reqwest) |
| **PUT** | ✅ `put(path, data:, headers:)` | ✅ `put(path, json, data, headers)` | ✅ `put(path, {body, headers})` | ✅ (reqwest) |
| **PATCH** | ❌ Not implemented | ✅ `patch(path, json, data, headers)` | ✅ `patch(path, {body, headers})` | ✅ (reqwest) |
| **DELETE** | ✅ `delete(path, params:, headers:)` | ✅ `delete(path, headers)` | ✅ `delete(path, headers)` | ✅ (reqwest) |

**Gap**: Ruby missing `patch()` method

**Recommendation**: ✅ **Add `patch()` to Ruby** for completeness

### Method Signature Comparison

**Ruby (Keyword Arguments)**:
```ruby
get(path, params: {}, headers: {})
post(path, data: {}, headers: {})
put(path, data: {}, headers: {})
delete(path, params: {}, headers: {})
```

**Python (Positional with Named)**:
```python
get(path, params=None, headers=None, **kwargs)
post(path, json=None, data=None, headers=None, **kwargs)
put(path, json=None, data=None, headers=None, **kwargs)
patch(path, json=None, data=None, headers=None, **kwargs)
delete(path, headers=None, **kwargs)
```

**TypeScript (Options Object)**:
```typescript
get(path, params?, headers?)
post(path, {body?, json?, headers?}?)
put(path, {body?, json?, headers?}?)
patch(path, {body?, json?, headers?}?)
delete(path, headers?)
```

**Analysis**:
- **Inconsistency**: Three different parameter patterns
- **Ruby**: Keyword-required (verbose but explicit)
- **Python**: Flexible positional/keyword with `**kwargs` escape hatch
- **TypeScript**: Options object pattern (common in TypeScript/JavaScript)

**Recommendation**:
- ✅ **Accept differences** - idiomatic to each language
- 📝 **Zen Alignment**: "Although practicality beats purity" - language idioms win

---

## Error Classification

### HTTP Status Code Mapping

All three implementations have remarkably consistent error classification:

| Status Code | Category | Retryable | Ruby | Python | TypeScript |
|-------------|----------|-----------|------|--------|------------|
| **400** | Bad Request | ❌ No | PermanentError | bad_request | bad_request |
| **401** | Unauthorized | ❌ No | PermanentError | unauthorized | unauthorized |
| **403** | Forbidden | ❌ No | PermanentError | forbidden | forbidden |
| **404** | Not Found | ❌ No | PermanentError | not_found | not_found |
| **405** | Method Not Allowed | ❌ No | - | method_not_allowed | method_not_allowed |
| **408** | Request Timeout | ✅ Yes | - | request_timeout | request_timeout |
| **422** | Unprocessable Entity | ❌ No | PermanentError | unprocessable_entity | unprocessable_entity |
| **429** | Rate Limited | ✅ Yes | RetryableError | rate_limited | rate_limited |
| **500** | Internal Server Error | ✅ Yes | RetryableError | internal_server_error | internal_server_error |
| **502** | Bad Gateway | ✅ Yes | RetryableError | bad_gateway | bad_gateway |
| **503** | Service Unavailable | ✅ Yes | RetryableError | service_unavailable | service_unavailable |
| **504** | Gateway Timeout | ✅ Yes | RetryableError | gateway_timeout | gateway_timeout |

**Analysis**:
- ✅ **Excellent alignment** - classification is nearly identical
- ⚠️ Ruby missing some specific 4xx codes (408, 405)
- ✅ All treat 429/503 as retryable with special handling

**Recommendation**:
- ✅ **Standardize**: Use Python/TypeScript's granular error types
- ✅ **Ruby**: Add missing 408, 405 classifications
- 📝 **Zen Alignment**: "There should be one obvious way" - standardized classification

### Error Classification Code

**Ruby**:
```ruby
case response.status
when 400, 401, 403, 404, 422
  raise TaskerCore::PermanentError.new(...)
when 429
  retry_after = extract_retry_after_header(response.headers)
  raise TaskerCore::RetryableError.new(..., retry_after: retry_after)
when 503
  retry_after = extract_retry_after_header(response.headers)
  raise TaskerCore::RetryableError.new(..., retry_after: retry_after)
when 500..599
  raise TaskerCore::RetryableError.new(...)
end
```

**Python**:
```python
NON_RETRYABLE_STATUS_CODES = {400, 401, 403, 404, 405, 406, 410, 422}
RETRYABLE_STATUS_CODES = {408, 429, 500, 502, 503, 504}

def _classify_error(self, response):
    error_type_map = {
        400: "bad_request",
        401: "unauthorized",
        # ... specific mappings
        429: "rate_limited",
        503: "service_unavailable",
    }
    return error_type_map.get(status_code, "http_error")
```

**TypeScript**:
```typescript
const RETRYABLE_STATUS_CODES = new Set([408, 429, 500, 502, 503, 504]);

private classifyError(response: ApiResponse): string {
  const errorTypeMap: Record<number, string> = {
    400: 'bad_request',
    401: 'unauthorized',
    // ... specific mappings
  };
  return errorTypeMap[statusCode] || 'http_error';
}
```

**Recommendation**:
- ✅ **Standardize**: Use Python/TypeScript's map-based approach (clearer than case/when)
- ✅ **Unified Constants**: Define standard lists of retryable/non-retryable codes
- 📝 **Documentation**: Create shared error classification reference

---

## Retry-After Header Support

### Implementation Comparison

**Ruby**:
```ruby
def extract_retry_after_header(headers)
  TaskerCore::ErrorClassification.extract_retry_after(headers)
end

# In error raising:
retry_after = extract_retry_after_header(response.headers)
raise TaskerCore::RetryableError.new(
  "Rate limited",
  retry_after: retry_after,  # Passed to error object
  error_category: 'rate_limit'
)
```

**Python**:
```python
@property
def retry_after(self) -> int | None:
    """Get Retry-After header value in seconds."""
    retry_after = self.headers.get("retry-after")
    if retry_after is None:
        return None
    try:
        return int(retry_after)
    except ValueError:
        # Could be date format, return default
        return 60

# In failure result:
if response.retry_after is not None:
    metadata["retry_after_seconds"] = response.retry_after
```

**TypeScript**:
```typescript
get retryAfter(): number | null {
  const retryAfter = this.headers['retry-after'];
  if (!retryAfter) {
    return null;
  }
  const parsed = Number.parseInt(retryAfter, 10);
  return Number.isNaN(parsed) ? 60 : parsed;
}

// In failure result:
if (response.retryAfter !== null) {
  metadata.retry_after_seconds = response.retryAfter;
}
```

**Analysis**:
- ✅ All three implement Retry-After parsing
- ✅ All default to 60 seconds for date-format headers (pragmatic)
- ✅ All expose via response wrapper or metadata
- ⚠️ Ruby delegates to `ErrorClassification` module (less discoverable)

**Recommendation**:
- ✅ **Consistency**: All implementations are functionally equivalent
- ✅ **Python/TypeScript pattern preferred**: Property on response object
- 📝 **Zen Alignment**: "Simple is better than complex" - property > delegation

---

## Response Wrapper Types

### Ruby: Faraday::Response (3rd-party)

**No custom wrapper** - uses Faraday's response object directly

**Properties**:
```ruby
response.status         # HTTP status code
response.headers        # Header hash
response.body           # Parsed body (via middleware)
response.success?       # 2xx check
response.reason_phrase  # Status message
```

**Analysis**: No wrapper needed - Faraday provides rich API

### Python: ApiResponse (Custom)

**Custom wrapper** around httpx.Response:

```python
class ApiResponse:
    status_code: int
    headers: dict[str, str]
    body: dict | str
    raw_response: httpx.Response
    
    @property
    def ok(self) -> bool:
        return 200 <= self.status_code < 300
    
    @property
    def is_client_error(self) -> bool
    @property
    def is_server_error(self) -> bool
    @property
    def is_retryable(self) -> bool
    @property
    def retry_after(self) -> int | None
```

**Analysis**: 
- ✅ Clean abstraction
- ✅ Helper properties for classification
- ✅ Automatic JSON parsing

### TypeScript: ApiResponse (Custom)

**Custom wrapper** around fetch Response:

```typescript
class ApiResponse {
  readonly statusCode: number;
  readonly headers: Record<string, string>;
  readonly body: unknown;
  readonly rawResponse: Response;
  
  get ok(): boolean { return this.statusCode >= 200 && this.statusCode < 300; }
  get isClientError(): boolean { ... }
  get isServerError(): boolean { ... }
  get isRetryable(): boolean { ... }
  get retryAfter(): number | null { ... }
}
```

**Analysis**:
- ✅ Nearly identical to Python (excellent alignment!)
- ✅ TypeScript naming conventions (camelCase)
- ✅ Helper getters for classification

**Recommendation**:
- ✅ **Python/TypeScript pattern is excellent** - adopt for Rust
- 📝 **Rust**: Create `ApiResponse<T>` struct with same helper methods
- 📝 **Zen Alignment**: "Beautiful is better than ugly" - wrapper provides clean API

---

## Result Helper Methods

### Success Helper

**Ruby**:
```ruby
def api_success(response)
  # Not present! Must manually construct success()
  success(result: parse_response_body(response))
end
```
❌ **Missing** - no dedicated API success helper

**Python**:
```python
def api_success(
    self,
    response: ApiResponse,
    result: dict | None = None,
    include_response: bool = True
) -> StepHandlerResult:
    if result is None:
        result = response.body if isinstance(response.body, dict) else {"data": response.body}
    
    metadata = {}
    if include_response:
        metadata["status_code"] = response.status_code
        metadata["headers"] = response.headers
    
    return self.success(result, metadata=metadata)
```

**TypeScript**:
```typescript
protected apiSuccess(
  response: ApiResponse,
  result?: Record<string, unknown>,
  includeResponse = true
): StepHandlerResult {
  const resultData = result || 
    (typeof response.body === 'object' ? response.body : { data: response.body });
  
  const metadata = {};
  if (includeResponse) {
    metadata.status_code = response.statusCode;
    metadata.headers = response.headers;
  }
  
  return this.success(resultData, metadata);
}
```

**Analysis**:
- ❌ **Ruby Gap**: No `api_success()` helper
- ✅ **Python/TypeScript**: Identical functionality
- ✅ Auto-wraps non-dict responses in `{data: ...}`

**Recommendation**:
- ✅ **Add to Ruby**: Implement `api_success()` helper
- ✅ **Add to Rust**: Follow Python/TypeScript pattern
- 📝 **Zen Alignment**: "Simple is better than complex" - helper reduces boilerplate

### Failure Helper

**Ruby**:
```ruby
# No api_failure() helper - process_response() raises exceptions
def process_response(response)
  return response if response.success?
  
  case response.status
  when 400, 401, 403, 404, 422
    raise TaskerCore::PermanentError.new(...)
  when 429
    raise TaskerCore::RetryableError.new(..., retry_after: ...)
  # ... more cases
  end
end
```
⚠️ **Different Pattern**: Raises exceptions instead of returning failure result

**Python**:
```python
def api_failure(
    self,
    response: ApiResponse,
    message: str | None = None,
) -> StepHandlerResult:
    error_type = self._classify_error(response)
    message = message or self._format_error_message(response)
    retryable = response.is_retryable
    
    metadata = {
        "status_code": response.status_code,
        "headers": response.headers,
    }
    if response.retry_after:
        metadata["retry_after_seconds"] = response.retry_after
    if response.body:
        metadata["response_body"] = response.body
    
    return self.failure(message, error_type, retryable, metadata)
```

**TypeScript**:
```typescript
protected apiFailure(response: ApiResponse, message?: string): StepHandlerResult {
  const errorType = this.classifyError(response);
  const errorMessage = message || this.formatErrorMessage(response);
  const retryable = response.isRetryable;
  
  const metadata = {
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
```

**Analysis**:
- ❌ **Ruby Different Pattern**: Raises exceptions vs returns results
- ✅ **Python/TypeScript**: Identical implementations
- ⚠️ **Architectural Inconsistency**: Ruby's raise-based approach differs from base handler pattern

**Recommendation**:
- ✅ **Standardize on result-based**: Python/TypeScript pattern
- ⚠️ **Ruby Refactor**: Add `api_failure()` helper, deprecate exception-raising in `process_response()`
- 📝 **Zen Alignment**: "There should be one obvious way" - return results consistently

---

## Special Features

### Ruby Advanced Features

**File Upload**:
```ruby
def api_upload_file(path, file_path, field_name: 'file', additional_fields: {})
  payload = additional_fields.merge({
    field_name => Faraday::Multipart::FilePart.new(file_path, ...)
  })
  http_request(:post, path, multipart: payload)
end
```

**Pagination**:
```ruby
def api_paginated_request(path, method: :get, pagination_key: 'cursor', 
                           limit_key: 'limit', max_pages: 100)
  results = []
  pagination_value = nil
  
  loop do
    # Fetch page
    params = { limit_key => page_size }
    params[pagination_key] = pagination_value if pagination_value
    response = http_request(method, path, params: params)
    
    # Extract results
    results.concat(response[:data])
    
    # Get next cursor
    pagination_value = extract_pagination_cursor(response, pagination_key)
    break unless pagination_value
  end
  
  results
end
```

**Analysis**:
- ✅ **Ruby has advanced features** Python/TypeScript lack
- ⚠️ **Inconsistent**: These features are Ruby-only
- 📝 **Decision Needed**: Should these be universal or Ruby-specific?

**Recommendation**:
- 🎯 **Phase 2 Priority**: Start with core HTTP methods
- 🔄 **Future**: Add pagination/upload as optional mixins (composition!)
- 📝 **Zen Alignment**: "Simple is better than complex" - core features first

### Connection Configuration

**Ruby (Extensive)**:
```ruby
def apply_connection_config(conn)
  conn.options.timeout = config[:timeout] || api_timeouts[:timeout]
  conn.options.open_timeout = config[:open_timeout] || api_timeouts[:open_timeout]
  
  # SSL
  if (ssl_config = config[:ssl])
    conn.ssl.merge!(ssl_config)
  end
  
  # Headers
  if (headers = config[:headers])
    headers.each { |k, v| conn.headers[k] = v }
  end
  
  # Auth (bearer, basic, api_key)
  apply_authentication(conn)
end
```

**Python (Minimal)**:
```python
httpx.Client(
    base_url=self.base_url,
    timeout=self.default_timeout,
    headers=self.default_headers,
)
```

**TypeScript (Minimal)**:
```typescript
// No connection object - uses fetch directly
// Configuration via static class properties
```

**Analysis**:
- ✅ **Ruby**: Most configurable (timeout, SSL, auth, headers, params)
- ⚠️ **Python/TypeScript**: Minimal config (class properties only)
- 📝 **Trade-off**: Flexibility vs simplicity

**Recommendation**:
- ✅ **Ruby's configurability is good** - keep it
- ✅ **Python/TypeScript**: Add constructor config support
- 📝 **Rust**: Support rich configuration (reqwest provides this)
- 📝 **Zen Alignment**: "Although practicality beats purity" - flexibility needed for real-world use

---

## Architecture Anti-Pattern

### All Use Subclass Inheritance

**Ruby**:
```ruby
class Api < Base  # ❌ Inheritance
  # ...
end
```

**Python**:
```python
class ApiHandler(StepHandler):  # ❌ Inheritance
  # ...
```

**TypeScript**:
```typescript
export abstract class ApiHandler extends StepHandler {  # ❌ Inheritance
  // ...
}
```

**Problem**: This violates the project's composition-over-inheritance goal!

**Impact**:
- Cannot combine API features with other handler types easily
- Forces single inheritance hierarchy
- Harder to test/mock
- Tight coupling

**Recommendation** (Composition Pattern):

**Ruby (Mixin)**:
```ruby
module StepHandler
  module ApiCapabilities
    include Concerns::HttpMethods
    include Concerns::ErrorClassification
    
    def get(path, params: {}, headers: {})
      # Implementation
    end
  end
end

class MyApiHandler < Base
  include StepHandler::ApiCapabilities  # ✅ Composition
end
```

**Python (Mixin)**:
```python
class ApiMixin:
    """Provides API capabilities via composition."""
    def get(self, path, params=None, headers=None):
        # Implementation
    
class MyApiHandler(StepHandler, ApiMixin):  # ✅ Composition
    pass
```

**TypeScript (Interface + Helper)**:
```typescript
interface ApiCapable {
  get(path: string, params?: Record<string, unknown>): Promise<ApiResponse>;
}

// Helper function or trait-like implementation
function withApiCapabilities(handler: StepHandler): ApiCapable {
  // Implementation
}
```

**Rust (Trait)**:
```rust
pub trait ApiCapable {
    async fn get(&self, path: &str) -> Result<ApiResponse>;
    async fn post(&self, path: &str, body: Value) -> Result<ApiResponse>;
}

// Implement for any handler
impl ApiCapable for MyHandler {
    // Implementation
}
```

**Recommendation**:
- ✅ **Adopt Now**: Migrate all languages to composition pattern
- ✅ **Breaking Change Acceptable**: Pre-alpha status allows this
- 📝 **Zen Alignment**: "Flat is better than nested" - composition over inheritance

---

## Functional Gaps

### Rust (Critical Gaps)
1. ❌ **No API handler at all** - needs full implementation
2. ❌ **No HTTP client integration** - need reqwest
3. ❌ **No error classification** - need status code mapping
4. ❌ **No Retry-After support** - need header parsing

### Ruby (Minor Gaps)
1. ⚠️ **Missing PATCH method** - easy to add
2. ⚠️ **No `api_success()` helper** - should add
3. ⚠️ **No `api_failure()` helper** - uses exceptions instead
4. ⚠️ **Missing 408, 405 error classifications** - should add

### Python (Complete)
1. ✅ **Fully featured** - reference implementation
2. ✅ **All HTTP methods** including PATCH
3. ✅ **Helper methods** for success/failure
4. ✅ **Comprehensive error classification**

### TypeScript (Complete)
1. ✅ **Fully featured** - matches Python
2. ✅ **All HTTP methods** including PATCH
3. ✅ **Helper methods** for success/failure
4. ✅ **Zero dependencies** (uses native fetch)

---

## Recommendations Summary

### Critical Changes (Implement Now)

#### 1. Rust API Handler Implementation
- ✅ Create `ApiCapable` trait (not subclass!)
- ✅ Use `reqwest` HTTP client
- ✅ Implement `ApiResponse<T>` wrapper type
- ✅ Add HTTP methods: GET, POST, PUT, PATCH, DELETE
- ✅ Implement error classification (match Python/TypeScript)
- ✅ Add Retry-After header support
- ✅ Add `api_success()` and `api_failure()` helpers

**Implementation Priority**: **HIGH** - critical missing feature

#### 2. Migration to Composition Pattern
All languages must migrate from subclass inheritance to composition:

**Ruby**:
- ✅ Create `StepHandler::ApiCapabilities` module
- ✅ Extract HTTP methods to mixin
- ✅ Deprecate `Api` subclass pattern
- ✅ Update documentation and examples

**Python**:
- ✅ Refactor `ApiHandler` to `ApiMixin`
- ✅ Use multiple inheritance: `class MyHandler(StepHandler, ApiMixin)`
- ✅ Update documentation

**TypeScript**:
- ✅ Extract API functionality to composable helper
- ✅ Consider decorator pattern or traits (via mixins)
- ✅ Update documentation

**Rust**:
- ✅ Implement as trait from the start (already composition-friendly)

#### 3. Ruby Enhancements
- ✅ Add `patch()` method
- ✅ Add `api_success()` helper
- ✅ Add `api_failure()` helper (complement exception-based approach)
- ✅ Add missing status code classifications (408, 405)

#### 4. Cross-Language Standardization
- ✅ Standardize error classification codes across all languages
- ✅ Document standard HTTP status code → error type mapping
- ✅ Align Retry-After handling patterns
- ✅ Create shared error classification reference document

### Documentation Requirements

1. **API Handler Reference Guide**:
   - HTTP method signatures per language
   - Error classification table
   - Retry-After handling examples
   - Response wrapper usage

2. **Composition Pattern Guide**:
   - How to use API capabilities via composition
   - Migration examples from inheritance to composition
   - Best practices for combining multiple capabilities

3. **HTTP Client Comparison**:
   - Feature matrix (Faraday vs httpx vs fetch vs reqwest)
   - Configuration examples
   - Performance characteristics

---

## Implementation Checklist

### Rust API Handler (New Implementation)
- [ ] Add `reqwest` dependency to Cargo.toml
- [ ] Create `ApiCapable` trait in separate module
- [ ] Implement `ApiResponse<T>` struct
- [ ] Implement HTTP method helpers (GET, POST, PUT, PATCH, DELETE)
- [ ] Implement error classification function
- [ ] Implement Retry-After header parsing
- [ ] Add `api_success()` and `api_failure()` helpers
- [ ] Add example handler using ApiCapable
- [ ] Add integration tests
- [ ] Update documentation

### Ruby Composition Migration
- [ ] Create `StepHandler::ApiCapabilities` module
- [ ] Move HTTP methods to module
- [ ] Move error classification to module
- [ ] Add `patch()` method
- [ ] Add `api_success()` helper
- [ ] Add `api_failure()` helper
- [ ] Deprecate `Api` class
- [ ] Update all examples to use mixin
- [ ] Add migration guide

### Python Composition Migration
- [ ] Rename `ApiHandler` → `ApiMixin`
- [ ] Keep class functional (backward compat in pre-alpha)
- [ ] Update examples to show composition
- [ ] Document mixin usage
- [ ] Add tests for composition pattern

### TypeScript Composition Migration
- [ ] Extract API functionality to composable helper
- [ ] Implement decorator or mixin pattern
- [ ] Update examples
- [ ] Document composition approach

### Documentation
- [ ] Create `docs/api-handlers-reference.md`
- [ ] Create `docs/composition-patterns.md`
- [ ] Update `docs/worker-crates/*.md` with composition examples
- [ ] Add HTTP status code classification table to docs

---

## Next Phase

**Phase 4: Decision Handler Pattern** will analyze conditional workflow routing handlers, building on the composition patterns established here. Key questions:
- Are decision handlers also using subclass inheritance?
- Is the `DecisionPointOutcome` structure consistent?
- Do all languages support both `create_steps` and `no_branches`?

---

## Appendix: Rust API Handler Sketch

**Trait Definition**:
```rust
use reqwest::{Client, Response};
use anyhow::Result;
use serde_json::Value;

#[async_trait]
pub trait ApiCapable {
    fn http_client(&self) -> &Client;
    
    async fn get(&self, path: &str, params: Option<Value>) -> Result<ApiResponse> {
        let client = self.http_client();
        let response = client.get(path)
            .query(&params.unwrap_or_default())
            .send()
            .await?;
        ApiResponse::from_response(response).await
    }
    
    async fn post(&self, path: &str, body: Value) -> Result<ApiResponse> {
        let client = self.http_client();
        let response = client.post(path)
            .json(&body)
            .send()
            .await?;
        ApiResponse::from_response(response).await
    }
    
    // ... other HTTP methods
    
    fn api_success(&self, response: ApiResponse) -> StepExecutionResult {
        let result = response.body.unwrap_or_default();
        let metadata = json!({
            "status_code": response.status_code,
            "headers": response.headers
        });
        success_result(self.step_uuid(), result, self.elapsed_ms(), Some(metadata))
    }
    
    fn api_failure(&self, response: ApiResponse) -> StepExecutionResult {
        let error_type = classify_http_error(response.status_code);
        let retryable = is_retryable_status(response.status_code);
        failure_result(
            self.step_uuid(),
            format!("HTTP {}: {}", response.status_code, response.status_text),
            error_type,
            retryable,
            Some(json!({"status_code": response.status_code}))
        )
    }
}

pub struct ApiResponse {
    pub status_code: u16,
    pub headers: HashMap<String, String>,
    pub body: Option<Value>,
    pub status_text: String,
}

impl ApiResponse {
    pub async fn from_response(response: Response) -> Result<Self> {
        let status_code = response.status().as_u16();
        let status_text = response.status().canonical_reason().unwrap_or("").to_string();
        let headers = response.headers().iter()
            .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        
        let body = response.json().await.ok();
        
        Ok(Self { status_code, headers, body, status_text })
    }
    
    pub fn ok(&self) -> bool {
        (200..300).contains(&self.status_code)
    }
    
    pub fn is_retryable(&self) -> bool {
        matches!(self.status_code, 408 | 429 | 500 | 502 | 503 | 504)
    }
    
    pub fn retry_after(&self) -> Option<u64> {
        self.headers.get("retry-after")
            .and_then(|v| v.parse::<u64>().ok())
            .or(Some(60)) // Default for date formats
    }
}
```

---

## Metadata

**Document Version**: 1.0  
**Analysis Date**: 2025-12-27  
**Reviewers**: TBD  
**Approval Status**: Draft
