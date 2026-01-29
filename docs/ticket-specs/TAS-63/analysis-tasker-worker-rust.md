# Coverage Analysis: tasker-worker-rust

**Current Coverage**: 9.21% line (3,499/38,002), 8.84% function (507/5,738)
**Target**: 55%
**Gap**: 45.79 percentage points

---

## Summary

The `tasker-worker-rust` crate at `workers/rust/src/` is the native Rust worker implementation that registers step handler functions and processes steps. It contains 33 source files organized into:

- **Core infrastructure**: `bootstrap.rs`, `event_handler.rs`, `global_event_system.rs`, `main.rs`, `lib.rs`
- **Step handler framework**: `step_handlers/mod.rs`, `step_handlers/registry.rs`, `step_handlers/capabilities.rs`
- **Workflow handlers**: 16+ handler modules implementing specific workflow patterns (linear, diamond, tree, DAG, order fulfillment, etc.)
- **Event subscribers**: `logging_subscriber.rs`, `metrics_subscriber.rs`
- **Event publishers**: `payment_event_publisher.rs`, `notification_event_publisher.rs`

The coverage gap is driven by two factors:
1. **Core infrastructure files at 0% coverage** (bootstrap, event_handler, main, global_event_system) -- these require running services to test
2. **Handler `call()` methods at low coverage** -- many handler execution bodies are only exercised through E2E tests, not unit tests

The registry (`71.19%`), event subscribers (`91-98%`), domain event publishers (`85-98%`), and capability examples (`91.46%`) already have strong coverage, demonstrating that the crate's testable framework components are well covered.

---

## Uncovered Files (0% Coverage)

| File | Lines | Functions | Role |
|------|-------|-----------|------|
| `bootstrap.rs` | 0/110 | 0/7 | Worker bootstrap orchestration -- creates registry, sets up event system, spawns HandlerDispatchService |
| `event_handler.rs` | 0/184 | 0/12 | RustEventHandler -- subscribes to WorkerEventSystem, dispatches to step handlers, publishes domain events |
| `global_event_system.rs` | 0/4 | 0/2 | LazyLock singleton for WorkerEventSystem |
| `main.rs` | 0/33 | 0/5 | Binary entry point -- bootstrap, signal handling, graceful shutdown |

**Total uncovered**: 331 lines, 26 functions

### Analysis

- **`bootstrap.rs` (110 lines)**: This is the integration glue code that creates the `RustStepHandlerRegistry`, sets up the `StepEventPublisherRegistry` with domain event publishers and event routers, and spawns the `HandlerDispatchService`. It requires a running PostgreSQL + PGMQ stack to bootstrap a real `WorkerSystemHandle`. Testing this requires integration/E2E test infrastructure.

- **`event_handler.rs` (184 lines)**: The `RustEventHandler` bridges the worker event system with native Rust handlers. It subscribes to `StepExecutionEvent` broadcasts, looks up handlers in the registry, executes them, creates `StepExecutionCompletionEvent`s, and invokes post-execution domain event publishers (TAS-65). The `publish_step_events` method has complex branching for publisher selection, event condition checking, and error logging.

- **`global_event_system.rs` (4 lines)**: Trivial singleton using `LazyLock`. Low line count but still contributes to the metric.

- **`main.rs` (33 lines)**: Binary entry point with `tokio::main`, signal handling (`SIGTERM`/`Ctrl+C`), and graceful shutdown. Typically excluded from unit test coverage or tested via integration tests.

---

## Lowest Coverage Files

| File | Line % | Lines Covered | Lines Total | Functions |
|------|--------|---------------|-------------|-----------|
| `notification_event_publisher.rs` | 10.61% | 7/66 | 1/9 | Custom StepEventPublisher for notification events |
| `order_fulfillment.rs` | 15.19% | 24/158 | 8/29 | 4-step order processing workflow (validate, reserve, pay, ship) |
| `microservices.rs` | 20.55% | 30/146 | 10/31 | 5-step user registration workflow |
| `data_pipeline.rs` | 23.88% | 48/201 | 16/42 | 8-step ETL pipeline (extract, transform, aggregate, insights) |
| `diamond_decision_batch.rs` | 41.67% | 60/144 | 20/84 | Complex diamond+decision+batch combined workflow |
| `batch_processing_products_csv.rs` | 41.86% | 18/43 | 6/25 | CSV-specific batch processing handlers |
| `ecommerce.rs` | 42.86% | 30/70 | 10/16 | E-commerce order processing (TAS-91 Blog Post 01) |
| `batch_processing_example.rs` | 45.00% | 18/40 | 6/22 | Generic batch processing pattern handlers |
| `team_scaling.rs` | 49.54% | 54/109 | 18/39 | Customer success + payments namespace handlers |

