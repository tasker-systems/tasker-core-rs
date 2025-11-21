# TAS-47: Blog Post Migration to tasker-core Plan

## Executive Summary

Migrate 5+ blog post examples from Rails engine (tasker-engine) to the new tasker-core Rust-backed system with Ruby FFI bindings. Focus on Ruby migration first, with Rust examples as future enhancement.

## 1. Blog Post Inventory & Analysis

### Confirmed Blog Posts (5 Complete + 1 Partial)

**✅ Post 01: E-commerce Checkout Reliability** (COMPLETE)
- **Narrative**: Black Friday checkout failures → bulletproof workflow
- **Pattern**: Linear workflow (validate → pay → inventory → order → email)
- **Complexity**: Medium - 5 steps, retry logic, external services
- **Rails Features**: YAML config, StepHandler::Base, mock services
- **Lines of Code**: ~2,922 total (step handlers)
- **Tests**: Full integration spec with success/error/retry scenarios
- **Migration**: Direct translation possible, good foundation example

**✅ Post 02: Data Pipeline Resilience** (COMPLETE)
- **Narrative**: 3 AM ETL alerts → reliable analytics pipeline
- **Pattern**: DAG workflow (parallel extracts → transforms → insights)
- **Complexity**: High - 8 steps, parallel execution, data aggregation
- **Rails Features**: Complex DAG, multiple data sources
- **Tests**: ETL workflow specs
- **Migration**: Maps to mixed_dag_workflow pattern

**✅ Post 03: Microservices Coordination** (COMPLETE)
- **Narrative**: Service chaos → orchestrated API coordination
- **Pattern**: Service call orchestration with circuit breakers
- **Complexity**: High - Circuit breakers, API mocking, retry strategies
- **Rails Features**: Custom concern (ApiRequestHandling), circuit breaker pattern
- **Tests**: Service coordination specs
- **Migration**: Circuit breaker concept needs adaptation

**✅ Post 04: Team Scaling** (COMPLETE)
- **Narrative**: Namespace conflicts → multi-team organization
- **Pattern**: Namespace isolation demonstration
- **Complexity**: Medium - Demonstrates namespace feature
- **Rails Features**: Multiple namespaces (payments, customer_success)
- **Tests**: Namespace isolation specs
- **Migration**: Namespace concept unchanged in tasker-core

**⏸️ Post 05: Production Observability** (RAILS COMPLETE, TASKER-CORE DEFERRED)
- **Narrative**: Black box workflows → complete visibility
- **Pattern**: Event monitoring and metrics
- **Complexity**: Medium - Event subscribers, monitoring
- **Rails Features**: Event system (56+ events), custom subscribers
- **Tests**: Observability specs
- **Migration**: ⏸️ DEFERRED TO TAS-65 - Rails event model (56+ developer-facing events) doesn't translate to distributed architecture. TAS-65 defines two-tier approach separating system events (telemetry-only) from custom domain events (developer-facing).

**⚠️ Post 06: Enterprise Security** (PARTIAL)
- **Narrative**: Compliance and audit trails
- **Status**: README and preview only, no full implementation
- **Migration**: Defer until core posts complete

## 2. Pattern Translation Matrix

### YAML Configuration Format

**Rails Engine Format:**
```yaml
name: process_order
namespace_name: ecommerce
version: 1.0.0
task_handler_class: BlogExamples::Post01::OrderProcessingHandler
schema: { JSON Schema }
step_templates:
  - name: validate_cart
    handler_class: BlogExamples::Post01::StepHandlers::ValidateCartHandler
    default_retryable: true
    default_retry_limit: 3
    handler_config: { timeout_seconds: 15 }
```

**tasker-core Format:**
```yaml
:name: process_order
:namespace_name: ecommerce
:version: 1.0.0
:task_handler:
  :callable: Ecommerce::OrderProcessingHandler
  :initialization: { handler config }
:input_schema: { JSON Schema }
:steps:
  - :name: validate_cart
    :handler:
      :callable: Ecommerce::StepHandlers::ValidateCartHandler
      :initialization: { validation_timeout: 10 }
    :retry:
      :retryable: true
      :limit: 3
      :backoff: exponential
```

**Key Differences:**
- Symbol keys (`:name`) vs string keys
- `step_templates` → `:steps`
- `handler_class` → `:handler/:callable`
- `handler_config` → `:handler/:initialization`
- Enhanced retry configuration with backoff strategies
- Environment-specific overrides supported

### Handler Interface Translation

**Rails Engine StepHandler:**
```ruby
class ProcessPaymentHandler < Tasker::StepHandler::Base
  def process(task, sequence, step)
    # Business logic
    { payment_id: result.id, status: 'charged' }
  end

  def process_results(step, payment_response, _initial_results)
    step.results = { formatted_results }
  end

  private

  def step_results(sequence, step_name)
    sequence.steps.find { |s| s.name == step_name }&.results
  end
end
```

**tasker-core StepHandler:**
```ruby
class ProcessPaymentHandler < TaskerCore::StepHandler::Base
  def call(task, sequence, step)
    # Business logic

    TaskerCore::Types::StepHandlerCallResult.success(
      result: { payment_id: result.id, status: 'charged' },
      metadata: { execution_hints: {}, backoff_hints: {} }
    )
  end

  private

  def extract_and_validate_inputs(task, sequence, step)
    sequence.get_results('validate_cart')
  end
end
```

