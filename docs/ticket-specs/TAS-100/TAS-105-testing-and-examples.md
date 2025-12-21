# TAS-105: TypeScript Testing and Examples

**Parent**: [TAS-100](./README.md)  
**Linear**: [TAS-105](https://linear.app/tasker-systems/issue/TAS-105)  
**Branch**: `jcoletaylor/tas-105-typescript-testing-examples`  
**Priority**: Medium  
**Status**: Detailed Specification  
**Depends On**: TAS-101, TAS-102, TAS-103, TAS-104

---

## Objective

Comprehensive test suite covering unit tests, integration tests, and example handlers demonstrating all patterns. Tests must run on both Bun and Node.js runtimes to ensure cross-runtime compatibility.

**Key Goal**: Achieve >80% code coverage while providing production-ready examples that developers can use as templates.

---

## Test Strategy Overview

### Three-Layer Testing Approach

1. **Unit Tests**: Test individual components in isolation with mocked FFI
2. **Integration Tests**: Test complete worker lifecycle with real FFI and test database
3. **Example Handlers**: Demonstrate real-world patterns (also serve as integration tests)

### Test Framework: Vitest

**Why Vitest**:
- Native ESM support (works with our `"type": "module"` setup)
- Fast execution (Vite-powered)
- Compatible with both Bun and Node.js
- Jest-compatible API (familiar to most developers)
- Built-in coverage reporting

**Configuration**: See Phase 1 for `vitest.config.ts`

---

## Reference Implementations

**Python Tests**: `workers/python/tests/`
- `test_handler_registry.py` - Registry singleton, registration, resolution
- `test_api_handler.py` - HTTP error classification, ApiResponse wrapper
- `test_bootstrap.py` - Worker lifecycle without actual bootstrap
- `test_decision_handler.py` - Decision routing and branch management
- `test_batchable.py` - Batch processing mixin

**Ruby Tests**: `workers/ruby/spec/`
- `worker/bootstrap_unit_spec.rb` - Bootstrap unit tests
- `step_handler/decision_spec.rb` - Decision handler patterns
- `handlers/examples/` - Example workflow handlers

**E2E Tests**: `tests/e2e/{python,ruby,rust}/`
- Full lifecycle tests with real database
- Template-driven workflows
- Error handling and retry scenarios

**Fixtures**: `tests/fixtures/`
- `task_templates/` - YAML workflow templates
- `products.csv` - Batch processing test data

---
