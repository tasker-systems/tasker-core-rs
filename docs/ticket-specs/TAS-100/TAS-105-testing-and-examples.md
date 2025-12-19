# TAS-105: TypeScript Testing and Examples

**Parent**: [TAS-100](./README.md)
**Linear**: [TAS-105](https://linear.app/tasker-systems/issue/TAS-105)
**Branch**: `jcoletaylor/tas-105-typescript-testing-examples`
**Priority**: Medium
**Status**: Todo
**Depends On**: TAS-101, TAS-102, TAS-103, TAS-104

## Objective

Comprehensive test suite covering unit tests, integration tests, and example handlers demonstrating all patterns. Tests must run on both Bun and Node.js runtimes.

## Test Strategy

### Unit Tests (vitest)
- FFI adapters (Bun and Node separately)
- Runtime detection
- StepContext accessors
- StepHandler base class
- HandlerRegistry
- EventEmitter lifecycle
- EventPoller polling behavior
- Specialized handlers (Api, Decision, Batchable)

### Integration Tests
- Full bootstrap → register handler → process event → shutdown
- E2E flow with test database
- Cross-runtime compatibility (Bun and Node)

### Example Handlers
Port Ruby/Python examples to TypeScript:
- Order fulfillment workflow
- Batch processing
- API integration
- Decision routing
- Error scenarios

## Reference Test Suites

**Ruby**: `workers/ruby/spec/`
**Python**: `workers/python/tests/`

## Files to Create

```
tests/
├── unit/
│   ├── ffi/
│   │   ├── runtime.test.ts
│   │   ├── bun.test.ts
│   │   └── node.test.ts
│   ├── handler/
│   │   ├── base.test.ts
│   │   ├── registry.test.ts
│   │   ├── api.test.ts
│   │   ├── decision.test.ts
│   │   └── batchable.test.ts
│   ├── types/
│   │   ├── context.test.ts
│   │   └── result.test.ts
│   └── events/
│       ├── event-emitter.test.ts
│       └── event-poller.test.ts
├── integration/
│   ├── bootstrap.test.ts
│   ├── full-lifecycle.test.ts
│   └── fixtures/
└── examples/
    └── handlers/
        ├── process-order.ts
        ├── batch-processor.ts
        ├── api-handler.ts
        └── decision-handler.ts
```

## Research Needed

1. Vitest configuration for dual-runtime testing
2. Mock strategy for FFI calls in unit tests
3. Test database setup (matches Ruby/Python patterns)
4. Fixture management for integration tests

## Success Criteria

- [ ] Unit test coverage >80%
- [ ] All tests pass on Bun runtime
- [ ] All tests pass on Node.js runtime
- [ ] Integration tests cover full worker lifecycle
- [ ] Example handlers demonstrate all patterns
- [ ] CI/CD pipeline runs tests on both runtimes

## Estimated Scope

~2-3 days (15-20 hours)

---

**To be expanded**: Detailed test plans and example code after core implementation.