**Key Differences:**
- `process()` → `call()`
- Direct return → `StepHandlerCallResult.success()` wrapper
- `process_results()` removed (handled by FFI bridge)
- `sequence.steps.find` → `sequence.get_results()`
- Metadata for execution hints included in result

### Error Handling Translation

**Rails Engine:**
```ruby
raise Tasker::PermanentError, "Card declined"
raise Tasker::RetryableError, "Gateway timeout"
```

**tasker-core:**
```ruby
raise TaskerCore::Errors::PermanentError.new(
  "Card declined",
  error_code: 'PAYMENT_DECLINED'
)
raise TaskerCore::Errors::RetryableError.new(
  "Gateway timeout",
  retry_after: 30
)
```

**Key Differences:**
- More structured error initialization
- Required `error_code` parameter
- Optional `retry_after` hint
- Optional context hash

## 3. Gap Analysis & Build vs Cut Decisions

### ✅ Direct Translations (Ready Now)

1. **Linear Workflows** (Post 01)
   - Maps perfectly to existing linear_workflow pattern
   - Step dependencies work identically
   - Retry logic fully supported

2. **DAG Workflows** (Post 02)
   - Maps to mixed_dag_workflow pattern
   - Parallel execution supported
   - Dependency resolution identical

3. **Namespace Isolation** (Post 04)
   - Namespace concept unchanged
   - PGMQ queue isolation already implemented

### ⚠️ Adaptations Required (Build)

4. **Event System** (Post 05)
   - **Rails**: 56+ events, custom subscribers, ActiveSupport::Notifications
   - **tasker-core**: PGMQ notifications, event bridge
   - **Decision**: Build simplified event monitoring examples showing PGMQ-based coordination
   - **Scope**: Focus on practical monitoring (task completion, step failures) vs comprehensive event catalog

5. **Circuit Breaker Pattern** (Post 03)
   - **Rails**: Custom concern with circuit breaker logic
   - **tasker-core**: Circuit breakers exist but different API
   - **Decision**: BUILD - Add circuit breaker examples to Ruby gem
   - **Scope**: Demonstrate existing tasker-core circuit breaker integration

### 🚫 Strategic Cuts (Defer or Skip)

6. **GraphQL/REST API Examples**
   - **Rails**: Full GraphQL schema, REST endpoints
   - **tasker-core**: PGMQ-based messaging, minimal HTTP
   - **Decision**: CUT - Focus on PGMQ patterns, not HTTP APIs
   - **Rationale**: Different architectural approach, not core to workflow orchestration

7. **Complex Event Subscribers**
   - **Rails**: Custom subscriber registry, complex event graphs
   - **tasker-core**: Simpler event model
   - **Decision**: SIMPLIFY - Show monitoring patterns, not full event system
   - **Rationale**: Event bridge is fundamentally different

8. **ActiveRecord Integration**
   - **Rails**: Direct AR models, database transactions
   - **tasker-core**: Message-based, PORO patterns
   - **Decision**: ADAPT - Use POROs like existing examples
   - **Rationale**: Already demonstrated in order_fulfillment examples

9. **Authentication/Authorization**
   - **Rails**: Built-in auth system, operation-level permissions
   - **tasker-core**: Not in worker layer
   - **Decision**: CUT - Out of scope for worker examples
   - **Rationale**: Orchestration concern, not worker concern

### 🔧 Infrastructure Decisions (Updated from Phase 1)

10. **Mock Service Framework**
    - **Original Need**: Blog examples use MockPaymentService, MockEmailService, etc.
    - **Original Decision**: PORT - Create equivalent mocks for tasker-core examples
    - **✅ Phase 1 Learning**: Self-contained handlers with inline simulation superior to external mock services
    - **Final Decision**: NO MOCK FRAMEWORK NEEDED - Use inline simulation methods in handlers

11. **Test Helpers**
    - **Original Need**: Blog specs use custom helpers (load_blog_code, execute_workflow)
    - **Original Decision**: BUILD - Create Rust E2E test helpers in `tests/e2e/ruby/`
    - **✅ Phase 1 Learning**: Existing `IntegrationTestManager` provides complete infrastructure
    - **Final Decision**: REUSE - Leverage existing E2E test patterns, no new helpers needed

## 3.5. Key Learnings from Phase 1

**See full report**: `docs/ticket-specs/TAS-47/phase-1-completion.md`

### Migration Patterns Established

1. **Self-Contained Handlers** ✅
   - Inline all simulation logic (no external mock services)
   - Define constants for mock data (PRODUCTS, services, etc.)
   - Create `simulate_*` methods for API calls
   - Zero external dependencies = portable, testable, self-documenting

2. **ActiveSupport Integration** ✅
   - Use built-in `Hash#deep_symbolize_keys` (already loaded)
   - Leverage all ActiveSupport core extensions
   - Remove custom helper methods duplicating functionality

3. **Minimal Requires** ✅
   - Remove requires for core infrastructure (loaded by `lib/tasker_core.rb`)
   - Keep only domain-specific requires (if any)
   - Clean handler files with no deep relative paths

4. **Test Strategy** ✅
   - Happy path focus for blog demonstrations
   - E2E tests validate complete workflow execution
   - Reuse existing `IntegrationTestManager` infrastructure
   - 15-20 second timeouts for multi-step workflows

5. **Template Organization** ✅
   - Place in `tests/fixtures/task_templates/ruby/`
   - Symbol keys (`:name`, `:namespace_name`, etc.)
   - Enhanced retry configuration with backoff strategies
   - Environment variables handle auto-discovery

