# TAS-92: Validation and Cohesion Verification Plan

**Parent**: [TAS-92](./README.md)
**Purpose**: Final verification that all API alignment goals are achieved across Ruby, Python, and Rust.

## Prerequisites

Before running this validation:

- [ ] TAS-95 (Python) PR merged to main
- [ ] TAS-96 (Ruby) PR merged to main
- [ ] TAS-97 (Rust) PR merged to main
- [ ] TAS-98 (Documentation) PR merged to main

## Validation Phases

### Phase 1: Cross-Language API Parity

#### 1.1 Handler Signatures

| Check | Ruby | Python | Rust |
|-------|------|--------|------|
| Uses unified context | `call(context)` | `call(context)` | `call(&TaskSequenceStep)` |
| Context has task_uuid | [ ] | [ ] | [ ] |
| Context has step_uuid | [ ] | [ ] | [ ] |
| Context has input_data | [ ] | [ ] | [ ] |
| Context has dependency_results | [ ] | [ ] | [ ] |

**Verification Commands:**
```bash
# Ruby - verify StepContext exists
grep -r "class StepContext" workers/ruby/lib/

# Python - verify StepContext exists
grep -r "class StepContext" python/tasker_core/

# Rust - verify TaskSequenceStep fields
grep -A 20 "pub struct TaskSequenceStep" tasker-shared/src/
```

#### 1.2 Result Factory Methods

| Check | Ruby | Python | Rust |
|-------|------|--------|------|
| `success()` method | [ ] | [ ] | [ ] |
| `failure()` method | [ ] | [ ] | [ ] |
| Returns success bool | [ ] | [ ] | [ ] |
| Returns result data | [ ] | [ ] | [ ] |
| Returns metadata | [ ] | [ ] | [ ] |
| Returns error_message | [ ] | [ ] | [ ] |
| Returns error_type | [ ] | [ ] | [ ] |
| Returns error_code | [ ] | [ ] | [ ] |
| Returns retryable | [ ] | [ ] | [ ] |

**Verification Commands:**
```bash
# Python - verify renamed methods
grep -n "def success\|def failure" python/tasker_core/types.py
grep -n "success_handler_result\|failure_handler_result" python/  # Should return nothing

# Ruby - verify methods exist
grep -n "def success\|def failure" workers/ruby/lib/tasker_core/step_handler/

# Rust - verify helper method
grep -n "with_error_code" tasker-shared/src/messaging/step_execution_result.rs
```

#### 1.3 Registry API Methods

| Method | Ruby | Python | Rust |
|--------|------|--------|------|
| `register(name, handler)` | [ ] | [ ] | [ ] |
| `is_registered(name)` | [ ] | [ ] | [ ] |
| `resolve(name)` | [ ] | [ ] | [ ] |
| `list_handlers()` | [ ] | [ ] | [ ] |

**Verification Commands:**
```bash
# Ruby - verify method renames
grep -n "def register\|def is_registered\|def resolve\|def list_handlers" \
  workers/ruby/lib/tasker_core/registry/

# Python - verify methods exist
grep -n "def register\|def is_registered\|def resolve\|def list_handlers" \
  python/tasker_core/

# Rust - verify methods exist
grep -n "fn is_registered\|fn list_handlers" workers/rust/src/step_handlers/registry.rs
```

### Phase 2: Specialized Handlers

#### 2.1 API Handler

| Feature | Ruby | Python | Rust |
|---------|------|--------|------|
| `get()` method | [ ] | [ ] | Documented |
| `post()` method | [ ] | [ ] | Documented |
| `put()` method | [ ] | [ ] | Documented |
| `delete()` method | [ ] | [ ] | Documented |

**Verification Commands:**
```bash
# Ruby - verify HTTP convenience methods
grep -n "def get\|def post\|def put\|def delete" \
  workers/ruby/lib/tasker_core/step_handler/api.rb

# Python - verify HTTP methods exist
grep -n "def get\|def post\|def put\|def delete" \
  python/tasker_core/step_handler/
```

#### 2.2 Decision Handler

| Feature | Ruby | Python |
|---------|------|--------|
| Simple helper accepts steps list | [ ] | [ ] |
| Simple helper accepts routing_context | [ ] | [ ] |
| Full method accepts DecisionPointOutcome | [ ] | [ ] |

**Verification Commands:**
```bash
# Python - verify decision_success helper
grep -A 10 "def decision_success" python/tasker_core/step_handler/decision.py

# Ruby - verify decision_success method
grep -A 10 "def decision_success" workers/ruby/lib/tasker_core/step_handler/decision.rb
```

#### 2.3 Batchable Handler

| Feature | Ruby | Python |
|---------|------|--------|
| `batch_worker_success()` method | [ ] | [ ] |
| `get_batch_context()` method | [ ] | [ ] |
| Uses `items_processed` field | [ ] | [ ] |
| Uses `items_succeeded` field | [ ] | [ ] |
| Uses `items_failed` field | [ ] | [ ] |

**Verification Commands:**
```bash
# Ruby - verify method renames
grep -n "batch_worker_success\|get_batch_context" \
  workers/ruby/lib/tasker_core/step_handler/batchable.rb

# Python - verify methods exist
grep -n "batch_worker_success\|get_batch_context" \
  python/tasker_core/step_handler/

# Verify no old method names
grep -rn "batch_worker_complete\|extract_cursor_context" workers/ruby/ python/
```

### Phase 3: Domain Events

#### 3.1 Publisher Contract

| Feature | Ruby | Python | Rust |
|---------|------|--------|------|
| `BasePublisher` exists | [ ] | [ ] | Trait exists |
| `name()` method | [ ] | [ ] | [ ] |
| `publish(ctx)` method | [ ] | [ ] | [ ] |
| `StepEventContext` defined | [ ] | [ ] | [ ] |

