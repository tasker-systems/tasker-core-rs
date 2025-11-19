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

**✅ Post 05: Production Observability** (COMPLETE)
- **Narrative**: Black box workflows → complete visibility
- **Pattern**: Event monitoring and metrics
- **Complexity**: Medium - Event subscribers, monitoring
- **Rails Features**: Event system (56+ events), custom subscribers
- **Tests**: Observability specs
- **Migration**: Event model differs significantly (PGMQ-based)

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

### 🔧 Infrastructure to Build

10. **Mock Service Framework**
    - **Need**: Blog examples use MockPaymentService, MockEmailService, etc.
    - **Decision**: PORT - Create equivalent mocks for tasker-core examples
    - **Scope**: Simple mock services for testing (payment, email, inventory)

11. **Test Helpers**
    - **Need**: Blog specs use custom helpers (load_blog_code, execute_workflow)
    - **Decision**: BUILD - Create Rust E2E test helpers in `tests/e2e/ruby/`
    - **Scope**: Extend existing E2E test patterns for blog examples

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

### Phase 1: Foundation (Week 1)
**Goal**: Establish infrastructure and migrate simplest example

**Tasks:**
1. Create directory structure:
   - `workers/ruby/spec/handlers/examples/blog_examples/`
   - `workers/ruby/spec/blog_examples/` for RSpec tests
   - `tests/e2e/ruby/*_test.rs` for E2E tests
2. Create mock service framework (port from Rails engine)
3. Create test helpers for E2E tests in `tests/e2e/ruby/common.rs`
4. **Migrate Post 01** (E-commerce) as proof of concept
   - Port all 5 step handlers
   - Port task handler
   - Port YAML config
   - Create RSpec unit tests for handlers
   - Create Rust E2E test (`ecommerce_checkout_test.rs`)
   - Create fixture YAML for E2E tests
5. Document migration patterns and learnings

**Success Criteria:**
- Post 01 handlers implemented and passing unit tests
- E2E test executing full workflow end-to-end
- Mock services functional
- Test infrastructure reusable
- Clear documentation of patterns

### Phase 2: DAG Patterns (Week 2)
**Goal**: Migrate complex workflow patterns

**Tasks:**
1. **Migrate Post 02** (Data Pipeline)
   - 8-step DAG workflow
   - Parallel execution patterns
   - Data aggregation examples
   - RSpec unit tests for handlers
   - Rust E2E test (`data_pipeline_test.rs`)
2. **Migrate Post 04** (Team Scaling)
   - Namespace isolation
   - Multi-team patterns
   - RSpec tests
   - Rust E2E test (`namespace_isolation_test.rs`)
3. Update blog markdown with new code references

**Success Criteria:**
- DAG workflows executing correctly in E2E tests
- Namespace isolation demonstrated
- All RSpec and E2E tests passing

### Phase 3: Advanced Features (Week 3)
**Goal**: Tackle adaptations and new patterns

**Tasks:**
1. **Migrate Post 03** (Microservices Coordination)
   - BUILD circuit breaker examples
   - Service call orchestration
   - Retry strategies
   - RSpec tests
   - Rust E2E test (`microservices_coordination_test.rs`)
2. **Migrate Post 05** (Observability)
   - SIMPLIFY event examples for PGMQ
   - Monitoring patterns
   - Metrics integration
   - RSpec tests
   - Rust E2E test (`observability_test.rs`)
3. Document differences from Rails engine
4. Update blog narratives

**Success Criteria:**
- Circuit breaker integration shown
- Event monitoring patterns clear
- Blog narratives updated
- All tests passing

### Phase 4: Documentation Integration (Week 4)
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
- Full test coverage (RSpec + E2E)

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

1. **✅ Plan approved** - Ready to proceed
2. **Create detailed Phase 1 tasks** in TAS-47 subtasks
3. **Set up development branch structure**
4. **Begin Phase 1 implementation**:
   - Mock services framework
   - E2E test helpers
   - Post 01 migration

---

**Recommendation**: Begin Phase 1 immediately. Post 01 (E-commerce) provides excellent foundation with clear success criteria and validates our testing approach (RSpec for units, Rust E2E for workflows).