---

## Gap Analysis by Priority

### Critical Priority

**Files**: `bootstrap.rs`, `event_handler.rs`
**Combined uncovered lines**: 294
**Risk**: These are the core integration points for the Rust worker. Any bug in bootstrap or event handling affects all workflow processing.

**`bootstrap.rs` -- Worker Bootstrap Logic**
- Creates and wires together `RustStepHandlerRegistry`, `RustStepHandlerRegistryAdapter`, `StepEventPublisherRegistry`, `DomainEventCallback`, and `HandlerDispatchService`
- Three config functions (`default_config`, `no_web_api_config`, `no_event_driven_config`) are never called in tests
- The `bootstrap()` async function performs multi-step initialization with error handling
- **Test approach**: Integration tests that bootstrap a worker with a test database, or targeted unit tests that mock the `WorkerBootstrap` and verify registry wiring. The config functions can be trivially unit tested.

**`event_handler.rs` -- Step Execution Event Handling**
- `RustEventHandler::new()` constructs the handler with all required dependencies
- `start()` spawns a tokio task that loops on a broadcast receiver
- `handle_step_execution()` performs handler lookup, execution, result construction, and completion publishing
- `publish_step_events()` checks event declarations, selects publishers, and logs results
- **Test approach**: Unit tests using mock `WorkerEventSystem`, `WorkerEventSubscriber`, and `StepEventPublisherRegistry`. Test handler found/not-found paths, success/error result construction, event publishing with declared/undeclared events. The broadcast channel pattern can be tested without a database.

### High Priority

**Files**: `notification_event_publisher.rs`, `order_fulfillment.rs`, `microservices.rs`, `data_pipeline.rs`
**Combined uncovered lines**: 497 (of 571 total)
**Risk**: Business logic handlers with validation, error handling, and dependency resolution that should be verified.

**`notification_event_publisher.rs` (10.61% covered)**
- Only the `test_should_handle` basic string test runs
- The `NotificationEventPublisher` struct, `build_notification_payload()` (channel-specific enrichment for email/SMS/push), `is_notification_step()`, and the `StepEventPublisher` trait implementation (`publish()`) are all uncovered
- **Test approach**: Create `StepEventContext` fixtures for notification steps. Test `should_handle()` with notification/alert/message step names. Test `build_notification_payload()` with email, SMS, push, and unknown channel contexts. Test `publish()` for success and failure scenarios. Requires mocking `DomainEventPublisher`.

**`order_fulfillment.rs` (15.19% covered)**
- Only struct construction and `name()` methods are covered (via registry tests)
- All four handler `call()` methods are uncovered: `ValidateOrderHandler`, `ReserveInventoryHandler`, `ProcessPaymentHandler`, `ShipOrderHandler`
- Complex validation logic (customer info, order items, quantities, prices, product lookup), dependency resolution between steps, and simulation functions are untested
- **Test approach**: Create `TaskSequenceStep` fixtures with appropriate context fields. Test each handler independently with valid and invalid inputs. Test validation error paths (missing customer, missing items, invalid quantities, price validation, product not found, order total too high). Test inter-step dependency resolution. The simulation functions (`simulate_product_lookup`, `simulate_inventory_reservation`, `simulate_payment_gateway_call`, `simulate_shipping_carrier_call`) can be tested directly.

**`microservices.rs` (20.55% covered)**
- Only struct creation and `name()` covered via registry
- Five handlers uncovered: `CreateUserAccountHandler`, `SetupBillingProfileHandler`, `InitializePreferencesHandler`, `SendWelcomeSequenceHandler`, `UpdateUserStatusHandler`
- Business logic includes email validation, idempotency handling, plan-based billing tiers, preference merging, multi-channel welcome sequences, and workflow aggregation
- **Test approach**: Create fixtures for each handler. Test user creation with valid/invalid emails, name validation, idempotent existing user, and new user creation. Test billing for free/pro/enterprise plans. Test preferences with and without custom overrides. Test welcome sequence channel selection by plan. Test status update aggregation.

**`data_pipeline.rs` (23.88% covered)**
- Only struct creation and `name()` covered via registry
- Eight handlers for full ETL pipeline uncovered
- **Test approach**: Create fixtures for each step. Test extract handlers produce correct simulated data. Test transform handlers process input correctly. Test aggregate handler merges data from multiple dependencies. Test insights generation.