### Code Quality Improvements

- **115 lines removed** through better patterns (ActiveSupport usage, minimal requires)
- **Zero external dependencies** in all handlers
- **1 passing E2E test** validating complete 5-step workflow
- **~1,331 lines total** for complete Post 01 migration

### Anti-Patterns to Avoid

❌ External mock service classes
❌ ActiveRecord model dependencies
❌ Custom helpers for built-in functionality
❌ Excessive `require_relative` chains
❌ Comprehensive edge-case testing in blog examples

## 4. Test Integration Strategy

### Test Structure

```
tasker-core/
├── workers/ruby/spec/
│   ├── blog_examples/                # NEW - Ruby unit/integration tests
│   │   ├── support/                  # Test infrastructure
│   │   │   ├── mock_services/       # Payment, Email, Inventory mocks
│   │   │   └── blog_spec_helper.rb  # Common test setup
│   │   │
│   │   ├── post_01_ecommerce/       # Post 01 handler tests
│   │   │   ├── handlers/
│   │   │   │   └── order_processing_handler_spec.rb
│   │   │   └── step_handlers/
│   │   │       ├── validate_cart_handler_spec.rb
│   │   │       ├── process_payment_handler_spec.rb
│   │   │       └── ...
│   │   │
│   │   ├── post_02_data_pipeline/   # Post 02 handler tests
│   │   └── ...
│   │
│   └── handlers/examples/            # EXISTING - Keep as-is
│
├── tests/e2e/ruby/                   # EXISTING + NEW E2E tests
│   ├── batch_processing_csv_test.rs # Existing
│   ├── conditional_approval_test.rs # Existing
│   ├── error_scenarios_test.rs      # Existing
│   ├── ecommerce_checkout_test.rs   # NEW - Post 01 E2E
│   ├── data_pipeline_test.rs        # NEW - Post 02 E2E
│   ├── microservices_coordination_test.rs  # NEW - Post 03 E2E
│   ├── namespace_isolation_test.rs  # NEW - Post 04 E2E
│   ├── observability_test.rs        # NEW - Post 05 E2E
│   └── mod.rs
│
└── workers/ruby/spec/handlers/examples/
    └── blog_examples/                # NEW - Ruby handler implementations
        ├── post_01_ecommerce/
        │   ├── handlers/
        │   │   └── order_processing_handler.rb
        │   ├── step_handlers/
        │   │   ├── validate_cart_handler.rb
        │   │   ├── process_payment_handler.rb
        │   │   ├── update_inventory_handler.rb
        │   │   ├── create_order_handler.rb
        │   │   └── send_confirmation_handler.rb
        │   └── config/
        │       └── order_processing_handler.yaml
        │
        ├── post_02_data_pipeline/
        └── ...
```

### E2E Test Approach (Rust-based)

**Extend Existing `tests/e2e/ruby/` Patterns:**

```rust
// tests/e2e/ruby/ecommerce_checkout_test.rs

use super::common::*;

#[tokio::test]
async fn test_successful_ecommerce_checkout_workflow() -> TestResult<()> {
    let fixture_path = get_fixture_path("ecommerce_checkout.yaml");

    // Load task template
    let config = load_task_template(&fixture_path)?;

    // Initialize test environment
    let test_env = TestEnvironment::new().await?;

    // Create task with checkout context
    let task_uuid = test_env.create_task(
        "ecommerce",
        "process_order",
        json!({
            "cart_items": [
                { "product_id": 1, "quantity": 2, "price": 29.99 }
            ],
            "payment_info": {
                "method": "credit_card",
                "token": "tok_test_123",
                "amount": 59.98
            },
            "customer_info": {
                "email": "customer@example.com",
                "name": "Test Customer",
                "tier": "standard"
            }
        })
    ).await?;

    // Wait for workflow completion
    test_env.wait_for_task_completion(task_uuid, Duration::from_secs(30)).await?;

    // Verify task completed successfully
    let task_status = test_env.get_task_status(task_uuid).await?;
    assert_eq!(task_status.status, "Complete");

    // Verify all steps completed in correct order
    let steps = test_env.get_task_steps(task_uuid).await?;
    assert_eq!(steps.len(), 5);
    assert_eq!(steps[0].name, "validate_cart");
    assert_eq!(steps[1].name, "process_payment");
    assert_eq!(steps[2].name, "update_inventory");
    assert_eq!(steps[3].name, "create_order");
    assert_eq!(steps[4].name, "send_confirmation");

    // Verify all steps have Complete status
    for step in &steps {
        assert_eq!(step.status, "Complete", "Step {} should be Complete", step.name);
    }

    Ok(())
}

#[tokio::test]
async fn test_payment_failure_retry_logic() -> TestResult<()> {
    let test_env = TestEnvironment::new().await?;

    // Create task with invalid payment (will trigger retry)
    let task_uuid = test_env.create_task(
        "ecommerce",
        "process_order",
        json!({
            "cart_items": [{ "product_id": 1, "quantity": 1, "price": 10.00 }],
            "payment_info": {
                "method": "credit_card",
                "token": "tok_simulate_timeout",  // Special token to simulate timeout
                "amount": 10.00
            },
            "customer_info": {
                "email": "customer@example.com",
                "name": "Test Customer"
            }
        })
    ).await?;

    // Wait for retry attempts
    test_env.wait_for_task_completion(task_uuid, Duration::from_secs(60)).await?;

    // Verify retry attempts were made
    let task_status = test_env.get_task_status(task_uuid).await?;
    let payment_step = task_status.steps.iter()
        .find(|s| s.name == "process_payment")
        .expect("Payment step should exist");

    assert!(payment_step.retry_count > 0, "Payment should have been retried");

    Ok(())
}

#[tokio::test]
async fn test_premium_customer_optimization() -> TestResult<()> {
    let test_env = TestEnvironment::new().await?;

    // Create task with premium customer tier
    let task_uuid = test_env.create_task(
        "ecommerce",
        "process_order",
        json!({
            "cart_items": [{ "product_id": 1, "quantity": 1, "price": 100.00 }],
            "payment_info": {
                "method": "credit_card",
                "token": "tok_test_premium",
                "amount": 100.00
            },
            "customer_info": {
                "email": "premium@example.com",
                "name": "Premium Customer",
                "tier": "premium"  // Premium customer
            }
        })
    ).await?;

    // Wait for completion
    test_env.wait_for_task_completion(task_uuid, Duration::from_secs(30)).await?;

    // Verify premium handling applied
    let task = test_env.get_task(task_uuid).await?;
    assert!(task.context.contains_key("processing_priority"));
    assert_eq!(task.context["processing_priority"], "high");

    Ok(())
}
```

