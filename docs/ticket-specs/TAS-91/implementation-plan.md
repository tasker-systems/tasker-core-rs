# TAS-91: Implementation Plan (Updated January 2026)

## Context Updates Since Original Spec

### Completed Prerequisites
- **TAS-137** (StepContext Usability): All languages now have harmonized APIs
  - `get_input()`, `get_dependency_result()`, `get_dependency_field()`
  - Retry helpers: `is_retry()`, `is_last_retry()`
  - Checkpoint access for batch resumability
- **Ruby Reference Implementation**: 32 handlers complete across 4 blog posts
- **Pattern Examples**: Each worker has 10-15 pattern examples as templates
- **E2E Infrastructure**: Shared test harness with Docker ports configured

### Current State Matrix

| Language | Blog Handlers | E2E Tests | Pattern Examples |
|----------|---------------|-----------|------------------|
| Ruby | 32 (complete) | 3 blog tests | N/A (blog is patterns) |
| Python | 0 | 0 blog tests | 10 patterns |
| TypeScript | 0 | 0 blog tests | 9 patterns |
| Rust | 0 | Uses Rust harness | 15 patterns |

---

## Phase 1: Post 01 E-commerce (Linear Workflow)

**Goal**: Establish patterns, validate E2E integration, build confidence

### 1.1 Python E-commerce Handlers

**Target Location**: `workers/python/tests/handlers/examples/blog_examples/post_01_ecommerce/`

**Handlers to Implement** (5):
1. `validate_cart_handler.py` - Input validation, mock product DB
2. `process_payment_handler.py` - Dependency access, error classification
3. `update_inventory_handler.py` - Stock deduction
4. `create_order_handler.py` - Aggregate results from prior steps
5. `send_confirmation_handler.py` - Notification simulation

**Supporting Files**:
- `models/product.py` - Product dataclass with mock data
- `models/order.py` - Order dataclass
- `__init__.py` - Handler exports and registration

**Validation Gate 1.1**:
- [ ] All 5 handlers pass unit tests
- [ ] Handlers registered in Python worker
- [ ] Manual test via curl against running worker

### 1.2 Python E-commerce E2E Test

**Target File**: `tests/e2e/python/ecommerce_order_test.rs`

**Test Fixture**: `tests/fixtures/task_templates/python/ecommerce_order_processing_py.yaml`

**Test Cases**:
1. Successful order processing (valid cart, payment succeeds)
2. Validation failure (invalid product ID)
3. Payment failure (declined card simulation)

**Validation Gate 1.2**:
- [ ] E2E test passes with Docker services running
- [ ] Test validates all 5 steps complete successfully
- [ ] Error scenarios handled correctly

### 1.3 TypeScript E-commerce Handlers

**Target Location**: `workers/typescript/tests/handlers/examples/blog_examples/post_01_ecommerce/`

**Handlers to Implement** (5):
1. `validate-cart-handler.ts`
2. `process-payment-handler.ts`
3. `update-inventory-handler.ts`
4. `create-order-handler.ts`
5. `send-confirmation-handler.ts`

**Validation Gate 1.3**:
- [ ] All 5 handlers pass unit tests
- [ ] Handlers registered in TypeScript worker
- [ ] Manual test via curl

### 1.4 TypeScript E-commerce E2E Test

**Target File**: `tests/e2e/typescript/ecommerce_order_test.rs`
**Test Fixture**: `tests/fixtures/task_templates/typescript/ecommerce_order_processing_ts.yaml`

**Validation Gate 1.4**:
- [ ] E2E test passes
- [ ] Matches Ruby test behavior

### 1.5 Rust E-commerce Handlers

**Target Location**: `workers/rust/src/step_handlers/blog_examples/post_01_ecommerce/`

**Handlers to Implement** (5):
- `validate_cart.rs`
- `process_payment.rs`
- `update_inventory.rs`
- `create_order.rs`
- `send_confirmation.rs`

**Validation Gate 1.5**:
- [ ] All handlers compile and pass unit tests
- [ ] Handlers registered in Rust worker registry
- [ ] E2E test passes (uses Rust test harness directly)

### Phase 1 Exit Criteria
- [ ] 15 handlers implemented (5 per language)
- [ ] 3 E2E test suites passing (Python, TypeScript, Rust)
- [ ] All handlers demonstrate TAS-137 patterns
- [ ] `cargo make test` passes in CI

---

## Phase 2: Post 02 Data Pipeline (DAG Workflow)