**Verification Commands:**
```bash
# Python - verify BasePublisher
grep -A 20 "class BasePublisher" python/tasker_core/domain_events/

# Ruby - verify publish(ctx) method
grep -n "def publish" workers/ruby/lib/tasker_core/domain_events/base_publisher.rb

# Rust - verify StepEventPublisher trait
grep -A 10 "trait StepEventPublisher" tasker-worker/src/
```

#### 3.2 Subscriber Contract

| Feature | Ruby | Python |
|---------|------|--------|
| `BaseSubscriber` exists | [ ] | [ ] |
| `subscribes_to` method | [ ] | [ ] |
| `handle(event)` method | [ ] | [ ] |

**Verification Commands:**
```bash
# Python - verify BaseSubscriber
grep -A 20 "class BaseSubscriber" python/tasker_core/domain_events/

# Ruby - verify BaseSubscriber
grep -A 10 "class BaseSubscriber" workers/ruby/lib/tasker_core/domain_events/
```

### Phase 4: Error Types

#### 4.1 Standard Values Available

| Value | Ruby | Python | Rust |
|-------|------|--------|------|
| `permanent_error` | [ ] | [ ] | [ ] |
| `retryable_error` | [ ] | [ ] | [ ] |
| `validation_error` | [ ] | [ ] | [ ] |
| `timeout` | [ ] | [ ] | [ ] |
| `handler_error` | [ ] | [ ] | [ ] |

**Verification Commands:**
```bash
# Ruby - verify ErrorTypes module
grep -A 10 "module ErrorTypes" workers/ruby/lib/tasker_core/types/

# Python - verify error type Literals
grep -A 5 "ErrorType = Literal" python/tasker_core/types.py

# Rust - verify error_types constants
grep -A 10 "pub mod error_types" tasker-shared/src/
```

### Phase 5: Test Suites

#### 5.1 Test Execution

```bash
# Ruby tests
cd workers/ruby
DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test \
TASKER_ENV=test bundle exec rspec

# Python tests
cd python
uv run pytest
uv run mypy .
uv run ruff check .

# Rust tests
cargo test --all-features
cargo clippy --all-targets --all-features
```

#### 5.2 Test Coverage Check

| Language | Test File | Passes |
|----------|-----------|--------|
| Ruby | `spec/tasker_core/step_handler/base_spec.rb` | [ ] |
| Ruby | `spec/tasker_core/registry/handler_registry_spec.rb` | [ ] |
| Ruby | Integration tests | [ ] |
| Python | `tests/test_step_handler.py` | [ ] |
| Python | `tests/test_module_exports.py` | [ ] |
| Python | `tests/test_domain_events.py` | [ ] |
| Rust | `tasker-shared` tests | [ ] |
| Rust | `workers/rust` tests | [ ] |

### Phase 6: Documentation Verification

#### 6.1 Files Exist

- [ ] `docs/worker-crates/api-convergence-matrix.md`
- [ ] `docs/worker-crates/example-handlers.md`
- [ ] `workers/rust/src/step_handlers/README.md`

#### 6.2 No Old API References

```bash
# Search for old method names that should not exist
grep -rn "success_handler_result\|failure_handler_result" docs/ python/
grep -rn "register_handler\|handler_available?\|registered_handlers" docs/ workers/ruby/
grep -rn "batch_worker_complete\|extract_cursor_context" docs/
grep -rn "call(task, sequence, step)" docs/ workers/ruby/
```

#### 6.3 Documentation Builds

```bash
# Verify docs build cleanly
cargo doc --all-features --no-deps
```

### Phase 7: Integration Verification

#### 7.1 End-to-End Handler Execution

Test that a handler using the new APIs works end-to-end:

```bash
# Ruby integration test
cd workers/ruby
DATABASE_URL=postgresql://tasker:tasker@localhost/tasker_rust_test \
TASKER_ENV=test bundle exec rspec spec/integration/ --format documentation

# Python integration test (if available)
cd python
uv run pytest tests/integration/ -v
```

#### 7.2 FFI Bridge Verification

Verify the Rust-to-Ruby and Rust-to-Python bridges work with new APIs:

```bash
# Ruby FFI test
cd workers/ruby
bundle exec rake compile
bundle exec rspec spec/ffi/ --format documentation
```

## Final Checklist

### Acceptance Criteria from TAS-92

- [ ] All three languages use `call(context)` signature
- [ ] Result factories named `success(...)` / `failure(...)` with consistent fields
- [ ] Registry APIs use `register`, `is_registered`, `resolve`, `list_handlers`
- [ ] Specialized handlers (API/Decision/Batchable) have consistent method names
- [ ] Domain event publisher/subscriber contracts documented and implemented
- [ ] Convergence matrix in docs showing final API surface per language
- [ ] All example handlers updated to new APIs
- [ ] Tests passing in all three worker implementations

### Sign-off

| Item | Verified By | Date |
|------|-------------|------|
| Python API alignment complete | | |
| Ruby API alignment complete | | |
| Rust API alignment complete | | |
| Documentation updated | | |
| All tests passing | | |
| Integration verified | | |

## Rollback Plan

If issues are discovered after merge:

1. **Immediate**: Revert the specific child ticket PR
2. **Investigation**: Document the issue in the Linear ticket
3. **Fix forward**: Create a fix PR rather than full revert if possible

## Post-Validation Tasks

After all validations pass:

1. Update TAS-92 status to "Done" in Linear
2. Close all child tickets (TAS-95, TAS-96, TAS-97, TAS-98)
3. Create summary comment on TAS-92 with final state
4. Consider announcement if this is a significant milestone