### RSpec Test Pattern (Unit/Component Tests Only)

**RSpec for Ruby Handler Logic Testing:**
```ruby
# workers/ruby/spec/blog_examples/post_01_ecommerce/step_handlers/process_payment_handler_spec.rb

RSpec.describe Ecommerce::StepHandlers::ProcessPaymentHandler do
  describe '#call' do
    let(:handler) { described_class.new }
    let(:task) { build_test_task_with_context(payment_context) }
    let(:sequence) { build_test_sequence_with_results(prior_step_results) }
    let(:step) { build_test_step('process_payment') }

    context 'with valid payment information' do
      it 'processes payment successfully' do
        result = handler.call(task, sequence, step)

        expect(result).to be_success
        expect(result.result[:payment_processed]).to be true
        expect(result.result[:payment_id]).to be_present
        expect(result.result[:transaction_id]).to be_present
      end

      it 'includes execution metadata' do
        result = handler.call(task, sequence, step)

        expect(result.metadata[:execution_hints]).to be_present
        expect(result.metadata[:http_headers]).to include('X-Payment-Gateway')
      end
    end

    context 'with invalid payment token' do
      it 'raises permanent error' do
        expect {
          handler.call(task_with_invalid_token, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError)
      end
    end

    context 'with gateway timeout' do
      it 'raises retryable error' do
        allow(handler).to receive(:simulate_payment_gateway_call)
          .and_return({ status: 'failed', error_code: 'gateway_timeout' })

        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::RetryableError)
      end
    end
  end
end
```

### Test Execution Commands

**RSpec (Unit/Component Tests):**
```bash
# Run all blog example unit tests
cd workers/ruby
bundle exec rspec spec/blog_examples/

# Run specific post tests
bundle exec rspec spec/blog_examples/post_01_ecommerce/
```

**E2E Tests (Rust-based):**
```bash
# Run all E2E tests including blog examples
cargo test --test '*' --features test-helpers

# Run specific blog example E2E test
cargo test --test ecommerce_checkout_test

# Run with output for debugging
cargo test --test ecommerce_checkout_test -- --nocapture
```

## 5. Rust Translation Approach (Future)

**Defer to Future Phase** - Focus Ruby-only migration first

When ready for Rust examples:

1. **Create Parallel Structure:**
   ```
   tasker-core/workers/rust/examples/
   └── blog_examples/
       ├── post_01_ecommerce/
       ├── post_02_data_pipeline/
       └── ...
   ```

2. **Rust Handler Pattern:**
   ```rust
   impl StepHandler for ProcessPaymentHandler {
       async fn call(&self, ctx: StepContext) -> Result<StepResult> {
           // Same business logic as Ruby
       }
   }
   ```

3. **Shared YAML Configs:**
   - Same YAML definitions work for both Ruby and Rust
   - Handler callables differ: `Ecommerce::Handler` vs `ecommerce::Handler`

## 6. Migration Priority & Phasing

### Phase 1: Foundation ✅ COMPLETE
**Goal**: Establish infrastructure and migrate simplest example

**Status**: ✅ COMPLETE (2025-11-19)
**See**: `docs/ticket-specs/TAS-47/phase-1-completion.md` for detailed report

**Completed Tasks:**
1. ✅ Created directory structure:
   - `workers/ruby/spec/handlers/examples/blog_examples/post_01_ecommerce/`
   - `tests/e2e/ruby/ecommerce_order_test.rs`
2. ✅ Established self-contained handler pattern (inline simulation, no external dependencies)
3. ✅ Leveraged existing E2E test infrastructure (`IntegrationTestManager`)
4. ✅ Migrated Post 01 (E-commerce) as proof of concept:
   - ✅ Ported all 5 step handlers (validate_cart, process_payment, update_inventory, create_order, send_confirmation)
   - ✅ Created task handler (order_processing_handler.rb)
   - ✅ Created YAML config (ecommerce_order_processing.yaml)
   - ✅ Created Rust E2E test passing all validations
5. ✅ Documented migration patterns and learnings

**Success Criteria Met:**
- ✅ Post 01 handlers implemented with zero external dependencies
- ✅ E2E test executing full 5-step workflow end-to-end (passing)
- ✅ Self-contained simulation pattern established (no mock service framework needed)
- ✅ Test infrastructure reusable (existing IntegrationTestManager)
- ✅ Clear documentation of patterns (phase-1-completion.md)