**Goal**: Demonstrate parallel execution and multi-dependency convergence

### 2.1 Python Data Pipeline Handlers

**Target Location**: `workers/python/tests/handlers/examples/blog_examples/post_02_data_pipeline/`

**Handlers to Implement** (8):
1. `extract_sales_data_handler.py` - Independent
2. `extract_inventory_data_handler.py` - Independent
3. `extract_customer_data_handler.py` - Independent
4. `transform_sales_handler.py` - Depends on extract_sales
5. `transform_inventory_handler.py` - Depends on extract_inventory
6. `transform_customers_handler.py` - Depends on extract_customers
7. `aggregate_metrics_handler.py` - **Convergence**: depends on all 3 transforms
8. `generate_insights_handler.py` - Depends on aggregate

**DAG Structure**:
```
extract_sales ─────→ transform_sales ────┐
extract_inventory ─→ transform_inventory ├→ aggregate_metrics → generate_insights
extract_customers ─→ transform_customers ┘
```

**Validation Gate 2.1**:
- [ ] All 8 handlers pass unit tests
- [ ] `aggregate_metrics_handler` correctly accesses 3 dependencies
- [ ] Uses `get_dependency_result()` for each transform

### 2.2 Python Data Pipeline E2E Test

**Target File**: `tests/e2e/python/data_pipeline_test.rs`
**Test Fixture**: `tests/fixtures/task_templates/python/data_pipeline_analytics_py.yaml`

**Test Validations**:
- Extract steps can run in parallel
- Transform steps wait for their extract dependency
- Aggregate step waits for all 3 transforms
- Final insights generated correctly

**Validation Gate 2.2**:
- [ ] E2E test passes
- [ ] Parallel execution verified (extract steps complete ~simultaneously)

### 2.3-2.5 TypeScript & Rust Data Pipeline

Same pattern as Phase 1 - 8 handlers each, E2E tests

**Phase 2 Exit Criteria**:
- [ ] 24 handlers implemented (8 per language)
- [ ] DAG execution verified in all languages
- [ ] Convergence pattern working (3 dependencies → 1 step)

---

## Phase 3: Post 03 Microservices Coordination (Tree Workflow)

**Goal**: Demonstrate API-style handlers with parallel branches

### Handlers to Implement (5 per language):
1. `create_user_account_handler` - Root step
2. `setup_billing_profile_handler` - Depends on create_user
3. `initialize_preferences_handler` - Depends on create_user (parallel with billing)
4. `send_welcome_sequence_handler` - Depends on both billing + preferences
5. `update_user_status_handler` - Depends on welcome

**Tree Structure**:
```
create_user_account
    ├── setup_billing_profile ────┐
    └── initialize_preferences ───┴── send_welcome_sequence → update_user_status
```

**Phase 3 Exit Criteria**:
- [ ] 15 handlers implemented (5 per language)
- [ ] Parallel branch execution verified
- [ ] E2E tests pass for all languages

---

## Phase 4: Post 04 Team Scaling (Namespace Isolation)

**Goal**: Demonstrate namespace-based handler organization

### Namespace Structure

**Customer Success Namespace** (5 handlers per language):
- `validate_refund_request_handler`
- `check_refund_policy_handler`
- `get_manager_approval_handler`
- `execute_refund_workflow_handler`
- `update_ticket_status_handler`

**Payments Namespace** (4 handlers per language):
- `validate_payment_eligibility_handler`
- `process_gateway_refund_handler`
- `update_payment_records_handler`
- `notify_customer_handler`

**Directory Structure**:
```
post_04_team_scaling/
├── customer_success/
│   └── step_handlers/
└── payments/
    └── step_handlers/
```

**Phase 4 Exit Criteria**:
- [ ] 27 handlers implemented (9 per language)
- [ ] Namespace isolation verified
- [ ] Cross-namespace calls working

---

## Phase 5: Documentation & Integration

### 5.1 Blog Examples README

Each language's `blog_examples/` directory gets a README with:
- Handler mapping table (Ruby → Python/TypeScript/Rust)
- Key pattern demonstrations
- Links to E2E tests

### 5.2 Cross-Reference Documentation

Update `docs/reference/step-context-api.md` with:
- Links to blog examples demonstrating each method
- Common patterns from blog examples

### 5.3 GitBook Integration (TAS-66)

Prepare codetabs-ready examples for:
- Each blog post showing all 4 languages
- Handler comparison tables

