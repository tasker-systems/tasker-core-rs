# TAS-103: TypeScript Specialized Handlers

**Parent**: [TAS-100](./README.md)
**Linear**: [TAS-103](https://linear.app/tasker-systems/issue/TAS-103)
**Branch**: `jcoletaylor/tas-103-typescript-specialized-handlers`
**Priority**: Medium
**Status**: Todo
**Depends On**: TAS-102

## Objective

Implement specialized handler types following TAS-92 alignment: `ApiHandler`, `DecisionHandler`, and `Batchable` mixin/base class.

## Specialized Handler Types

### 1. ApiHandler
- Base class for HTTP/REST API integration
- Methods: `get()`, `post()`, `put()`, `delete()`
- Automatic HTTP status code classification (4xx → permanent, 5xx → retryable)
- Uses `fetch()` or similar (runtime-agnostic)

### 2. DecisionHandler
- Base class for dynamic workflow routing
- Helper: `decisionSuccess(steps: string[], routingContext?: object)`
- Constructs `DecisionPointOutcome` structure

### 3. Batchable
- Mixin or base class for batch processing
- Methods: `batchWorkerSuccess()`, `getBatchContext()`
- Standardized field names: `items_processed`, `items_succeeded`, `items_failed`
- Cursor context: `start_cursor`, `end_cursor`, `batch_size`

## Reference Implementations

**Ruby**: 
- `workers/ruby/lib/tasker_core/step_handler/api.rb`
- `workers/ruby/lib/tasker_core/step_handler/decision.rb`
- `workers/ruby/lib/tasker_core/step_handler/batchable.rb`

**Python**:
- `workers/python/python/tasker_core/step_handler/api.py`
- `workers/python/python/tasker_core/step_handler/decision.py`
- `workers/python/python/tasker_core/batch_processing/batchable.py`

## Files to Create

```
src/handler/
├── api.ts          # ApiHandler base class
├── decision.ts     # DecisionHandler base class
└── batchable.ts    # Batchable mixin/base
```

## Research Needed

1. TypeScript mixin pattern vs inheritance for Batchable (Python uses mixin, Ruby uses inheritance)
2. `fetch()` API availability in Bun vs Node.js
3. Error classification strategy for HTTP status codes
4. `DecisionPointOutcome` structure from Ruby/Python

## Success Criteria

- [ ] ApiHandler with HTTP convenience methods
- [ ] Automatic error classification by status code
- [ ] DecisionHandler with simplified routing helper
- [ ] Batchable with standard field names
- [ ] Example handlers demonstrating each type
- [ ] Unit tests for specialized handlers

## Estimated Scope

~1-2 days (10-15 hours)

---

**To be expanded**: Detailed implementation after TAS-102 completion.