**Key Learnings:**
- **Self-contained handlers**: Inline simulation superior to external mock services
- **ActiveSupport usage**: Use built-in `Hash#deep_symbolize_keys` instead of custom helpers
- **Minimal requires**: Remove infrastructure requires already loaded by `lib/tasker_core.rb`
- **Test focus**: Happy path sufficient for blog demonstrations
- **Template location**: Must be in `tests/fixtures/task_templates/ruby/` for Docker auto-discovery

### Phase 2: Namespace Isolation ✅ COMPLETE
**Goal**: Demonstrate multi-team organization with namespace isolation

**Status**: ✅ COMPLETE (2025-11-20)

**Rationale**: Started with Post 04 before Post 02 because:
- Simpler pattern (namespace feature demonstration vs complex DAG)
- Builds directly on Post 01 foundation (linear workflows)
- Clear demonstration of existing tasker-core feature
- Post 02 (DAG) has higher complexity (8 steps, parallel execution)

**Completed Tasks:**
1. ✅ **Migrated Post 04** (Team Scaling - Namespace Isolation)
   - Created handlers for 2 namespaces (`payments`, `customer_success`)
   - Demonstrated namespace isolation with concurrent task execution
   - Self-contained handlers following Post 01 pattern (inline simulation, zero dependencies)
   - Created 2 YAML templates (payments_process_refund.yaml, customer_success_process_refund.yaml)
   - Created Rust E2E test with 3 scenarios (`namespace_isolation_test.rs`)
   - Tested concurrent execution across namespaces successfully
2. ✅ Applied Phase 1 learnings:
   - Self-contained handlers with inline simulation (no mock services)
   - Used ActiveSupport built-ins (`Hash#deep_symbolize_keys`)
   - Minimal require statements
   - Happy path E2E testing focus
3. ✅ Removed example handler unit tests:
   - Deleted 5 RSpec unit tests for Post 01 example handlers
   - Kept framework-level tests (types, FFI, decision points, batch processing)
   - E2E tests provide complete validation of handler functionality

**Implementation Details:**
- **11 handlers total**: 2 task handlers + 9 step handlers
- **Payments namespace** (4 steps): validate_payment_eligibility → process_gateway_refund → update_payment_records → notify_customer
- **Customer Success namespace** (5 steps): validate_refund_request → check_refund_policy → get_manager_approval → execute_refund_workflow → update_ticket_status
- **Cross-namespace coordination**: customer_success calls payments workflow (simulated)
- **Inline policy rules**: REFUND_POLICIES constant hash for tier-based policy checks

**Bonus Achievements:**
- 🐛 Fixed database constraint bug: Changed `tasker_named_tasks_namespace_name_unique` to allow multiple versions per namespace
- 🐛 Fixed payment ID validation: Updated regex to allow underscores (`/^pay_[a-zA-Z0-9_]+$/`)

**Success Criteria Met:**
- ✅ Multiple namespace handlers implemented (payments + customer_success)
- ✅ Namespace isolation demonstrated (concurrent execution test)
- ✅ E2E test showing multi-namespace workflow (3 test scenarios, all passing)
- ✅ All tests passing (E2E + framework RSpec)
- ✅ Patterns established for future phases

**Key Learnings:**
- **Namespace isolation works perfectly**: Same workflow name (`process_refund`) in different namespaces executes without conflicts
- **Database versioning important**: Multiple template versions per namespace required for production deployments
- **Cross-namespace patterns**: Demonstrate coordination through simulated task creation (actual implementation would use orchestration API)
- **Policy-driven logic**: Inline constants (like REFUND_POLICIES) work well for tier-based business rules

### Phase 3: DAG Patterns ✅ COMPLETE
**Goal**: Migrate complex parallel workflow patterns

**Status**: ✅ COMPLETE (2025-11-20)

**Post 02 - Data Pipeline Resilience**
**Narrative**: 3 AM ETL alerts → reliable analytics pipeline
**Pattern**: DAG workflow with parallel extracts, transforms, and aggregation

**Completed Tasks:**
1. ✅ **Migrated Post 02** (Data Pipeline Resilience - DAG Pattern)
   - Created handlers for 8-step DAG workflow
   - Demonstrated parallel extraction (3 parallel steps with no dependencies)
   - Demonstrated sequential transformation (3 steps each depending on its extract)
   - Demonstrated DAG convergence (aggregate step depends on all 3 transforms)
   - Self-contained handlers with inline simulation (SAMPLE_SALES_DATA, SAMPLE_INVENTORY_DATA, SAMPLE_CUSTOMER_DATA)
   - Created YAML template (data_pipeline_analytics_pipeline.yaml)
   - Created Rust E2E test validating complete DAG execution
   - Test passed on first try and verified via ruby-worker logs

**Implementation Details:**
- **8 handlers total**: 1 task handler + 7 step handlers
- **Extract Phase** (3 parallel steps, no dependencies):
  - extract_sales_data: 5 sample sales records
  - extract_inventory_data: 8 sample inventory items
  - extract_customer_data: 5 sample customer profiles
- **Transform Phase** (3 sequential steps):
  - transform_sales: Calculate daily totals and product summaries
  - transform_inventory: Calculate warehouse summaries and reorder alerts
  - transform_customers: Calculate tier analysis and lifetime value