**Phase 5 Exit Criteria**:
- [ ] READMEs in all blog_examples directories
- [ ] Cross-language mapping complete
- [ ] Examples ready for GitBook codetabs

---

## Running Validation

### Service Management (cargo-make)

```bash
# Start services (no Docker rebuild needed)
cargo make services-start        # All services (orchestration + all workers)
cargo make services-status       # Check what's running
cargo make services-health-check # Verify health endpoints
cargo make services-stop         # Stop all services

# Individual services
cargo make run-orchestration     # Start orchestration (port 8080)
cargo make run-worker-python     # Start Python worker (port 8083)
cargo make run-worker-typescript # Start TypeScript worker (port 8085)
cargo make run-worker-rust       # Start Rust worker (port 8081)
cargo make run-worker-ruby       # Start Ruby worker (port 8082)

# View logs
cargo make logs-orchestration    # Tail orchestration logs
```

### Port Reference

| Service | Port |
|---------|------|
| Orchestration | 8080 |
| Rust Worker | 8081 |
| Ruby Worker | 8082 |
| Python Worker | 8083 |
| TypeScript Worker | 8085 |

### Per-Phase Validation Commands

```bash
# Python handlers (unit tests)
cd workers/python && cargo make test

# TypeScript handlers (unit tests)
cd workers/typescript && cargo make test

# Rust handlers (unit tests)
cd workers/rust && cargo make test

# E2E tests (requires services running)
cargo make services-start
cargo make test-e2e                                # All E2E tests

# Filtered E2E tests with nextest
cargo nextest run -E 'binary(e2e_tests)' -- python::ecommerce
cargo nextest run -E 'binary(e2e_tests)' -- typescript::ecommerce

# Full validation
cargo make test
```

### CI Integration

All phases must pass in CI before merge:
- `cargo make check` (lint + format)
- `cargo make test` (all tests including E2E)
- No regressions in existing pattern tests

---

## Effort Estimates (Updated)

| Phase | Handlers | E2E Tests | Est. Effort |
|-------|----------|-----------|-------------|
| Phase 1 | 15 (5×3) | 3 | Foundation - careful iteration |
| Phase 2 | 24 (8×3) | 3 | Parallel patterns established |
| Phase 3 | 15 (5×3) | 3 | Tree pattern, faster now |
| Phase 4 | 27 (9×3) | 3 | Namespace patterns |
| Phase 5 | N/A | N/A | Documentation |

**Total**: 81 handlers, 12 E2E test files

---

## Key Implementation Notes

### Using TAS-137 APIs

All handlers should use the new cross-language standard APIs:

```python
# Python
def call(self, context: StepContext) -> StepHandlerResult:
    order_id = context.get_input("order_id")
    cart = context.get_dependency_result("validate_cart")
    item_count = context.get_dependency_field("validate_cart", "summary", "item_count")
```

```typescript
// TypeScript
async call(context: StepContext): Promise<StepHandlerResult> {
    const orderId = context.getInput<string>('order_id');
    const cart = context.getDependencyResult('validate_cart');
    const itemCount = context.getDependencyField('validate_cart', 'summary', 'itemCount');
}
```

```rust
// Rust
async fn call(&self, step_data: &TaskSequenceStep) -> Result<StepExecutionResult> {
    let order_id: String = step_data.get_input("order_id")?;
    let cart: CartResult = step_data.get_dependency_result_column_value("validate_cart")?;
    let item_count: i32 = step_data.get_dependency_field("validate_cart", &["summary", "item_count"])?;
}
```

### Error Classification

All handlers should use language-appropriate error types:
- **Python**: `PermanentError`, `RetryableError` from `tasker_core.errors`
- **TypeScript**: `PermanentError`, `RetryableError` from `@tasker/worker`
- **Rust**: `StepError::Permanent`, `StepError::Retryable`

### Mock Data Consistency

Use the Ruby implementation as reference but make data language-idiomatic:
- Python: dataclasses with type hints
- TypeScript: interfaces with readonly properties
- Rust: structs with serde derives

---

## Dependencies

- TAS-137 (Complete) - StepContext APIs
- TAS-66 (Parallel) - GitBook integration for Phase 5
- Docker infrastructure must be running for E2E tests

---

## Success Metrics

1. All 81 handlers implemented and registered
2. All 12 E2E test files passing
3. `cargo make test` green in CI
4. Documentation complete with cross-language mapping
5. Ready for GitBook codetabs integration
