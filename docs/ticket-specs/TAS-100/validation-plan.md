# TAS-100 Validation Plan

**Parent**: [TAS-100](./README.md)
**Status**: Planning

## Overview

Comprehensive validation checklist to be executed after all TAS-100 child tickets (TAS-101 through TAS-107) are complete. This ensures the TypeScript worker meets all requirements and maintains cross-language consistency with Ruby, Python, and Rust workers.

## Validation Categories

### 1. FFI Integration (TAS-101)

**Bun Runtime**:
- [ ] FFI adapter loads `libtasker_worker` successfully
- [ ] `pollStepEvents()` returns events correctly
- [ ] `completeStepEvent()` sends completions without errors
- [ ] Memory management (no leaks, proper string freeing)
- [ ] Handles null pointers safely

**Node.js Runtime**:
- [ ] FFI adapter loads `libtasker_worker` successfully
- [ ] `pollStepEvents()` returns events correctly
- [ ] `completeStepEvent()` sends completions without errors
- [ ] Memory management (no leaks, proper string freeing)
- [ ] Handles null pointers safely

**Common**:
- [ ] Runtime detection correctly identifies Bun and Node
- [ ] EventPoller polls at 10ms intervals
- [ ] EventEmitter pub/sub works correctly
- [ ] Bootstrap/shutdown lifecycle is clean

### 2. Handler API (TAS-102)

- [ ] `StepContext` provides all TAS-92 standard fields
- [ ] Handler signature: `call(context: StepContext): Promise<StepHandlerResult>`
- [ ] `success(result, metadata?)` matches cross-language API
- [ ] `failure(message, errorType, retryable, ...)` matches cross-language API
- [ ] Error types defined: `permanent_error`, `retryable_error`, etc.
- [ ] HandlerRegistry implements: `register()`, `isRegistered()`, `resolve()`, `listHandlers()`
- [ ] StepExecutionSubscriber routes events to handlers

### 3. Specialized Handlers (TAS-103)

- [ ] ApiHandler has `get()`, `post()`, `put()`, `delete()` methods
- [ ] ApiHandler classifies HTTP errors correctly (4xx → permanent, 5xx → retryable)
- [ ] DecisionHandler has `decisionSuccess(steps, routingContext)` helper
- [ ] Batchable has `batchWorkerSuccess()` and `getBatchContext()`
- [ ] Field names standardized: `items_processed`, `items_succeeded`, `items_failed`

### 4. Server and Bootstrap (TAS-104)

- [ ] `bin/server.ts` starts worker successfully
- [ ] Signal handlers (SIGTERM, SIGINT) trigger graceful shutdown
- [ ] Bootstrap API exported: `bootstrapWorker()`, `stopWorker()`, `getWorkerStatus()`
- [ ] Headless mode works without HTTP server
- [ ] Configuration loaded from TOML
- [ ] Health checks work correctly

### 5. Testing (TAS-105)

- [ ] Unit tests pass on Bun runtime
- [ ] Unit tests pass on Node.js runtime
- [ ] Integration tests cover full lifecycle
- [ ] Test coverage >80%
- [ ] Example handlers demonstrate all patterns
- [ ] No test flakiness

### 6. Documentation (TAS-107)

- [ ] `typescript.md` complete with all sections
- [ ] Cross-language comparison updated
- [ ] Code examples tested and working
- [ ] API documentation complete
- [ ] Migration guides available

## Cross-Language Consistency (TAS-92 Alignment)

### Handler Signature
- [ ] Ruby: `call(context)` (post-TAS-96) ✅
- [ ] Python: `call(context)` ✅
- [ ] Rust: `call(&TaskSequenceStep)` ✅
- [ ] **TypeScript**: `call(context)` ✅

### Result Factories
- [ ] Ruby: `success()`, `failure()` ✅
- [ ] Python: `success()`, `failure()` ✅
- [ ] Rust: `StepExecutionResult::success()`, `::failure()` ✅
- [ ] **TypeScript**: `success()`, `failure()` ✅

### Registry API
- [ ] All languages implement: `register()`, `isRegistered()`, `resolve()`, `listHandlers()`
- [ ] **TypeScript** matches this API

### Error Fields
- [ ] All languages have: `errorMessage`, `errorType`, `errorCode`, `retryable`
- [ ] **TypeScript** has these fields

## End-to-End Validation

### Scenario 1: Simple Handler
```typescript
// Register handler
registry.register('test_handler', TestHandler);

// Trigger task in DB
// Worker polls event
// Handler executes
// Result sent to orchestration
// Task completes successfully
```

- [ ] Works on Bun runtime
- [ ] Works on Node.js runtime

### Scenario 2: API Handler
```typescript
// Handler makes HTTP request
// Classifies error correctly
// Returns appropriate result
```

- [ ] 4xx errors → permanent failure
- [ ] 5xx errors → retryable failure
- [ ] Success → success result

### Scenario 3: Decision Handler
```typescript
// Handler evaluates condition
// Routes to appropriate steps
// DecisionPointOutcome constructed correctly
```

- [ ] Routing works correctly
- [ ] Steps activated as expected

### Scenario 4: Batch Handler
```typescript
// Handler processes batch
// Cursor context extracted
// Items processed correctly
// Batch result with cursor returned
```

- [ ] Batch processing works
- [ ] Field names correct

### Scenario 5: Graceful Shutdown
```typescript
// Worker running
// Send SIGTERM
// In-flight handlers complete
// Worker stops cleanly
// No zombie processes
```

- [ ] Works on Bun runtime
- [ ] Works on Node.js runtime

## Performance Validation

- [ ] FFI call overhead <1ms (acceptable)
- [ ] Handler dispatch latency <10ms
- [ ] Memory usage stable (no leaks)
- [ ] Can process >100 steps/second (reasonable baseline)

## Platform Compatibility

- [ ] macOS (development)
- [ ] Linux (production)
- [ ] Windows (if supported)

## Regression Testing

After TAS-100 completion, verify existing workers still function:
- [ ] Rust worker tests pass
- [ ] Ruby worker tests pass
- [ ] Python worker tests pass
- [ ] No breaking changes to shared Rust codebase

## Final Checklist

- [ ] All sub-issues (TAS-101 through TAS-107) marked complete
- [ ] All validation criteria above passing
- [ ] CI/CD pipeline green for TypeScript worker
- [ ] No open bugs blocking release
- [ ] Documentation reviewed and approved
- [ ] Ready for production deployment

---

**Execute this validation plan after completing all TAS-100 sub-issues.**