- **Aggregate Phase** (1 step - DAG convergence):
  - aggregate_metrics: Combine all 3 transform results, calculate cross-source metrics
- **Insights Phase** (1 step):
  - generate_insights: Generate business intelligence with health score

**Key DAG Patterns Demonstrated:**
- **Parallel Execution**: 3 extract steps run concurrently (no dependencies)
- **Sequential Dependencies**: Each transform depends on its corresponding extract
- **DAG Convergence**: Aggregate step depends on all 3 transforms (3 branches → 1 step)
- **Multi-Branch Aggregation**: `sequence.get_results()` accesses results from multiple parallel branches
- **Cross-Source Analytics**: Calculate metrics spanning multiple data sources

**Log Verification:**
Ruby-worker logs confirmed successful execution:
- Extract steps: All 3 extracted proper sample data from inline constants
- Transform steps: Each accessed prior step results via `sequence.get_results()`
- Aggregate step: Successfully converged all 3 transform branches with 3 sources
- Insights step: Generated 3 business insights with health score "Excellent" (100/100)

**Success Criteria Met:**
- ✅ 8 handlers implemented (1 task + 7 step handlers)
- ✅ 1 YAML template (data_pipeline_analytics_pipeline.yaml)
- ✅ 1 E2E test demonstrating DAG execution (passing)
- ✅ Parallel execution validated (3 extract steps with no dependencies)
- ✅ DAG convergence demonstrated (aggregate depends on all 3 transforms)
- ✅ Data aggregation across branches working (logs confirmed)
- ✅ All tests passing
- ✅ Execution verified via logs

**Key Learnings:**
- **DAG convergence works perfectly**: Aggregate step successfully accessed results from all 3 parallel transform branches
- **Parallel execution validated**: Extract phase demonstrated concurrent execution of independent steps
- **Cross-source aggregation**: Successfully combined metrics from sales, inventory, and customer data
- **Self-contained simulation**: Inline constants (SAMPLE_*_DATA) provide realistic data without external dependencies
- **First-try success**: Well-established patterns from Phases 1-2 enabled zero-bug implementation

### Phase 4: Microservices Coordination ✅ COMPLETE
**Goal**: Demonstrate parallel execution and service coordination patterns

**Status**: ✅ COMPLETE (2025-11-20)

**Post 03 - Microservices Coordination**
**Narrative**: Service chaos → orchestrated API coordination
**Pattern**: User registration workflow with parallel service calls

**Completed Tasks:**
1. ✅ **Migrated Post 03** (Microservices Coordination)
   - Created 5-step user registration workflow
   - Demonstrated parallel execution (billing + preferences)
   - Demonstrated error classification (RetryableError vs PermanentError)
   - Demonstrated idempotent operations (409 conflict handling)
   - Demonstrated graceful degradation (free plan skips billing)
   - Self-contained handlers with inline simulation (no mock service framework)
   - Created YAML template (microservices_user_registration.yaml)
   - Created Rust E2E test with 3 scenarios (all passing)

**Implementation Details:**
- **6 handlers total**: 1 task handler + 5 step handlers
- **Workflow Structure**:
  - create_user_account (sequential, with idempotency)
  - setup_billing_profile + initialize_preferences (parallel)
  - send_welcome_sequence (depends on billing + preferences, convergence)
  - update_user_status (final step)
- **Simplification vs Rails**: ~600 lines vs 1200+ (no mock service framework)

**Key Patterns Demonstrated:**
- **Parallel Execution**: Billing and preferences run concurrently
- **Circuit Breaker**: Tasker's retry system with error classification
- **Idempotency**: Create user handles 409 conflicts gracefully
- **Graceful Degradation**: Free plan skips billing setup
- **Multi-Service Coordination**: 4 simulated services

**Success Criteria Met:**
- ✅ 6 handlers implemented (1 task + 5 step)
- ✅ 1 YAML template (microservices_user_registration.yaml)
- ✅ 3 E2E tests passing (pro, free, enterprise plans)
- ✅ Parallel execution demonstrated
- ✅ Error classification and graceful degradation shown
- ✅ All patterns self-contained (no external mock services)

### Phase 5: Observability ⏸️ DEFERRED TO TAS-65
**Goal**: Event monitoring and metrics integration

**Status**: ⏸️ DEFERRED - See TAS-65 for distributed event system architecture

**Rationale for Deferral:**
The Rails engine's event system (56+ events exposed to developers) doesn't translate directly to tasker-core's distributed architecture. A comprehensive redesign is needed that separates:
- **System Events**: Internal lifecycle events → OpenTelemetry only (not developer-facing)
- **Custom Domain Events**: Business logic events → Developer-facing pub/sub API

**See**: `docs/ticket-specs/TAS-65.md` for the distributed event system specification

**Post 05 will be revisited after TAS-65 Phase 1** (System Events → OpenTelemetry mapping)

### Phase 5: Documentation Integration (Deferred)
**Goal**: Update blog content and finalize

**Tasks:**
1. Update all blog post markdown files
   - Replace Rails code examples with tasker-core
   - Update explanations for new patterns
   - Add migration notes where relevant
2. Create migration guide document
3. Update setup scripts for Docker-based examples
4. Final E2E testing of all examples
5. Documentation review and polish

**Success Criteria:**
- All blog posts updated
- Migration guide complete
- Setup scripts working
- Full test coverage (E2E tests for all examples)

## 7. Documentation Plan