### Medium Priority

**Files**: `diamond_decision_batch.rs`, `batch_processing_products_csv.rs`, `ecommerce.rs`, `batch_processing_example.rs`, `team_scaling.rs`
**Combined uncovered lines**: ~220
**Risk**: Complex workflow patterns that may have subtle bugs in decision routing and batch processing.

**`diamond_decision_batch.rs` (41.67% covered)**
- 10 handlers for a complex diamond+decision+batch combined workflow
- Some struct creation covered, but `call()` bodies for decision routing, batch analysis, and result aggregation are partially covered
- **Test approach**: Test the routing decision handler with different input values. Test batch analyzer handlers produce correct batch specifications. Test aggregate handlers merge results correctly.

**`batch_processing_products_csv.rs` (41.86% covered)**
- CSV-specific batch processing with analyzer, processor, and aggregator
- **Test approach**: Test CSV analysis output, batch processor logic, and result aggregation.

**`ecommerce.rs` (42.86% covered)**
- 5-handler e-commerce workflow (TAS-91 Blog Post 01)
- **Test approach**: Test each handler with fixtures simulating e-commerce operations.

**`batch_processing_example.rs` (45.00% covered)**
- Generic batch processing with dataset analysis, batch worker, and results aggregation
- **Test approach**: Test batch computation logic, dataset analysis, and aggregation.

**`team_scaling.rs` (49.54% covered)**
- Customer success refund workflow and payments namespace handlers
- **Test approach**: Test refund validation, policy checking, manager approval, execution, and status updates. Test payment eligibility, gateway processing, records update, and notifications.

### Lower Priority

**Files**: Files above 55% coverage and infrastructure files
**Risk**: These already meet or approach the target threshold.

| File | Coverage | Notes |
|------|----------|-------|
| `mixed_dag_workflow.rs` | 57.53% | Approaching target, 7 handlers |
| `conditional_approval_rust.rs` | 58.06% | 6 handlers, some paths uncovered |
| `checkpoint_and_fail_handler.rs` | 64.71% | Error injection test handler |
| `diamond_workflow.rs` | 66.67% | Above target |
| `linear_workflow.rs` | 66.67% | Above target |
| `tree_workflow.rs` | 66.67% | Above target |
| `registry.rs` | 71.19% | Above target, good test suite exists |
| `mod.rs` (step_handlers) | 76.81% | Above target |
| `fail_n_times_handler.rs` | 83.33% | Above target |
| `payment_event_publisher.rs` | 84.68% | Above target |
| `metrics_subscriber.rs` | 91.24% | Strong coverage |
| `capability_examples.rs` | 91.46% | Strong coverage |
| `payment_example.rs` | 96.85% | Excellent |
| `domain_event_publishing.rs` | 97.74% | Excellent |
| `logging_subscriber.rs` | 97.75% | Excellent |
| `capabilities.rs` | 100.0% | Complete |

**`global_event_system.rs` (0%, 4 lines)**: Trivial LazyLock singleton. Testing adds minimal value but is easy: call `get_global_event_system()` twice and verify same Arc pointer.

**`main.rs` (0%, 33 lines)**: Binary entry point. Typically not unit-testable and excluded from coverage targets. Signal handling can be verified via integration tests.

---

## Recommended Test Plan

### Phase 1: Quick Wins -- Infrastructure & Config (estimated +2-3% coverage)

1. **Test `global_event_system.rs`**: Call `get_global_event_system()`, verify returns valid Arc. Minimal effort, eliminates a 0% file.

2. **Test `bootstrap.rs` config functions**: Unit test `default_config()`, `no_web_api_config()`, `no_event_driven_config()` -- verify they return correct `WorkerBootstrapConfig` values. Pure functions, no infrastructure needed.

3. **Test `RustWorkerBootstrapResult` Debug impl**: Create a mock result and format it.

### Phase 2: Event Handler Unit Tests (estimated +3-5% coverage)

4. **Test `event_handler.rs`**:
   - Test `RustEventHandler::new()` creates a valid handler
   - Test `handle_step_execution()` with a registered handler (success path)
   - Test `handle_step_execution()` with an unregistered handler (not-found path)
   - Test `handle_step_execution()` with a handler that returns an error
   - Test `publish_step_events()` with no declared events (early return)
   - Test `publish_step_events()` with declared events and publisher that handles/declines
   - Test `publish_step_events()` error logging path

   These tests require mock implementations of `WorkerEventSystem`, `WorkerEventSubscriber`, and `StepEventPublisherRegistry`, which can be constructed without a database.

