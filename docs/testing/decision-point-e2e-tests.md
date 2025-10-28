# Decision Point E2E Tests - TAS-53

This document describes the E2E tests for TAS-53 Decision Point functionality and how to run them.

## Test Location

```
tests/e2e/ruby/conditional_approval_test.rs
```

## Design Note: Deferred Step Type (Added 2025-10-27)

A critical design refinement was introduced to handle convergence patterns in decision point workflows:

### The Convergence Problem

In conditional_approval, all three possible outcomes (auto_approve, manager_approval, finance_review) converge to the same finalize_approval step. However, we cannot create finalize_approval at task initialization because:
1. We don't know which approval steps will be created
2. finalize_approval needs different dependencies depending on the decision point's choice

### Solution: `type: deferred`

A new step type was added to handle this pattern:

```yaml
- name: finalize_approval
  type: deferred  # NEW STEP TYPE!
  dependencies: [auto_approve, manager_approval, finance_review]  # All possible deps
```

**How it works:**
1. Deferred steps list ALL possible dependencies in the template
2. At initialization, deferred steps are excluded (they're descendants of decision points)
3. When a decision point creates outcome steps, the system:
   - Detects downstream deferred steps
   - Computes: `declared_deps ∩ actually_created_steps` = actual DAG
   - Creates deferred steps with resolved dependencies

**Example:**
- When routing_decision chooses `auto_approve`:
  - Creates: `auto_approve`
  - Detects: `finalize_approval` is deferred with deps `[auto_approve, manager_approval, finance_review]`
  - Intersection: `[auto_approve]` ∩ `[auto_approve]` = `[auto_approve]`
  - Creates: `finalize_approval` depending on `auto_approve` only

This elegantly solves convergence without requiring handlers to explicitly list convergence steps or special orchestration logic.

## Test Coverage

The test suite validates the conditional approval workflow, which demonstrates decision point functionality with dynamic step creation based on runtime conditions (approval amount thresholds).

### Test Cases

1. **test_small_amount_auto_approval()** - Tests amounts < $1,000
   - Expected path: validate_request → routing_decision → **auto_approve** → finalize_approval
   - Verifies only 4 steps created
   - Confirms manager_approval and finance_review are NOT created

2. **test_medium_amount_manager_approval()** - Tests amounts $1,000-$4,999
   - Expected path: validate_request → routing_decision → **manager_approval** → finalize_approval
   - Verifies only 4 steps created
   - Confirms auto_approve and finance_review are NOT created

3. **test_large_amount_dual_approval()** - Tests amounts >= $5,000
   - Expected path: validate_request → routing_decision → **manager_approval** + **finance_review** → finalize_approval
   - Verifies 5 steps created
   - Confirms both parallel approval steps complete
   - Verifies auto_approve is NOT created

4. **test_decision_point_step_dependency_structure()** - Validates dependency resolution
   - Verifies dynamically created steps depend on routing_decision
   - Confirms finalize_approval waits for all approval steps
   - Tests proper execution order

5. **test_boundary_conditions()** - Tests exactly at $1,000 threshold
   - Verifies manager approval is used (not auto)

6. **test_boundary_large_threshold()** - Tests exactly at $5,000 threshold
   - Verifies dual approval path is triggered

7. **test_very_small_amount()** - Tests $0.01 amount
   - Verifies auto-approval for very small amounts

## Running the Tests

### Prerequisites

The tests require the full integration environment to be running. Use the Docker Compose test strategy:

```bash
# From the tasker-core directory

# 1. Stop any existing containers and clean up
docker-compose -f docker/docker-compose.test.yml down -v

# 2. Rebuild containers with latest changes
docker-compose -f docker/docker-compose.test.yml up --build -d

# 3. Wait for services to be healthy (about 10-15 seconds)
sleep 15

# 4. Run the conditional approval E2E tests
export DATABASE_URL="postgresql://tasker:tasker@localhost:5432/tasker_rust_test"
cargo test --test e2e_tests e2e::ruby::conditional_approval_test -- --nocapture

# 5. Clean up after testing (optional)
docker-compose -f docker/docker-compose.test.yml down
```

### Running Specific Tests

```bash
# Run just the small amount test
cargo test --test e2e_tests e2e::ruby::conditional_approval_test::test_small_amount_auto_approval -- --nocapture

# Run just the large amount test
cargo test --test e2e_tests e2e::ruby::conditional_approval_test::test_large_amount_dual_approval -- --nocapture

# Run all boundary tests
cargo test --test e2e_tests e2e::ruby::conditional_approval_test::test_boundary -- --nocapture
```

### Environment Variables

The tests use the following environment variables (set automatically in docker-compose.test.yml):

- `DATABASE_URL`: PostgreSQL connection string
- `TASKER_ENV`: Set to "test" for test configuration
- `TASK_TEMPLATE_PATH`: Points to test fixtures directory
- `RUST_LOG`: Set to "info" or "debug" for detailed logging

## Test Workflow Details

### Conditional Approval Workflow

The workflow implements amount-based routing:

```
┌─────────────────┐
│ validate_request│
│   (initial)     │
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ routing_decision│ ◄─── DECISION POINT (type: decision)
│  (decision)     │
└────────┬────────┘
         │
         ├─────────── < $1,000 ─────────┐
         │                              │
         │                              ▼
         │                     ┌────────────────┐
         │                     │  auto_approve  │
         │                     └────────┬───────┘
         │                              │
         ├─────── $1,000-$4,999 ────────┼────┐
         │                              │    │
         │                              │    ▼
         │                              │  ┌──────────────────┐
         │                              │  │ manager_approval │
         │                              │  └────────┬─────────┘
         │                              │           │
         └──────── >= $5,000 ───────────┼───────────┼────┐
                                        │           │    │
                                        │           │    ▼
                                        │           │  ┌───────────────┐
                                        │           │  │ finance_review│
                                        │           │  └───────┬───────┘
                                        │           │          │
                                        ▼           ▼          ▼
                                     ┌─────────────────────────┐
                                     │   finalize_approval     │
                                     │    (convergence)        │
                                     └─────────────────────────┘
```

### Decision Point Mechanism

1. **routing_decision** step executes with `type: decision` marker
2. Handler returns `DecisionPointOutcome::CreateSteps` with step names
3. Orchestration creates those steps dynamically and adds dependencies
4. Dynamically created steps execute like normal steps
5. Convergence step (finalize_approval) waits for all paths

## Task Template Location

The test uses the task template at:
```
tests/fixtures/task_templates/ruby/conditional_approval_handler.yaml
```

## Ruby Handler Implementation

The Ruby handlers are located at:
```
workers/ruby/spec/handlers/examples/conditional_approval/
├── handlers/
│   └── conditional_approval_handler.rb
└── step_handlers/
    ├── validate_request_handler.rb
    ├── routing_decision_handler.rb      ◄─── DECISION POINT HANDLER
    ├── auto_approve_handler.rb
    ├── manager_approval_handler.rb
    ├── finance_review_handler.rb
    └── finalize_approval_handler.rb
```

### Key Implementation Detail

The `routing_decision_handler.rb` returns a decision point outcome:

```ruby
outcome = if steps_to_create.empty?
            TaskerCore::Types::DecisionPointOutcome.no_branches
          else
            TaskerCore::Types::DecisionPointOutcome.create_steps(steps_to_create)
          end

TaskerCore::Types::StepHandlerCallResult.success(
  result: {
    # IMPORTANT: The decision point outcome MUST be in this key
    decision_point_outcome: outcome.to_h,
    route_type: route[:type],
    # ... other result fields
  }
)
```

## Troubleshooting

### Tests Fail with "Template Not Found"

Ensure the Ruby worker container has the correct template path:
```bash
docker-compose -f docker/docker-compose.test.yml logs ruby-worker
# Should show: TASK_TEMPLATE_PATH=/app/tests/fixtures/task_templates/ruby
```

### Tests Timeout

Increase wait time in docker-compose startup:
```bash
sleep 30  # Instead of sleep 15
```

### Database Connection Errors

Verify PostgreSQL is running and healthy:
```bash
docker-compose -f docker/docker-compose.test.yml ps
docker-compose -f docker/docker-compose.test.yml logs postgres
```

### Step Creation Doesn't Happen

Check orchestration logs for decision point processing:
```bash
docker-compose -f docker/docker-compose.test.yml logs orchestration | grep -i decision
```

## Success Criteria

All tests should pass with output similar to:
```
test e2e::ruby::conditional_approval_test::test_small_amount_auto_approval ... ok
test e2e::ruby::conditional_approval_test::test_medium_amount_manager_approval ... ok
test e2e::ruby::conditional_approval_test::test_large_amount_dual_approval ... ok
test e2e::ruby::conditional_approval_test::test_decision_point_step_dependency_structure ... ok
test e2e::ruby::conditional_approval_test::test_boundary_conditions ... ok
test e2e::ruby::conditional_approval_test::test_boundary_large_threshold ... ok
test e2e::ruby::conditional_approval_test::test_very_small_amount ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

## Next Steps

After validating Ruby workers:
- **Phase 8a**: Implement Rust worker support for decision points
- **Phase 9a**: Create E2E tests for Rust worker decision points