### Create Migration Guide
**Location**: `docs/ticket-specs/TAS-47/migration-guide.md`

**Contents:**
- Rails Engine → tasker-core pattern mappings
- YAML format conversion guide
- Handler interface changes
- Error handling updates
- Event system differences
- Testing approach changes (RSpec vs Rust E2E)

### Update Blog Posts
**For Each Post:**
1. Keep narrative and problem statement (timeless)
2. Update "The Solution" code examples to tasker-core
3. Add "Migration Notes" section explaining key differences
4. Update setup instructions for Docker-based execution
5. Link to working code in tasker-core repo

### Create Index Documents

**Location**: `workers/ruby/spec/handlers/examples/blog_examples/README.md`

**Contents:**
- Overview of blog examples
- Quick start guide
- Testing instructions (RSpec unit tests)
- Links to each example
- Differences from Rails engine

**Location**: `tests/e2e/ruby/README.md` (extend existing)

**Contents:**
- Overview of E2E test structure
- Blog example E2E tests
- Running instructions
- Fixture management
- Debugging tips

## 8. Risk Mitigation

### Technical Risks

**Risk 1: FFI Bridge Limitations**
- **Mitigation**: Start with simplest example (Post 01)
- **Fallback**: Document limitations, adapt examples

**Risk 2: E2E Test Complexity**
- **Mitigation**: Follow existing patterns in `tests/e2e/ruby/`
- **Fallback**: Simplify test scenarios if necessary

**Risk 3: Mock Service Complexity**
- **Mitigation**: Port incrementally, test each service
- **Fallback**: Simplify mocks to essential behavior only

**Risk 4: Rust Test Environment Setup**
- **Mitigation**: Reuse existing E2E infrastructure
- **Fallback**: Document manual setup if automation fails

### Scope Risks

**Risk 5: Blog Content Divergence**
- **Mitigation**: Keep narratives, adapt only code
- **Fallback**: Create "tasker-core edition" parallel blog

**Risk 6: Feature Gap Discovery**
- **Mitigation**: Case-by-case evaluation per our agreement
- **Fallback**: Document differences, focus on core value

## 9. Success Metrics

1. **Code Migration**: 5 blog posts with working Ruby examples
2. **Test Coverage**:
   - All handler unit tests passing (RSpec)
   - All E2E workflow tests passing (Rust)
3. **Documentation**: Blog posts updated with new code
4. **Developer Experience**: Setup time < 5 minutes (Docker-based)
5. **Pattern Completeness**: Linear, DAG, namespace, events all demonstrated

## 10. Next Steps

### ✅ Completed
1. ✅ Plan approved
2. ✅ **Phase 1 complete** (2025-11-19) - Post 01 E-commerce
   - Completion documented (`phase-1-completion.md`)
   - Migration patterns established and validated
   - Self-contained handler pattern established
3. ✅ **Phase 2 complete** (2025-11-20) - Post 04 Namespace Isolation
   - 11 handlers across 2 namespaces (payments, customer_success)
   - 2 YAML templates
   - 3 E2E tests (all passing)
   - Fixed database constraint bug (versioning)
   - Removed example handler unit tests (framework tests retained)
4. ✅ **Phase 3 complete** (2025-11-20) - Post 02 Data Pipeline (DAG)
   - 8 handlers demonstrating DAG workflow
   - 1 YAML template (data_pipeline_analytics_pipeline.yaml)
   - 1 E2E test (data_pipeline_test.rs) - passed on first try
   - Verified via ruby-worker logs (all steps executed correctly)
   - Demonstrated parallel execution, DAG convergence, multi-branch aggregation
5. ✅ **Phase 4 complete** (2025-11-20) - Post 03 Microservices Coordination
   - 6 handlers demonstrating parallel service coordination
   - 1 YAML template (microservices_user_registration.yaml)
   - 3 E2E tests (pro, free, enterprise plans) - all passing
   - Demonstrated parallel execution, idempotency, graceful degradation
   - Simplified from Rails: ~600 lines vs 1200+ (no mock service framework)
6. ⏸️ **Phase 5 deferred** to TAS-65 - Post 05 Observability
   - Created TAS-65 specification (`docs/ticket-specs/TAS-65.md`)
   - Defined two-tier event architecture:
     - System Events → OpenTelemetry (not developer-facing)
     - Custom Domain Events → Developer-facing pub/sub API

### 🎯 Current Status

**Phase 1**: ✅ COMPLETE (2025-11-19) - E-commerce (Post 01)
**Phase 2**: ✅ COMPLETE (2025-11-20) - Namespace Isolation (Post 04)
**Phase 3**: ✅ COMPLETE (2025-11-20) - DAG Patterns (Post 02)
**Phase 4**: ✅ COMPLETE (2025-11-20) - Microservices Coordination (Post 03)
**Phase 5**: ⏸️ DEFERRED TO TAS-65 - Observability (Post 05)

### 📊 Migration Summary

**Completed**: 4 out of 5 blog posts migrated (80% complete)
- ✅ Post 01: E-commerce Checkout Reliability (Linear workflow)
- ✅ Post 02: Data Pipeline Resilience (DAG workflow)
- ✅ Post 03: Microservices Coordination (Parallel execution, circuit breaker)
- ✅ Post 04: Team Scaling (Namespace isolation)
- ⏸️ Post 05: Production Observability (Deferred to TAS-65 event system redesign)