### Phase 3: Handler Business Logic Tests (estimated +15-25% coverage)

5. **Test `notification_event_publisher.rs`**:
   - Test `should_handle()` with various step names
   - Test `build_notification_payload()` for email, SMS, push, and unknown channels
   - Test `publish()` for success and failure cases

6. **Test `order_fulfillment.rs` handlers**:
   - Create `TaskSequenceStep` fixtures with appropriate context
   - Test `ValidateOrderHandler` with valid order, missing customer, missing items, invalid quantities, product not found, excessive total
   - Test `ReserveInventoryHandler` with valid reservations and insufficient stock
   - Test `ProcessPaymentHandler` with successful and declined payments
   - Test `ShipOrderHandler` with various shipping methods
   - Test helper functions: `simulate_product_lookup`, `simulate_inventory_reservation`, `simulate_payment_gateway_call`, `simulate_shipping_carrier_call`, `calculate_delivery_estimate`

7. **Test `microservices.rs` handlers**:
   - Test `CreateUserAccountHandler` -- valid user, invalid email, empty name, existing user (idempotency)
   - Test `SetupBillingProfileHandler` -- free plan (skipped), pro plan, enterprise plan
   - Test `InitializePreferencesHandler` -- default preferences, custom overrides
   - Test `SendWelcomeSequenceHandler` -- free/pro/enterprise channel selection
   - Test `UpdateUserStatusHandler` -- aggregation from all prior steps
   - Test helper functions: `is_valid_email`, `get_billing_tier`, `get_default_preferences`, `get_welcome_template`, `generate_id`

8. **Test `data_pipeline.rs` handlers**:
   - Test each of the 8 extract/transform/aggregate/insight handlers with fixtures
   - Verify data transformation logic and aggregation

### Phase 4: Remaining Handler Coverage (estimated +5-10% coverage)

9. **Test remaining handlers below 55%**:
   - `diamond_decision_batch.rs` -- routing decisions and batch processing
   - `batch_processing_products_csv.rs` -- CSV-specific logic
   - `ecommerce.rs` -- order processing flow
   - `batch_processing_example.rs` -- generic batch patterns
   - `team_scaling.rs` -- refund workflows and payment processing

10. **Improve files near 55% threshold**:
    - `mixed_dag_workflow.rs` (57.53%) -- test additional DAG paths
    - `conditional_approval_rust.rs` (58.06%) -- test untested approval paths

### Phase 5: Integration Tests (estimated +2-5% coverage)

11. **Bootstrap integration test**: With test database running, test the full `bootstrap()` function and verify service handles are created correctly.

12. **Event handler integration test**: Test the full event handler loop with real broadcast channels and mock handlers.

---

## Estimated Impact

| Phase | Files Targeted | Estimated Lines Added | Estimated New Coverage |
|-------|---------------|----------------------|----------------------|
| Phase 1: Quick Wins | 3 files | ~30 test lines | +2-3% (to ~11-12%) |
| Phase 2: Event Handler | 1 file (184 lines) | ~150 test lines | +3-5% (to ~14-17%) |
| Phase 3: Handler Logic | 4 files (571 lines) | ~400 test lines | +15-25% (to ~29-42%) |
| Phase 4: Remaining Handlers | 5 files (~406 lines) | ~300 test lines | +5-10% (to ~34-52%) |
| Phase 5: Integration | 2 files (294 lines) | ~100 test lines | +2-5% (to ~36-57%) |
| **Total** | **15 files** | **~980 test lines** | **~27-48% increase (to ~36-57%)** |

To reliably reach the 55% target:
- Phases 1-3 are essential (addresses all 0% files and largest uncovered handlers)
- Phase 4 provides the margin to clear the threshold
- Phase 5 is optional but recommended for confidence in the bootstrap/event handler integration

The most impactful single action is **Phase 3** (handler business logic tests), which targets 571 uncovered lines across 4 files with the most complex business logic. These tests can be written as pure unit tests using `TaskSequenceStep` fixtures, requiring no running services.

**Key fixture requirement**: All handler tests need a `TaskSequenceStep` test builder/fixture. If one does not already exist in the test infrastructure, creating a reusable builder that can set context fields, dependency results, and step definitions will accelerate all handler testing.