**Total Handlers Implemented**: 30 handlers across 4 posts
- Post 01: 6 handlers (1 task + 5 step)
- Post 02: 8 handlers (1 task + 7 step)
- Post 03: 6 handlers (1 task + 5 step)
- Post 04: 11 handlers (2 task + 9 step)

**Total E2E Tests**: 7 passing tests
- ecommerce_order_test.rs (5 steps, linear workflow)
- data_pipeline_test.rs (8 steps, DAG workflow)
- microservices_coordination_test.rs (3 test scenarios: pro, free, enterprise)
- namespace_isolation_test.rs (3 test scenarios, 2 namespaces)

### 📋 Next Steps

**TAS-47 Core Migration: COMPLETE**

All core workflow patterns have been successfully demonstrated:
- ✅ Linear workflows (Post 01)
- ✅ DAG workflows with parallel execution and convergence (Post 02)
- ✅ Multi-service coordination with parallel handlers (Post 03)
- ✅ Namespace isolation (Post 04)

**Post 05 (Observability) Status**:
- ⏸️ Deferred to **TAS-65: Distributed Event System Architecture**
- See `docs/ticket-specs/TAS-65.md` for comprehensive specification
- Rails event system (56+ events) doesn't translate to distributed architecture
- TAS-65 defines two-tier approach:
  - System Events → OpenTelemetry (spans, metrics, logs)
  - Custom Domain Events → Developer-facing pub/sub API

**Remaining Documentation Tasks** (optional):
1. Create comprehensive migration guide
2. Update blog post markdown with tasker-core examples
3. Create README for blog examples directory

---

**Migration Status**: SUCCESS - 4/5 posts migrated, 30 handlers, 7 E2E tests

---

## 11. Branch Completion Summary

**Branch**: `jcoletaylor/tas-47-ruby-blog-post-integration`
**Completed**: 2025-11-20
**Status**: READY FOR PR

### What Was Accomplished

This branch successfully migrated 4 out of 5 Rails engine blog post examples to tasker-core with Ruby FFI bindings. The migration demonstrated that all core workflow orchestration patterns work correctly in the new distributed Rust-backed architecture.

### Files Created

**Handlers (30 total)**:
```
workers/ruby/spec/handlers/examples/blog_examples/
├── post_01_ecommerce/
│   ├── handlers/order_processing_handler.rb
│   └── step_handlers/ (5 handlers)
├── post_02_data_pipeline/
│   ├── handlers/analytics_pipeline_handler.rb
│   └── step_handlers/ (7 handlers)
├── post_03_microservices_coordination/
│   ├── handlers/user_registration_handler.rb
│   └── step_handlers/ (5 handlers)
└── post_04_namespace_isolation/
    ├── payments/handlers/ (1 task + 4 step handlers)
    └── customer_success/handlers/ (1 task + 5 step handlers)
```

**YAML Templates (5 total)**:
```
tests/fixtures/task_templates/ruby/
├── ecommerce_order_processing.yaml
├── data_pipeline_analytics_pipeline.yaml
├── microservices_user_registration.yaml
├── payments_process_refund.yaml
└── customer_success_process_refund.yaml
```

**E2E Tests (4 test files, 7 test scenarios)**:
```
tests/e2e/ruby/
├── ecommerce_order_test.rs (1 test)
├── data_pipeline_test.rs (1 test)
├── microservices_coordination_test.rs (3 tests)
└── namespace_isolation_test.rs (3 tests)
```

**Specifications**:
```
docs/ticket-specs/
├── TAS-47/plan.md (this document, updated)
└── TAS-65.md (new: Distributed Event System Architecture)
```

### Key Patterns Established

1. **Self-Contained Handlers**: Inline simulation with constants (no external mock services)
2. **ActiveSupport Integration**: Use built-in methods (`deep_symbolize_keys`)
3. **Minimal Requires**: Infrastructure loaded by `lib/tasker_core.rb`
4. **E2E Test Focus**: Happy path testing via Rust tests, no RSpec unit tests for examples
5. **Template Naming**: `{namespace}_{workflow_name}.yaml`

### Workflow Patterns Demonstrated

| Pattern | Post | Steps | Key Feature |
|---------|------|-------|-------------|
| Linear | Post 01 | 5 | Sequential execution, retry logic |
| DAG | Post 02 | 8 | Parallel execution, convergence |
| Parallel Services | Post 03 | 5 | Billing ∥ Preferences, idempotency |
| Namespace Isolation | Post 04 | 4+5 | Multi-namespace, same workflow names |

### Deferred Work

**Post 05 (Observability)** → **TAS-65: Distributed Event System**

The Rails engine's event system (56+ events exposed to developers via `ActiveSupport::Notifications`) doesn't translate to tasker-core's distributed architecture. TAS-65 specification defines:

- **System Events**: Internal lifecycle events → OpenTelemetry only (not developer-facing)
- **Custom Domain Events**: Business logic events → Developer-facing pub/sub API via PGMQ

### Migration Statistics

| Metric | Value |
|--------|-------|
| Blog posts migrated | 4/5 (80%) |
| Total handlers | 30 |
| Total YAML templates | 5 |
| Total E2E tests | 7 |
| Lines of Ruby code | ~2,500 |
| Lines saved vs Rails | ~50% (self-contained pattern) |
| Test success rate | 100% |

### PR Checklist

- [x] All 4 blog posts migrated with working handlers
- [x] All E2E tests passing
- [x] Plan document updated with completion status
- [x] TAS-65 specification created for deferred work
- [x] No breaking changes to existing code
- [x] Docker test environment verified working
