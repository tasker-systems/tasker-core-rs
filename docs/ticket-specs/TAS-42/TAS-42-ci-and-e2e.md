# TAS-42 CI and E2E Testing - Consolidated Strategy

## Executive Summary

This specification establishes a unified E2E testing architecture for tasker-core that eliminates test infrastructure duplication and focuses on API contract validation. We consolidate all end-to-end tests into a single Rust-based framework (`tests/e2e/`) while refocusing Ruby specs on framework-specific concerns (FFI, types, events, registry).

**Core Insight**: We're testing API contracts, not implementation languages. Whether a handler is written in Ruby or Rust is irrelevant to E2E tests - both interact with the same orchestration API.

**Key Changes**:
- Migrate Ruby integration tests from RSpec to Rust e2e framework
- Create `tests/e2e/ruby/` for Ruby worker-specific e2e scenarios
- Refocus `workers/ruby/spec/` on Ruby framework unit tests
- Use `DockerIntegrationManager` for all e2e testing
- Single HTTP client (`tasker-client`) for all API interactions
- Eliminate duplicate test infrastructure (Faraday vs tasker-client)

**Benefits**:
- Single source of truth for API contract tests
- Reduced debugging surface (fix endpoint issues once)
- Clearer test boundaries (e2e vs framework vs unit)
- Language-agnostic testing (supports future Python worker)
- Faster CI feedback (parallel execution, less duplication)

---

## Current State Analysis

### ✅ What Works Well

**Docker Infrastructure** (`docker/`)
- Complete multi-service setup: PostgreSQL + PGMQ, Orchestration, Rust Worker, Ruby Worker
- Service-specific Dockerfiles with optimized multi-stage builds
- Health checks and dependency management via docker-compose
- Ruby-worker.test.Dockerfile successfully compiles Ruby FFI extensions

**Rust E2E Tests** (`tests/e2e/rust/`)
- 6 workflow pattern tests: linear, diamond, tree, mixed_dag, order_fulfillment, simple_integration
- `DockerIntegrationManager` helper for service setup and validation
- Tests assume Docker Compose services are running (simple, reliable)
- Uses typed `tasker-client` crate for API interactions

**Ruby Integration Tests** (`workers/ruby/spec/integration/`)
- 5 workflow tests matching Rust patterns (linear, diamond, tree, mixed_dag, order_fulfillment)
- `RubyWorkerIntegrationManager` with HTTP clients for orchestration and worker APIs
- Health checks, task completion polling, comprehensive verification
- Successfully validates Ruby worker processes tasks correctly

**Test Handlers**
- Error scenario handlers in `workers/ruby/spec/handlers/examples/error_scenarios/`
- Templates in `workers/ruby/spec/fixtures/templates/`
- Demonstrates permanent errors, retryable errors, and success scenarios

### 🔍 The Duplication Problem

**Duplicate E2E Test Harnesses**:
1. **Rust**: `DockerIntegrationManager` with `tasker-client` crate
2. **Ruby**: `RubyWorkerIntegrationManager` with Faraday HTTP client

Both test the same thing (orchestration API responses) but:
- Different HTTP clients lead to different endpoint issues
- Different assertion styles require double debugging
- Maintenance burden: fix API changes in two places
- Unclear which is source of truth

**Debugging Test Infrastructure Instead of Business Logic**:
- Recent issues: 404 errors from non-existent `/v1/tasks/{uuid}/status` endpoint
- API endpoint inconsistencies between Faraday and tasker-client
- Field name mismatches (`step_uuid` vs `workflow_step_uuid`)
- Error constructor argument differences

**Blurred Test Boundaries**:
- Ruby integration tests mixing e2e validation with framework concerns
- Unclear what should be tested in RSpec vs Rust
- Framework-level tests (FFI, types, events) not explicitly separated

---

## Architecture Decision: Unified E2E Testing

### Rationale

**Testing API Contracts, Not Implementation Languages**

E2E tests verify that:
1. Orchestration API accepts task requests
2. Workers claim and execute steps
3. Results are processed correctly
4. Task/step state transitions are valid
5. Error handling works as specified

None of these require knowing the handler implementation language. The test calls HTTP endpoints and verifies responses - that's the entire contract.

**Analogy: Microservice Testing**

You don't write microservice e2e tests in the same language as each service:
- Payment service (Java) ← tested via HTTP API
- Inventory service (Go) ← tested via HTTP API
- Notification service (Python) ← tested via HTTP API

Similarly, tasker-core workers are language-independent from the API perspective:
- Rust worker handlers ← tested via orchestration API
- Ruby worker handlers ← tested via orchestration API
- Python worker handlers (future) ← tested via orchestration API

### Target Architecture

```
tests/e2e/
├── rust/                          # Rust worker-specific e2e tests
│   ├── linear_workflow.rs
│   ├── diamond_workflow.rs
│   └── ...
├── ruby/                          # Ruby worker-specific e2e tests
│   ├── error_scenarios_test.rs    # NEW - migrated from RSpec
│   ├── linear_workflow_test.rs    # NEW - migrated from RSpec
│   ├── blog_ecommerce_test.rs     # NEW - blog example
│   └── ...
└── python/                        # Future: Python worker tests
    └── data_pipeline_test.rs

workers/ruby/spec/
├── ffi/                           # FFI layer tests (framework)
│   ├── bootstrap_spec.rb
│   └── function_calls_spec.rb
├── types/                         # Type wrapper tests (framework)
│   ├── task_wrapper_spec.rb
│   └── step_wrapper_spec.rb
├── events/                        # Event system tests (framework)
│   └── event_system_spec.rb
├── registry/                      # Handler registry tests (framework)
│   └── handler_registry_spec.rb
├── config/                        # Configuration tests (framework)
│   └── config_loading_spec.rb
└── errors/                        # Error class tests (framework)
    └── error_classes_spec.rb
```

**Test Responsibilities**:

| Test Type | Location | Language | Purpose | Requires Docker |
|-----------|----------|----------|---------|----------------|
| E2E Tests | `tests/e2e/rust/` | Rust | Rust worker API contract | Yes |
| E2E Tests | `tests/e2e/ruby/` | Rust | Ruby worker API contract | Yes |
| Framework Tests | `workers/ruby/spec/` | Ruby | Ruby FFI/types/events | No |
| Unit Tests | `src/` | Rust | Rust implementation details | No |

### Benefits of Consolidation

**1. Single Source of Truth**
- One HTTP client: `tasker-client` crate
- One test manager: `DockerIntegrationManager`
- One assertion style: Rust `assert!()` and test framework

**2. Reduced Debugging Surface**
- API endpoint changes: update once in `tasker-client`
- Response structure changes: update once in e2e tests
- Docker configuration: single `docker-compose.test.yml`

**3. Clear Test Boundaries**
- **E2E**: Black-box API testing (no language assumptions)
- **Framework**: Ruby-specific implementation testing
- **Unit**: Component isolation testing

**4. Language-Agnostic Extension**
- Adding Python worker: create `tests/e2e/python/`
- No new test infrastructure needed
- Same patterns, same tools

**5. Faster CI Feedback**
- Parallel test execution via `cargo nextest`
- Framework tests run independently (no Docker startup)
- Clear separation: fast framework tests, comprehensive e2e tests

---

## Phase 1: Audit Current State

**Objective**: Document what exists, identify overlaps, and plan migration

### 1.1 Catalog Existing Test Coverage

**Task**: Create comprehensive inventory of all tests

**Rust E2E Tests** (`tests/e2e/rust/`):
```bash
# List all Rust e2e tests
find tests/e2e/rust -name "*.rs" -exec echo {} \;

# For each test file, document:
# - Test name and purpose
# - Workflow pattern tested
# - Namespaces used
# - Expected handlers
# - Approximate runtime
```

**Ruby Integration Tests** (`workers/ruby/spec/integration/`):
```bash
# List all Ruby integration tests
find workers/ruby/spec/integration -name "*_spec.rb" -exec echo {} \;

# For each test file, document:
# - Test scenarios covered
# - Overlap with Rust tests
# - Ruby-specific assertions
# - Dependencies on Ruby test helpers
```

**Output**: `docs/testing-audit.md` with complete test inventory

### 1.2 Identify Test Handler Requirements

**Task**: Document all test handlers and their usage

**Handler Inventory**:
```bash
# List all test handlers
find workers/ruby/spec/handlers -name "*_handler.rb"

# For each handler, document:
# - Namespace and name
# - Purpose (success, permanent error, retryable error, etc.)
# - Which tests use it
# - Template configuration
```

**Template Inventory**:
```bash
# List all test templates
find workers/ruby/spec/fixtures/templates -name "*.yaml"

# For each template, verify:
# - Correct schema (steps: not step_templates:)
# - Handler callables match file names
# - Retry configuration is sensible
# - Test environment overrides
```

**Output**: `docs/test-handler-inventory.md` with handler catalog

### 1.3 Analyze Test Infrastructure

**Task**: Document capabilities and gaps in test managers

**DockerIntegrationManager Analysis** (`src/test_helpers/integration_test_manager.rs`):
- Methods available
- Setup and teardown behavior
- Health check strategy
- Environment variable configuration
- Client capabilities (orchestration, worker)

**RubyWorkerIntegrationManager Analysis** (`workers/ruby/spec/integration/test_helpers/ruby_integration_manager.rb`):
- Methods available
- Overlap with DockerIntegrationManager
- Ruby-specific features
- API endpoint assumptions

**Gap Analysis**:
- Missing features in DockerIntegrationManager needed for Ruby tests
- Features in RubyWorkerIntegrationManager that should be ported
- Helper methods needed for common test scenarios

**Output**: Feature parity checklist in `docs/test-manager-comparison.md`

### 1.4 Validation

**Success Criteria**:
- [ ] Complete test inventory created
- [ ] All test handlers documented
- [ ] Test infrastructure gaps identified
- [ ] Migration scope clearly defined

---

## Phase 2: Consolidate E2E Test Infrastructure

**Objective**: Migrate Ruby integration tests to Rust e2e framework in `tests/e2e/ruby/`

### 2.1 Enhance DockerIntegrationManager

**Task**: Add missing features needed for Ruby worker testing

**Enhancements Needed**:

1. **Worker-specific health checks**
   ```rust
   // src/test_helpers/integration_test_manager.rs

   impl DockerIntegrationManager {
       /// Setup with Ruby worker health validation
       pub async fn setup_with_ruby_worker() -> Result<Self> {
           let manager = Self::setup().await?;
           manager.verify_ruby_worker_ready().await?;
           Ok(manager)
       }

       async fn verify_ruby_worker_ready(&self) -> Result<()> {
           let worker_health = self.worker_client.health_check().await?;
           ensure!(worker_health.status == "healthy", "Ruby worker not healthy");

           // Verify handler discovery
           let handlers = self.worker_client.list_handlers().await?;
           ensure!(!handlers.is_empty(), "No handlers discovered");

           Ok(())
       }
   }
   ```

2. **Test helper methods**
   ```rust
   /// Wait for task to fail (for error scenario tests)
   pub async fn wait_for_task_failure(
       client: &OrchestrationClient,
       task_uuid: &Uuid,
       timeout_secs: u64,
   ) -> Result<Task> {
       let start = Instant::now();
       loop {
           let task = client.get_task(task_uuid).await?;

           if task.execution_status == ExecutionStatus::Error {
               return Ok(task);
           }

           if start.elapsed().as_secs() > timeout_secs {
               bail!("Task did not fail within {timeout_secs}s");
           }

           tokio::time::sleep(Duration::from_secs(1)).await;
       }
   }
   ```

3. **Environment configuration helpers**
   ```rust
   /// Create task request with test-specific context
   pub fn create_task_request_with_bypass(
       namespace: &str,
       name: &str,
       context: serde_json::Value,
       bypass_steps: Vec<String>,
   ) -> TaskRequest {
       TaskRequest {
           namespace: namespace.to_string(),
           name: name.to_string(),
           version: "1.0.0".to_string(),
           context,
           bypass_steps,
           // ... other fields
       }
   }
   ```

**Files to Modify**:
- `src/test_helpers/integration_test_manager.rs` - Add helper methods
- `src/test_helpers/mod.rs` - Export new helpers

### 2.2 Configure Test Handler Discovery

**Task**: Ensure Ruby worker discovers test handlers in Docker environment

**Docker Compose Configuration** (`docker/docker-compose.test.yml`):
```yaml
services:
  ruby-worker:
    build:
      context: .
      dockerfile: docker/build/ruby-worker.test.Dockerfile
    volumes:
      # Mount test handlers so they're discoverable at runtime
      - ./workers/ruby/spec:/app/workers/ruby/spec:ro
    environment:
      # Template discovery path (note: TASKER_TEMPLATE_PATH not TASK_TEMPLATE_PATH)
      - TASKER_TEMPLATE_PATH=/app/workers/ruby/spec/fixtures/templates
      - TASKER_ENV=test
      - RUST_LOG=info
      # Ensure Ruby can find handler classes
      - RUBYLIB=/app/workers/ruby/spec/handlers
    depends_on:
      postgres:
        condition: service_healthy
      orchestration:
        condition: service_started
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8082/health"]
      interval: 5s
      timeout: 10s
      retries: 20
      start_period: 30s
```

**Ruby Worker Bootstrap** (`workers/ruby/lib/tasker_core/bootstrap.rb`):
```ruby
module TaskerCore
  module Bootstrap
    class << self
      def discover_handlers!
        template_path = ENV['TASKER_TEMPLATE_PATH'] || default_template_path

        unless Dir.exist?(template_path)
          Logger.instance.log_system(:error, 'template_path_not_found',
            path: template_path,
            message: 'Template path does not exist')
          return
        end

        # Load all YAML templates
        templates = Dir[File.join(template_path, '**', '*.yaml')]
        Logger.instance.log_system(:info, 'discovering_templates',
          count: templates.size,
          path: template_path)

        templates.each do |template_file|
          register_template(template_file)
        end

        # Load handler Ruby files
        handler_base = File.join(File.dirname(template_path), 'handlers')
        if Dir.exist?(handler_base)
          handlers = Dir[File.join(handler_base, '**', '*_handler.rb')]
          Logger.instance.log_system(:info, 'loading_handlers',
            count: handlers.size,
            base: handler_base)

          handlers.each { |f| require f }
        end
      end

      private

      def default_template_path
        if ENV['TASKER_ENV'] == 'test'
          File.join(Dir.pwd, 'spec', 'fixtures', 'templates')
        else
          File.join(Dir.pwd, 'config', 'tasker', 'templates')
        end
      end
    end
  end
end
```

### 2.3 Migrate Ruby Integration Tests

**Task**: Port each Ruby integration test to Rust e2e test

**Migration Pattern**:

**Before** (`workers/ruby/spec/integration/linear_workflow_docker_integration_spec.rb`):
```ruby
RSpec.describe 'Linear Workflow - Docker Integration' do
  let(:manager) { RubyWorkerIntegrationManager.setup }

  it 'completes linear workflow with Ruby handlers' do
    task_request = create_task_request(
      'test_workflows',
      'linear_workflow',
      { test_data: 'linear_test' }
    )

    response = manager.orchestration_client.create_task(task_request)
    task_uuid = response[:task_uuid]

    expect(task_uuid).to be_present

    # Wait for completion
    task = wait_for_task_completion(manager, task_uuid, 30)

    # Verify completion
    expect(task[:status]).to eq('complete')
    expect(task[:execution_status]).to eq('complete')
    expect(task[:total_steps]).to eq(3)
    expect(task[:completed_steps]).to eq(3)
  end
end
```

**After** (`tests/e2e/ruby/linear_workflow_test.rs`):
```rust
use tasker_core::test_helpers::{DockerIntegrationManager, wait_for_task_completion};
use serde_json::json;

#[tokio::test]
async fn test_linear_workflow_with_ruby_handlers() -> anyhow::Result<()> {
    // Setup with Ruby worker health validation
    let manager = DockerIntegrationManager::setup().await?;

    // Create task request
    let task_request = manager.create_task_request(
        "test_workflows",
        "linear_workflow",
        json!({ "test_data": "linear_test" }),
    );

    // Submit task
    let response = manager.orchestration_client
        .create_task(task_request)
        .await?;

    // Wait for completion
    wait_for_task_completion(
        &manager.orchestration_client,
        &response.task_uuid,
        30,
    ).await?;

    // Verify completion
    let task = manager.orchestration_client
        .get_task(&response.task_uuid)
        .await?;

    assert_eq!(task.status, TaskStatus::Complete);
    assert_eq!(task.execution_status, ExecutionStatus::Complete);
    assert_eq!(task.total_steps, 3);
    assert_eq!(task.completed_steps, 3);

    Ok(())
}
```

**Migration Checklist per Test**:
- [ ] Identify Ruby test scenario and purpose
- [ ] Create equivalent Rust test file in `tests/e2e/ruby/`
- [ ] Port test context and request creation
- [ ] Port verification assertions (RSpec → Rust)
- [ ] Run Rust test and verify equivalent behavior
- [ ] Document any differences or limitations
- [ ] Delete Ruby integration test after verification

**Tests to Migrate**:
1. `linear_workflow_docker_integration_spec.rb` → `linear_workflow_test.rs`
2. `diamond_workflow_docker_integration_spec.rb` → `diamond_workflow_test.rs`
3. `tree_workflow_docker_integration_spec.rb` → `tree_workflow_test.rs`
4. `mixed_dag_docker_integration_spec.rb` → `mixed_dag_test.rs`
5. `order_fulfillment_docker_integration_spec.rb` → `order_fulfillment_test.rs`
6. `error_scenarios_docker_integration_spec.rb` → `error_scenarios_test.rs`

### 2.4 Verification

**Success Criteria**:
- [ ] All Ruby integration tests have Rust e2e equivalents
- [ ] Rust e2e tests pass consistently (3+ consecutive runs)
- [ ] Test execution time is equivalent or faster
- [ ] All test handlers discoverable in Docker environment
- [ ] No loss of test coverage

**Validation Commands**:
```bash
# Run all e2e tests (Rust + Ruby workers)
cargo nextest run --test '*'

# Run only Ruby worker e2e tests
cargo nextest run --test 'ruby_*'

# Verify handler discovery
curl http://localhost:8082/handlers
```

---

## Phase 3: Refocus Ruby Framework Testing

**Objective**: Define and implement Ruby-specific framework tests separate from e2e scenarios

### 3.1 Define Ruby Framework Test Scope

**In Scope for Ruby Specs** (`workers/ruby/spec/`):

**1. FFI Layer** (`spec/ffi/bootstrap_spec.rb`):
```ruby
RSpec.describe 'TaskerCore FFI Bootstrap' do
  it 'calls Rust bootstrap successfully' do
    result = TaskerCore::FFI::Bootstrap.start!(config_hash)

    expect(result).to be_a(TaskerCore::FFI::BootstrapResult)
    expect(result.success?).to be true
  end

  it 'handles bootstrap errors gracefully' do
    invalid_config = { database_url: 'invalid' }

    expect {
      TaskerCore::FFI::Bootstrap.start!(invalid_config)
    }.to raise_error(TaskerCore::Errors::BootstrapError)
  end

  it 'marshals complex configuration to Rust' do
    complex_config = {
      database: { url: 'postgresql://localhost/test', pool_size: 10 },
      event_system: { mode: 'hybrid', fallback_interval_ms: 1000 }
    }

    result = TaskerCore::FFI::Bootstrap.start!(complex_config)
    expect(result.success?).to be true
  end
end
```

**2. Type System** (`spec/types/wrappers_spec.rb`):
```ruby
RSpec.describe 'TaskerCore::Models::WorkflowStepWrapper' do
  let(:step_data) do
    {
      workflow_step_uuid: SecureRandom.uuid,
      name: 'test_step',
      state: 'pending',
      retry_count: 0,
      results: { key: 'value' }
    }
  end

  it 'exposes workflow_step_uuid field' do
    step = TaskerCore::Models::WorkflowStepWrapper.new(step_data)

    expect(step.workflow_step_uuid).to match(/^[0-9a-f-]{36}$/)
  end

  it 'provides access to nested results hash' do
    step = TaskerCore::Models::WorkflowStepWrapper.new(step_data)

    expect(step.results).to be_a(Hash)
    expect(step.results[:key]).to eq('value')
  end

  it 'handles nil results gracefully' do
    step = TaskerCore::Models::WorkflowStepWrapper.new(
      step_data.merge(results: nil)
    )

    expect(step.results).to be_nil
  end
end
```

**3. Event System** (`spec/events/event_system_spec.rb`):
```ruby
RSpec.describe 'TaskerCore Event System' do
  it 'fires step_execution_started event' do
    received_events = []

    TaskerCore::Events.subscribe('step.execution.started') do |event|
      received_events << event
    end

    # Trigger event
    TaskerCore::Events.publish('step.execution.started', {
      step_uuid: SecureRandom.uuid,
      step_name: 'test_step'
    })

    expect(received_events.size).to eq(1)
    expect(received_events.first[:step_name]).to eq('test_step')
  end

  it 'supports multiple subscribers' do
    subscriber1_called = false
    subscriber2_called = false

    TaskerCore::Events.subscribe('task.created') { subscriber1_called = true }
    TaskerCore::Events.subscribe('task.created') { subscriber2_called = true }

    TaskerCore::Events.publish('task.created', {})

    expect(subscriber1_called).to be true
    expect(subscriber2_called).to be true
  end
end
```

**4. Registry System** (`spec/registry/handler_registry_spec.rb`):
```ruby
RSpec.describe 'TaskerCore::Registry::HandlerRegistry' do
  around do |example|
    # Set template path for test
    ENV['TASKER_TEMPLATE_PATH'] = 'spec/fixtures/templates'
    example.run
    ENV.delete('TASKER_TEMPLATE_PATH')
  end

  it 'discovers handlers from template path' do
    registry = TaskerCore::Registry::HandlerRegistry.instance
    registry.discover_handlers!

    expect(registry.handlers).to include('ErrorScenarios::SuccessHandler')
    expect(registry.handlers).to include('ErrorScenarios::PermanentErrorHandler')
  end

  it 'resolves handler callable to class' do
    registry = TaskerCore::Registry::HandlerRegistry.instance

    handler_class = registry.resolve('ErrorScenarios::SuccessHandler')

    expect(handler_class).to eq(ErrorScenarios::SuccessHandler)
    expect(handler_class.ancestors).to include(TaskerCore::StepHandler::Base)
  end

  it 'raises error for unknown handler' do
    registry = TaskerCore::Registry::HandlerRegistry.instance

    expect {
      registry.resolve('NonExistent::Handler')
    }.to raise_error(TaskerCore::Errors::HandlerNotFoundError)
  end
end
```

**5. Configuration** (`spec/config/loading_spec.rb`):
```ruby
RSpec.describe 'TaskerCore Configuration Loading' do
  it 'loads test environment configuration' do
    ENV['TASKER_ENV'] = 'test'

    config = TaskerCore::Config.load

    expect(config.environment).to eq('test')
    expect(config.event_system.mode).to eq('polling_only')
  end

  it 'validates required configuration keys' do
    ENV['TASKER_ENV'] = 'test'
    # Unset DATABASE_URL to trigger validation error
    ENV.delete('DATABASE_URL')

    expect {
      TaskerCore::Config.load
    }.to raise_error(TaskerCore::Errors::ConfigurationError)
  end
end
```

**6. Error Handling** (`spec/errors/error_classes_spec.rb`):
```ruby
RSpec.describe 'TaskerCore Error Classes' do
  describe 'PermanentError' do
    it 'accepts message and context' do
      error = TaskerCore::Errors::PermanentError.new(
        'Payment failed',
        context: { error_code: 'INVALID_CARD', card_type: 'amex' }
      )

      expect(error.message).to eq('Payment failed')
      expect(error.context[:error_code]).to eq('INVALID_CARD')
    end

    it 'rejects invalid keyword arguments' do
      expect {
        TaskerCore::Errors::PermanentError.new(
          'Message',
          error_code: 'CODE'  # Invalid - should be in context:
        )
      }.to raise_error(ArgumentError)
    end
  end

  describe 'RetryableError' do
    it 'accepts retry_after parameter' do
      error = TaskerCore::Errors::RetryableError.new(
        'Service timeout',
        retry_after: 30,
        context: { service: 'payment_gateway' }
      )

      expect(error.retry_after).to eq(30)
      expect(error.context[:service]).to eq('payment_gateway')
    end
  end
end
```

**Out of Scope for Ruby Specs** (moved to Rust e2e):
- Task creation and completion workflows
- Step execution through workers
- Retry behavior and exponential backoff
- State machine transitions
- API endpoint responses
- Distributed system coordination
- Docker service integration

### 3.2 Restructure Ruby Spec Directory

**Task**: Reorganize `workers/ruby/spec/` to reflect framework-only scope

**New Structure**:
```
workers/ruby/spec/
├── spec_helper.rb                    # RSpec configuration
├── ffi/
│   ├── bootstrap_spec.rb            # FFI bootstrap testing
│   └── function_calls_spec.rb       # FFI function call testing
├── types/
│   ├── task_wrapper_spec.rb         # Task wrapper field access
│   └── step_wrapper_spec.rb         # Step wrapper field access
├── events/
│   └── event_system_spec.rb         # dry-events integration
├── registry/
│   └── handler_registry_spec.rb     # Handler discovery and resolution
├── config/
│   └── loading_spec.rb              # Configuration loading and validation
├── errors/
│   └── error_classes_spec.rb        # Error class constructors
├── handlers/                         # Test handler implementations
│   └── examples/
│       ├── error_scenarios/          # Used by e2e tests
│       └── blog/                     # Used by blog e2e tests
├── fixtures/
│   └── templates/                    # YAML templates for handlers
└── support/
    └── framework_helpers.rb          # Helpers for framework tests
```

**Migration Tasks**:
1. Delete `spec/integration/` directory (moved to `tests/e2e/ruby/`)
2. Delete `spec/integration/test_helpers/ruby_integration_manager.rb` (no longer needed)
3. Create new framework test directories (`ffi/`, `types/`, `events/`, etc.)
4. Write framework tests as specified in 3.1
5. Update `spec_helper.rb` to remove Docker-related configuration

### 3.3 Create Framework Test Helpers

**Task**: Build helpers for framework testing without Docker

**Framework Helpers** (`spec/support/framework_helpers.rb`):
```ruby
module FrameworkTestHelpers
  # Create mock step data for wrapper testing
  def build_mock_step_data(overrides = {})
    {
      workflow_step_uuid: SecureRandom.uuid,
      name: 'test_step',
      state: 'pending',
      retry_count: 0,
      results: nil,
      error_info: nil,
      created_at: Time.now.utc.iso8601,
      updated_at: Time.now.utc.iso8601
    }.merge(overrides)
  end

  # Create mock task data for wrapper testing
  def build_mock_task_data(overrides = {})
    {
      task_uuid: SecureRandom.uuid,
      name: 'test_task',
      namespace: 'test_namespace',
      status: 'pending',
      context: {},
      created_at: Time.now.utc.iso8601
    }.merge(overrides)
  end

  # Stub FFI bootstrap for isolated testing
  def stub_ffi_bootstrap(result: :success)
    allow(TaskerCore::FFI::Bootstrap).to receive(:start!).and_return(
      case result
      when :success
        double(success?: true, error: nil)
      when :failure
        double(success?: false, error: 'Bootstrap failed')
      end
    )
  end

  # Reset handler registry between tests
  def reset_handler_registry
    TaskerCore::Registry::HandlerRegistry.instance.clear!
  end
end

RSpec.configure do |config|
  config.include FrameworkTestHelpers

  config.before(:each) do
    # Reset global state between framework tests
    reset_handler_registry if respond_to?(:reset_handler_registry)
  end
end
```

### 3.4 Validation

**Success Criteria**:
- [ ] Ruby specs clearly focused on framework concerns
- [ ] No Docker dependencies in Ruby specs
- [ ] Fast test execution (< 30 seconds for all framework tests)
- [ ] Clear separation from e2e tests
- [ ] Helper methods simplify framework testing

**Validation Commands**:
```bash
# Run framework tests (no Docker needed)
cd workers/ruby
bundle exec rspec spec/

# Should complete in < 30 seconds
time bundle exec rspec spec/
```

---

## Phase 4: Error Scenario Testing

**Objective**: Create comprehensive error handling tests using Rust e2e framework with Ruby handlers

### 4.1 Error Scenario Test Handlers

**Task**: Ensure error handlers are complete and properly configured

**Handler Inventory** (`workers/ruby/spec/handlers/examples/error_scenarios/`):

1. **SuccessHandler** - Always succeeds immediately
2. **PermanentErrorHandler** - Raises PermanentError, no retries
3. **RetryableErrorHandler** - Raises RetryableError, exhausts retries

**Template Configuration** (`workers/ruby/spec/fixtures/templates/error_testing_handler.yaml`):
```yaml
---
name: error_testing
namespace_name: test_errors
version: 1.0.0
description: "Error scenario testing for core failure patterns"
metadata:
  author: "Test Infrastructure"
  tags:
    - namespace:test_errors
    - pattern:error_scenarios
    - implementation:ruby_ffi
task_handler:
  callable: ErrorScenarios::ErrorTestingHandler
  initialization:
    error_tracking_enabled: true

steps:
  # Happy Path - Always succeeds
  - name: success_step
    description: "Always succeeds - validates happy path execution"
    handler:
      callable: ErrorScenarios::SuccessHandler
      initialization:
        scenario: success
    retry:
      retryable: false
      limit: 0
      backoff: exponential
      backoff_base_ms: 100
      max_backoff_ms: 1000
    timeout_seconds: 5

  # Permanent Failure - No retries
  - name: permanent_error_step
    description: "Permanent error that should not be retried"
    handler:
      callable: ErrorScenarios::PermanentErrorHandler
      initialization:
        error_type: permanent
    retry:
      retryable: false
      limit: 0
      backoff: exponential
      backoff_base_ms: 100
      max_backoff_ms: 1000
    timeout_seconds: 5

  # Retryable Failure - Retries then fails
  - name: retryable_error_step
    description: "Retryable error that exhausts retry limit"
    handler:
      callable: ErrorScenarios::RetryableErrorHandler
      initialization:
        error_type: retryable
    retry:
      retryable: true
      limit: 2
      backoff: exponential
      backoff_base_ms: 100
      max_backoff_ms: 500
    timeout_seconds: 5

# Test environment overrides for fast execution
environments:
  test:
    steps:
      - name: success_step
        retry:
          backoff_base_ms: 50
          max_backoff_ms: 200
      - name: retryable_error_step
        retry:
          limit: 2
          backoff_base_ms: 50
          max_backoff_ms: 200
```

**Validation**:
```bash
# Verify template schema
cargo run --bin config-validator

# Verify handlers are discoverable
curl http://localhost:8082/handlers | jq '.handlers[] | select(.namespace == "test_errors")'
```

### 4.2 Create Error Scenario E2E Tests

**Task**: Write Rust e2e tests for error scenarios

**Test File** (`tests/e2e/ruby/error_scenarios_test.rs`):
```rust
use tasker_core::test_helpers::{
    DockerIntegrationManager,
    wait_for_task_completion,
    wait_for_task_failure,
};
use serde_json::json;
use anyhow::Result;

/// Test happy path execution with success handler
#[tokio::test]
async fn test_success_scenario() -> Result<()> {
    let manager = DockerIntegrationManager::setup().await?;

    // Create task with only success_step (bypass error steps)
    let task_request = manager.create_task_request_with_bypass(
        "test_errors",
        "error_testing",
        json!({ "scenario_type": "success" }),
        vec!["permanent_error_step".into(), "retryable_error_step".into()],
    );

    let response = manager.orchestration_client
        .create_task(task_request)
        .await?;

    // Should complete in < 3 seconds (fast execution)
    wait_for_task_completion(
        &manager.orchestration_client,
        &response.task_uuid,
        3,
    ).await?;

    // Verify completion
    let task = manager.orchestration_client
        .get_task(&response.task_uuid)
        .await?;

    assert_eq!(task.status, TaskStatus::Complete);
    assert_eq!(task.execution_status, ExecutionStatus::Complete);
    assert_eq!(task.total_steps, 1);
    assert_eq!(task.completed_steps, 1);
    assert_eq!(task.failed_steps, 0);

    println!("✅ Success scenario completed in < 3 seconds");
    Ok(())
}

/// Test permanent failure with no retries
#[tokio::test]
async fn test_permanent_failure_scenario() -> Result<()> {
    let manager = DockerIntegrationManager::setup().await?;

    // Create task with only permanent_error_step
    let task_request = manager.create_task_request_with_bypass(
        "test_errors",
        "error_testing",
        json!({ "scenario_type": "permanent_failure" }),
        vec!["success_step".into(), "retryable_error_step".into()],
    );

    let response = manager.orchestration_client
        .create_task(task_request)
        .await?;

    let start_time = std::time::Instant::now();

    // Should fail quickly (no retries)
    wait_for_task_failure(
        &manager.orchestration_client,
        &response.task_uuid,
        3,
    ).await?;

    let elapsed = start_time.elapsed();

    // Verify fast failure (< 2s, no retry delays)
    assert!(elapsed.as_secs() < 2, "Should fail quickly without retries");

    // Verify failure state
    let task = manager.orchestration_client
        .get_task(&response.task_uuid)
        .await?;

    assert!(matches!(
        task.execution_status,
        ExecutionStatus::Error | ExecutionStatus::Blocked
    ));
    assert_eq!(task.total_steps, 1);
    assert_eq!(task.completed_steps, 0);
    assert_eq!(task.failed_steps, 1);

    // Verify step is in error state (not waiting for retry)
    let steps = manager.orchestration_client
        .list_task_steps(&response.task_uuid)
        .await?;

    let error_step = steps.iter()
        .find(|s| s.name == "permanent_error_step")
        .expect("Should find permanent_error_step");

    assert_eq!(error_step.state, StepState::Error);
    assert!(!error_step.is_retryable);

    println!("✅ Permanent failure completed in {:?} (no retries)", elapsed);
    Ok(())
}

/// Test retryable failure with backoff exhaustion
#[tokio::test]
async fn test_retryable_failure_scenario() -> Result<()> {
    let manager = DockerIntegrationManager::setup().await?;

    // Create task with only retryable_error_step
    let task_request = manager.create_task_request_with_bypass(
        "test_errors",
        "error_testing",
        json!({ "scenario_type": "retryable_failure" }),
        vec!["success_step".into(), "permanent_error_step".into()],
    );

    let response = manager.orchestration_client
        .create_task(task_request)
        .await?;

    let start_time = std::time::Instant::now();

    // Should fail after retries (2 retries @ 50ms base)
    wait_for_task_failure(
        &manager.orchestration_client,
        &response.task_uuid,
        5,
    ).await?;

    let elapsed = start_time.elapsed();

    // Verify took time for retries but not too long
    assert!(elapsed.as_millis() > 100, "Should have retry delays");
    assert!(elapsed.as_secs() < 3, "Should not take too long");

    // Verify failure state after retry exhaustion
    let task = manager.orchestration_client
        .get_task(&response.task_uuid)
        .await?;

    assert!(matches!(
        task.execution_status,
        ExecutionStatus::Error | ExecutionStatus::Blocked
    ));
    assert_eq!(task.total_steps, 1);
    assert_eq!(task.completed_steps, 0);
    assert_eq!(task.failed_steps, 1);

    // Verify step exhausted retries
    let steps = manager.orchestration_client
        .list_task_steps(&response.task_uuid)
        .await?;

    let error_step = steps.iter()
        .find(|s| s.name == "retryable_error_step")
        .expect("Should find retryable_error_step");

    assert_eq!(error_step.state, StepState::Error);
    assert!(error_step.is_retryable);
    assert!(error_step.retry_count >= 2, "Should have retried at least twice");

    println!("✅ Retryable failure completed in {:?} (with {} retries)",
        elapsed, error_step.retry_count);
    Ok(())
}

/// Test mixed workflow with both success and failure steps
#[tokio::test]
async fn test_mixed_workflow_scenario() -> Result<()> {
    let manager = DockerIntegrationManager::setup().await?;

    // Run all steps (no bypass)
    let task_request = manager.create_task_request(
        "test_errors",
        "error_testing",
        json!({ "scenario_type": "mixed" }),
    );

    let response = manager.orchestration_client
        .create_task(task_request)
        .await?;

    // Should fail overall (error steps will fail)
    wait_for_task_failure(
        &manager.orchestration_client,
        &response.task_uuid,
        5,
    ).await?;

    // Verify mixed results
    let task = manager.orchestration_client
        .get_task(&response.task_uuid)
        .await?;

    assert_eq!(task.total_steps, 3);
    assert!(task.completed_steps >= 1, "At least success_step should complete");
    assert!(task.failed_steps >= 1, "At least one error step should fail");

    println!("✅ Mixed workflow: {} succeeded, {} failed",
        task.completed_steps, task.failed_steps);
    Ok(())
}
```

### 4.3 Validation

**Success Criteria**:
- [ ] All error scenario tests pass consistently
- [ ] Success scenario completes in < 3s
- [ ] Permanent failure completes in < 2s (no retry delays)
- [ ] Retryable failure shows retry behavior (>100ms, <3s)
- [ ] Mixed workflow handles partial success/failure

**Run Tests**:
```bash
# Run only error scenario tests
cargo nextest run --test 'ruby_error_scenarios_test'

# Run with detailed output
cargo nextest run --test 'ruby_error_scenarios_test' --no-capture
```

---

## Phase 5: Blog Example Testing

**Objective**: Port rich blog examples from Rails engine to Rust e2e framework

### 5.1 Select Blog Examples

**Available Examples** (from `tasker-engine`):

| Post | Workflow | Complexity | Priority |
|------|----------|-----------|----------|
| Post 01: E-commerce | Order processing (linear) | Medium | **HIGH** |
| Post 02: Data Pipeline | ETL pipeline (diamond) | High | **HIGH** |
| Post 03: Microservices | Service coordination (mixed DAG) | Medium | MEDIUM |
| Post 04: Team Scaling | Multi-namespace workflows | Low | LOW |
| Post 05: Observability | Event-driven monitoring | Medium | MEDIUM |

**Phase 5 Scope**: Port Post 01 (E-commerce) and Post 02 (Data Pipeline)

### 5.2 Port E-commerce Example

**Task**: Migrate e-commerce order processing workflow

**Source Structure** (Rails engine):
```
tasker-engine/spec/blog/fixtures/post_01_ecommerce_reliability/
├── config/order_processing_handler.yaml
├── handlers/order_processing_handler.rb
├── step_handlers/
│   ├── validate_cart_handler.rb
│   ├── process_payment_handler.rb
│   ├── update_inventory_handler.rb
│   ├── create_order_handler.rb
│   └── send_confirmation_handler.rb
└── mock_services/
    ├── payment_service.rb
    ├── email_service.rb
    └── inventory_service.rb
```

**Target Structure** (tasker-core):
```
workers/ruby/spec/handlers/examples/blog/post_01_ecommerce/
├── task_handler.rb
├── step_handlers/
│   ├── validate_cart_handler.rb      # Ported from Rails
│   ├── process_payment_handler.rb    # Ported with error patterns
│   ├── update_inventory_handler.rb   # Ported from Rails
│   ├── create_order_handler.rb       # Ported from Rails
│   └── send_confirmation_handler.rb  # Ported from Rails
└── mock_services/
    ├── payment_service.rb             # Adapted for standalone Ruby
    ├── email_service.rb               # Adapted for standalone Ruby
    └── inventory_service.rb           # Adapted for standalone Ruby

workers/ruby/spec/fixtures/templates/blog/
└── order_processing_handler.yaml      # Ported from Rails config

tests/e2e/ruby/blog/
└── ecommerce_order_processing_test.rs # NEW - E2E test
```

**Adaptation Requirements**:

1. **Remove Rails Dependencies**:
   ```ruby
   # Before (Rails engine)
   class ProcessPaymentHandler < Tasker::StepHandler::Base
     def call(task, sequence, step)
       Rails.logger.info "Processing payment..."
       # ...
     end
   end

   # After (tasker-core)
   class ProcessPaymentHandler < TaskerCore::StepHandler::Base
     def call(task, sequence, step)
       TaskerCore::Logger.instance.log_step(:info, 'processing_payment',
         step_uuid: step.workflow_step_uuid,
         message: 'Processing payment...')
       # ...
     end
   end
   ```

2. **Update Error Classes**:
   ```ruby
   # Before
   raise Tasker::RetryableError.new("Timeout", error_code: 'TIMEOUT')

   # After
   raise TaskerCore::Errors::RetryableError.new(
     "Timeout",
     context: { error_code: 'TIMEOUT' }
   )
   ```

3. **Adapt Mock Services**:
   ```ruby
   # Before (Rails)
   class MockPaymentService
     include Singleton

     def process_payment(payment_info)
       # Uses ActiveSupport, Rails.cache, etc.
     end
   end

   # After (Standalone Ruby)
   class MockPaymentService
     @instance = new

     class << self
       attr_reader :instance
     end

     def process_payment(payment_info)
       # Pure Ruby implementation, no ActiveSupport
       # Use instance variables for state tracking
     end
   end
   ```

**E2E Test** (`tests/e2e/ruby/blog/ecommerce_order_processing_test.rs`):
```rust
use tasker_core::test_helpers::{DockerIntegrationManager, wait_for_task_completion};
use serde_json::json;
use anyhow::Result;

#[tokio::test]
async fn test_ecommerce_order_processing_happy_path() -> Result<()> {
    let manager = DockerIntegrationManager::setup().await?;

    // Create order processing task with e-commerce context
    let task_request = manager.create_task_request(
        "ecommerce",
        "process_order",
        json!({
            "cart_items": [
                {
                    "product_id": 1,
                    "quantity": 2,
                    "price": 29.99,
                    "name": "Widget"
                }
            ],
            "payment_info": {
                "method": "credit_card",
                "amount": 59.98,
                "currency": "USD"
            },
            "customer_info": {
                "id": 12345,
                "email": "test@example.com",
                "name": "Test Customer"
            }
        }),
    );

    let response = manager.orchestration_client
        .create_task(task_request)
        .await?;

    // Wait for workflow completion
    wait_for_task_completion(
        &manager.orchestration_client,
        &response.task_uuid,
        30,
    ).await?;

    // Verify all steps completed successfully
    let task = manager.orchestration_client
        .get_task(&response.task_uuid)
        .await?;

    assert_eq!(task.status, TaskStatus::Complete);
    assert_eq!(task.total_steps, 5);
    assert_eq!(task.completed_steps, 5);

    // Verify specific steps
    let steps = manager.orchestration_client
        .list_task_steps(&response.task_uuid)
        .await?;

    let step_names: Vec<&str> = steps.iter()
        .map(|s| s.name.as_str())
        .collect();

    assert!(step_names.contains(&"validate_cart"));
    assert!(step_names.contains(&"process_payment"));
    assert!(step_names.contains(&"update_inventory"));
    assert!(step_names.contains(&"create_order"));
    assert!(step_names.contains(&"send_confirmation"));

    Ok(())
}

#[tokio::test]
async fn test_ecommerce_payment_failure_permanent() -> Result<()> {
    let manager = DockerIntegrationManager::setup().await?;

    // Create task with invalid payment method (triggers permanent error)
    let task_request = manager.create_task_request(
        "ecommerce",
        "process_order",
        json!({
            "payment_info": {
                "method": "invalid_card_type",  // Will trigger permanent error
                "amount": 59.98
            },
            "cart_items": [{ "product_id": 1, "quantity": 1, "price": 59.98 }],
            "customer_info": { "id": 1, "email": "test@example.com" }
        }),
    );

    let response = manager.orchestration_client
        .create_task(task_request)
        .await?;

    // Wait for failure
    wait_for_task_failure(
        &manager.orchestration_client,
        &response.task_uuid,
        30,
    ).await?;

    // Verify permanent failure (no retries)
    let steps = manager.orchestration_client
        .list_task_steps(&response.task_uuid)
        .await?;

    let payment_step = steps.iter()
        .find(|s| s.name == "process_payment")
        .expect("Should find payment step");

    assert_eq!(payment_step.state, StepState::Error);
    assert_eq!(payment_step.retry_count, 0); // No retries for permanent error

    Ok(())
}

#[tokio::test]
async fn test_ecommerce_payment_retry_then_succeed() -> Result<()> {
    let manager = DockerIntegrationManager::setup().await?;

    // Create task that will fail payment twice then succeed
    let task_request = manager.create_task_request(
        "ecommerce",
        "process_order",
        json!({
            "payment_info": {
                "method": "flaky_gateway",  // Will fail twice then succeed
                "amount": 59.98
            },
            "cart_items": [{ "product_id": 1, "quantity": 1, "price": 59.98 }],
            "customer_info": { "id": 1, "email": "test@example.com" }
        }),
    );

    let response = manager.orchestration_client
        .create_task(task_request)
        .await?;

    // Wait for eventual success (after retries)
    wait_for_task_completion(
        &manager.orchestration_client,
        &response.task_uuid,
        60, // Allow time for retries
    ).await?;

    // Verify successful completion after retries
    let task = manager.orchestration_client
        .get_task(&response.task_uuid)
        .await?;

    assert_eq!(task.status, TaskStatus::Complete);

    // Verify payment step retried
    let steps = manager.orchestration_client
        .list_task_steps(&response.task_uuid)
        .await?;

    let payment_step = steps.iter()
        .find(|s| s.name == "process_payment")
        .expect("Should find payment step");

    assert_eq!(payment_step.state, StepState::Complete);
    assert!(payment_step.retry_count >= 2, "Should have retried at least twice");

    Ok(())
}
```

### 5.3 Port Data Pipeline Example

**Task**: Migrate ETL pipeline with diamond DAG pattern

**Workflow Structure**:
```
       extract_users
      /              \
     /                \
extract_orders    extract_products
     \                /
      \              /
       transform_metrics
             |
       generate_insights
             |
       update_dashboard
```

**Key Features**:
- Parallel extraction steps (extract_users, extract_orders, extract_products)
- Dependency convergence (transform_metrics waits for all extractions)
- Rate limiting and connection timeout error scenarios

**Test File** (`tests/e2e/ruby/blog/data_pipeline_test.rs`):
```rust
#[tokio::test]
async fn test_data_pipeline_diamond_dag() -> Result<()> {
    let manager = DockerIntegrationManager::setup().await?;

    let task_request = manager.create_task_request(
        "data_pipeline",
        "etl_workflow",
        json!({
            "data_sources": {
                "users_db": "postgresql://localhost/users",
                "orders_db": "postgresql://localhost/orders",
                "products_db": "postgresql://localhost/products"
            },
            "target_warehouse": "postgresql://localhost/warehouse"
        }),
    );

    let response = manager.orchestration_client
        .create_task(task_request)
        .await?;

    wait_for_task_completion(
        &manager.orchestration_client,
        &response.task_uuid,
        60,
    ).await?;

    // Verify diamond DAG executed correctly
    let steps = manager.orchestration_client
        .list_task_steps(&response.task_uuid)
        .await?;

    // All extract steps should complete
    let extract_steps: Vec<_> = steps.iter()
        .filter(|s| s.name.starts_with("extract_"))
        .collect();

    assert_eq!(extract_steps.len(), 3);
    assert!(extract_steps.iter().all(|s| s.state == StepState::Complete));

    // Transform should wait for all extracts
    let transform_step = steps.iter()
        .find(|s| s.name == "transform_metrics")
        .expect("Should find transform step");

    assert_eq!(transform_step.state, StepState::Complete);

    Ok(())
}

#[tokio::test]
async fn test_data_pipeline_partial_extract_failure() -> Result<()> {
    let manager = DockerIntegrationManager::setup().await?;

    // Configure one extract to fail permanently
    let task_request = manager.create_task_request(
        "data_pipeline",
        "etl_workflow",
        json!({
            "data_sources": {
                "users_db": "invalid_connection_string",  // Will fail
                "orders_db": "postgresql://localhost/orders",
                "products_db": "postgresql://localhost/products"
            }
        }),
    );

    let response = manager.orchestration_client
        .create_task(task_request)
        .await?;

    // Wait for failure
    wait_for_task_failure(
        &manager.orchestration_client,
        &response.task_uuid,
        30,
    ).await?;

    let task = manager.orchestration_client
        .get_task(&response.task_uuid)
        .await?;

    // Verify partial completion
    assert!(task.completed_steps >= 2, "At least 2 extracts should succeed");
    assert_eq!(task.failed_steps, 1, "1 extract should fail");

    // Transform should not execute (blocked by failed dependency)
    let steps = manager.orchestration_client
        .list_task_steps(&response.task_uuid)
        .await?;

    let transform_step = steps.iter()
        .find(|s| s.name == "transform_metrics");

    assert!(
        transform_step.is_none() || transform_step.unwrap().state == StepState::Pending,
        "Transform should not execute with failed dependency"
    );

    Ok(())
}
```

### 5.4 Validation

**Success Criteria**:
- [ ] E-commerce workflow completes successfully
- [ ] E-commerce handles payment failures (permanent and retryable)
- [ ] Data pipeline executes diamond DAG correctly
- [ ] Data pipeline handles partial extraction failures
- [ ] All blog tests pass consistently

**Run Tests**:
```bash
# Run all blog example tests
cargo nextest run --test 'ruby_blog_*'

# Run specific blog test
cargo nextest run --test 'ruby_blog_ecommerce_order_processing_test'
```

---

## Phase 6: CI Pipeline Integration

**Objective**: Consolidate CI workflows for unified testing

### 6.1 Consolidated CI Workflow

**File**: `.github/workflows/e2e-tests.yml`

```yaml
name: E2E Tests (All Workers)

on:
  pull_request:
    paths:
      - 'src/**'
      - 'tests/**'
      - 'workers/**'
      - 'docker/**'
      - '.github/workflows/e2e-tests.yml'
  push:
    branches: [main]
  workflow_dispatch:

jobs:
  e2e-tests:
    name: E2E Tests (Docker)
    runs-on: ubuntu-22.04
    timeout-minutes: 15

    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - name: Install cargo-nextest
        uses: taiki-e/install-action@v2
        with:
          tool: nextest

      - name: Start Docker services
        run: |
          docker compose -f docker/docker-compose.test.yml up -d --build

      - name: Wait for orchestration service
        run: |
          timeout 90s bash -c 'until curl -f http://localhost:8080/health; do
            echo "Waiting for orchestration...";
            sleep 2;
          done'

      - name: Wait for Ruby worker service
        run: |
          timeout 120s bash -c 'until curl -f http://localhost:8082/health; do
            echo "Waiting for Ruby worker (FFI bootstrap may take time)...";
            sleep 3;
          done'

      - name: Verify handler discovery
        run: |
          echo "Checking orchestration handlers..."
          curl -f http://localhost:8080/handlers | jq '.handlers | length'

          echo "Checking Ruby worker handlers..."
          curl -f http://localhost:8082/handlers | jq '.handlers | length'

      - name: Run all e2e tests
        run: |
          cargo nextest run \
            --test '*' \
            --profile ci \
            --no-fail-fast
        env:
          TASKER_TEST_ORCHESTRATION_URL: http://localhost:8080
          TASKER_TEST_WORKER_URL: http://localhost:8082
          TASKER_TEST_SKIP_HEALTH_CHECK: "true"  # We already checked
          RUST_LOG: info

      - name: Upload test results
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: e2e-test-results
          path: target/nextest/ci/junit.xml

      - name: View Docker logs on failure
        if: failure()
        run: |
          echo "=== Orchestration Logs ==="
          docker compose -f docker/docker-compose.test.yml logs orchestration

          echo "=== Ruby Worker Logs ==="
          docker compose -f docker/docker-compose.test.yml logs ruby-worker

      - name: Shutdown services
        if: always()
        run: docker compose -f docker/docker-compose.test.yml down -v

  ruby-framework-tests:
    name: Ruby Framework Tests (No Docker)
    runs-on: ubuntu-22.04
    timeout-minutes: 10

    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Setup Ruby
        uses: ruby/setup-ruby@v1
        with:
          ruby-version: '3.2'
          bundler-cache: true
          working-directory: workers/ruby

      - name: Setup Rust (for Ruby extension)
        uses: dtolnay/rust-toolchain@stable

      - name: Install Ruby dependencies
        working-directory: workers/ruby
        run: bundle install

      - name: Compile Ruby FFI extension
        working-directory: workers/ruby
        run: bundle exec rake compile

      - name: Run Ruby framework tests
        working-directory: workers/ruby
        run: |
          bundle exec rspec spec/ \
            --format documentation \
            --format RspecJunitFormatter \
            --out ../../target/ruby-framework-results.xml
        env:
          TASKER_ENV: test
          TASKER_TEMPLATE_PATH: ./spec/fixtures/templates

      - name: Upload Ruby test results
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: ruby-framework-test-results
          path: target/ruby-framework-results.xml

  test-summary:
    name: Test Summary
    runs-on: ubuntu-22.04
    needs: [e2e-tests, ruby-framework-tests]
    if: always()

    steps:
      - name: Download e2e test results
        uses: actions/download-artifact@v4
        with:
          name: e2e-test-results
          path: results/

      - name: Download Ruby framework test results
        uses: actions/download-artifact@v4
        with:
          name: ruby-framework-test-results
          path: results/

      - name: Publish test results
        uses: EnricoMi/publish-unit-test-result-action@v2
        with:
          files: results/**/*.xml
          check_name: Test Results Summary
```

### 6.2 Remove Duplicate CI Workflows

**Task**: Delete old Ruby-specific integration test workflow

**Files to Delete**:
- `.github/workflows/test-ruby-integration.yml` (replaced by e2e-tests.yml)

**Files to Update**:
- `.github/workflows/test.yml` - Remove Ruby integration test references

### 6.3 Validation

**Success Criteria**:
- [ ] CI runs both e2e and framework tests
- [ ] E2E tests complete in < 10 minutes
- [ ] Framework tests complete in < 2 minutes
- [ ] Test results published to GitHub
- [ ] Clear separation between test types in CI output

---

## Phase 7: Documentation and Validation

**Objective**: Document new testing strategy and validate all tests

### 7.1 Update Documentation

**Files to Create/Update**:

**1. E2E Testing Guide** (`docs/testing-e2e.md`):
```markdown
# E2E Testing Guide

## Overview

All end-to-end tests are written in Rust and located in `tests/e2e/`. Tests verify API contracts regardless of worker implementation language.

## Test Structure

- `tests/e2e/rust/` - Rust worker-specific scenarios
- `tests/e2e/ruby/` - Ruby worker-specific scenarios
- Future: `tests/e2e/python/` - Python worker scenarios

## Running Tests

### Prerequisites
```bash
# Start Docker services
docker compose -f docker/docker-compose.test.yml up -d --build

# Wait for services to be healthy
curl http://localhost:8080/health
curl http://localhost:8082/health
```

### Run All E2E Tests
```bash
cargo nextest run --test '*'
```

### Run Worker-Specific Tests
```bash
# Rust worker tests only
cargo nextest run --test 'rust_*'

# Ruby worker tests only
cargo nextest run --test 'ruby_*'
```

## Writing New E2E Tests

See [E2E Test Template](./e2e-test-template.md) for guidance.
```

**2. Ruby Framework Testing Guide** (`workers/ruby/docs/framework-testing.md`):
```markdown
# Ruby Framework Testing Guide

## Overview

Ruby specs in `workers/ruby/spec/` focus on Ruby-specific framework concerns:
- FFI layer integration
- Type wrappers and conversions
- Event system (dry-events)
- Handler registry and discovery
- Configuration loading
- Error class behavior

## Test Scope

### In Scope
- Does Ruby correctly call Rust FFI functions?
- Do type wrappers expose correct fields?
- Does handler discovery work?
- Do error classes construct correctly?

### Out of Scope (see E2E tests)
- Task execution workflows
- Distributed system coordination
- API endpoint responses
- Retry and backoff behavior

## Running Tests

```bash
cd workers/ruby
bundle exec rspec spec/
```

Tests run without Docker and complete in < 30 seconds.
```

**3. Testing Strategy Overview** (`docs/testing-strategy.md`):
```markdown
# Testing Strategy

## Test Pyramid

```
        /\
       /  \      E2E Tests (Rust)
      /----\     - API contract validation
     /  /\  \    - Multi-service integration
    /  /  \  \
   /  /____\  \  Framework Tests (Ruby/Rust)
  /  /      \  \ - Language-specific logic
 /__/________\__\
                 Unit Tests (Rust)
                 - Component isolation
```

## Test Responsibilities

| Test Type | Location | Purpose | Docker Required |
|-----------|----------|---------|----------------|
| E2E | `tests/e2e/` | Black-box API testing | Yes |
| Framework | `workers/ruby/spec/` | Ruby framework testing | No |
| Unit | `src/` | Rust component testing | No |

## Key Principles

1. **Language-Agnostic E2E**: Tests call APIs, not implementations
2. **Single Test Harness**: All e2e tests use `DockerIntegrationManager`
3. **Clear Boundaries**: E2E vs framework vs unit tests are distinct
4. **Fast Feedback**: Framework tests run without Docker
```

### 7.2 Create Migration Checklist

**File**: `docs/e2e-migration-checklist.md`

```markdown
# E2E Migration Checklist

## Phase 1: Audit ✅
- [ ] Test inventory created
- [ ] Handler inventory documented
- [ ] Infrastructure gaps identified

## Phase 2: Consolidation ✅
- [ ] DockerIntegrationManager enhanced
- [ ] Handler discovery configured
- [ ] All Ruby integration tests migrated to Rust e2e
- [ ] Old Ruby integration tests deleted

## Phase 3: Framework Testing ✅
- [ ] Ruby spec scope defined
- [ ] Framework test structure created
- [ ] FFI tests written
- [ ] Type wrapper tests written
- [ ] Event system tests written
- [ ] Registry tests written
- [ ] Configuration tests written
- [ ] Error class tests written

## Phase 4: Error Scenarios ✅
- [ ] Error handlers complete
- [ ] Templates configured correctly
- [ ] Success scenario test passes
- [ ] Permanent failure test passes
- [ ] Retryable failure test passes
- [ ] Mixed workflow test passes

## Phase 5: Blog Examples ✅
- [ ] E-commerce handlers ported
- [ ] E-commerce e2e tests written
- [ ] Data pipeline handlers ported
- [ ] Data pipeline e2e tests written
- [ ] All blog tests pass

## Phase 6: CI Integration ✅
- [ ] E2E CI workflow created
- [ ] Framework CI workflow created
- [ ] Old CI workflows removed
- [ ] Test results published

## Phase 7: Documentation ✅
- [ ] E2E testing guide written
- [ ] Framework testing guide written
- [ ] Testing strategy documented
- [ ] Migration checklist complete

## Final Validation ✅
- [ ] All tests pass in CI (3+ consecutive runs)
- [ ] Test execution time acceptable (< 15 min total)
- [ ] No flaky tests observed
- [ ] Documentation reviewed and approved
```

### 7.3 Final Validation

**Success Criteria**:

**Quantitative**:
- [ ] 100% of old Ruby integration tests migrated
- [ ] All e2e tests pass (Rust + Ruby workers)
- [ ] All Ruby framework tests pass
- [ ] CI runtime < 15 minutes total
- [ ] 0 flaky tests (3 consecutive CI runs without failures)

**Qualitative**:
- [ ] Clear separation between e2e and framework tests
- [ ] Single test infrastructure (no duplication)
- [ ] Documentation enables new contributors
- [ ] Test failures provide actionable information

**Validation Commands**:
```bash
# Run full test suite
cargo nextest run --all-features
cargo nextest run --test '*'

# Run Ruby framework tests
cd workers/ruby && bundle exec rspec spec/

# Verify no old integration tests remain
find workers/ruby/spec/integration -name "*_spec.rb" | wc -l
# Expected: 0

# Verify e2e tests exist
find tests/e2e/ruby -name "*_test.rs" | wc -l
# Expected: >= 8 (error scenarios + workflows + blog examples)
```

---

## Implementation Timeline

### Week 1: Foundation and Audit
**Days 1-2**: Phase 1 - Audit current state
- Create test inventory
- Document handler requirements
- Analyze infrastructure gaps

**Days 3-5**: Phase 2 - Begin consolidation
- Enhance DockerIntegrationManager
- Configure handler discovery
- Migrate 2 simple workflow tests (linear, diamond)

### Week 2: Consolidation and Framework
**Days 6-8**: Phase 2 - Complete consolidation
- Migrate remaining workflow tests
- Verify all tests pass
- Delete old Ruby integration tests

**Days 9-10**: Phase 3 - Framework testing
- Define scope and structure
- Write FFI and type wrapper tests
- Write event and registry tests

### Week 3: Error Scenarios and Blog Examples
**Days 11-12**: Phase 4 - Error scenarios
- Validate error handlers
- Write error scenario e2e tests
- Verify fast execution (< 3s per test)

**Days 13-15**: Phase 5 - Blog examples (Part 1)
- Port e-commerce example
- Write e2e tests for e-commerce
- Verify payment error scenarios

### Week 4: Blog Examples and CI
**Days 16-18**: Phase 5 - Blog examples (Part 2)
- Port data pipeline example
- Write e2e tests for data pipeline
- Verify diamond DAG execution

**Days 19-20**: Phase 6 - CI integration
- Create consolidated CI workflow
- Remove duplicate workflows
- Verify CI passes

### Week 5: Documentation and Validation
**Days 21-23**: Phase 7 - Documentation
- Write testing guides
- Update strategy documentation
- Create migration checklist

**Days 24-25**: Phase 7 - Final validation
- Run full test suite (3+ times)
- Verify no flaky tests
- Review documentation
- Get approval for merge

---

## Success Metrics

### Quantitative Metrics

**Test Coverage**:
- 100% of Ruby integration tests migrated to Rust e2e
- 6+ Ruby framework test suites (FFI, types, events, registry, config, errors)
- 6+ error scenario tests (success, permanent, retryable, mixed)
- 4+ blog example tests (e-commerce happy, payment errors, data pipeline)
- Total: 20+ new tests covering all scenarios

**Performance**:
- E2E tests: < 10 minutes (all workers)
- Framework tests: < 30 seconds (Ruby only)
- Total CI runtime: < 15 minutes
- Error scenario tests: < 3 seconds each

**Reliability**:
- 0 flaky tests (3 consecutive CI runs)
- 100% pass rate in CI
- No test infrastructure debugging issues

### Qualitative Metrics

**Code Quality**:
- Clear test boundaries (e2e vs framework vs unit)
- Single source of truth for API contract tests
- Consistent test patterns across worker languages
- Comprehensive error handling coverage

**Developer Experience**:
- Easy to add new e2e tests (copy template, modify)
- Fast feedback from framework tests (no Docker)
- Clear documentation enables new contributors
- Actionable CI failure messages

**Maintainability**:
- Single test infrastructure to maintain
- No duplicate test harnesses
- Language-agnostic e2e testing
- Clear migration path for new workers (Python, etc.)

---

## Risks and Mitigations

| Risk | Impact | Probability | Mitigation |
|------|--------|-------------|------------|
| **Handler discovery fails in Docker** | High | Medium | Add explicit volume mounts, verify `TASKER_TEMPLATE_PATH` in entrypoint script |
| **Test migration introduces coverage gaps** | High | Low | Careful 1:1 migration, verification before deletion |
| **Ruby framework tests still need Docker** | Medium | Low | Stub FFI calls, use test doubles for Rust layer |
| **E2E tests become flaky** | High | Medium | Use deterministic test data, avoid timing dependencies |
| **CI runtime too long** | Medium | Low | Parallel test execution, optimize Docker builds |
| **Documentation incomplete** | Low | Medium | Create docs during implementation, not after |
| **Blog examples too complex to port** | Medium | Low | Start with simplest (e-commerce), adapt incrementally |
| **Test infrastructure changes break existing tests** | High | Low | Feature flags for new infrastructure, gradual rollout |

### Specific Risk: Handler Discovery in Docker

**Problem**: Ruby worker in Docker may not find handlers in `workers/ruby/spec/handlers/`

**Mitigation**:
1. Explicit volume mount in docker-compose.test.yml
2. Set `TASKER_TEMPLATE_PATH=/app/workers/ruby/spec/fixtures/templates`
3. Set `RUBYLIB=/app/workers/ruby/spec/handlers` to ensure require paths work
4. Add bootstrap logging to verify handler discovery
5. Create health check that verifies handler count > 0

**Validation**:
```bash
# Check handler discovery after Docker startup
curl http://localhost:8082/handlers | jq '.handlers | length'
# Expected: > 0

# Check specific namespace
curl http://localhost:8082/handlers | jq '.handlers[] | select(.namespace == "test_errors")'
# Expected: 3 handlers (success, permanent_error, retryable_error)
```

---

## Deliverables

### Code Deliverables

1. **Enhanced Test Infrastructure**
   - `src/test_helpers/integration_test_manager.rs` with Ruby worker support
   - `tests/e2e/ruby/` with 10+ comprehensive e2e tests
   - `workers/ruby/spec/` restructured for framework testing only

2. **Consolidated CI Workflows**
   - `.github/workflows/e2e-tests.yml` running all workers
   - Separate framework test job (fast feedback)
   - Test result publishing and reporting

3. **Test Handlers and Templates**
   - Error scenario handlers (3 core patterns)
   - Blog example handlers (e-commerce, data pipeline)
   - Properly configured YAML templates

### Documentation Deliverables

1. **Testing Guides**
   - `docs/testing-e2e.md` - E2E testing guide
   - `workers/ruby/docs/framework-testing.md` - Framework testing guide
   - `docs/testing-strategy.md` - Overall strategy

2. **Migration Documentation**
   - `docs/e2e-migration-checklist.md` - Comprehensive checklist
   - `docs/test-handler-inventory.md` - Handler catalog
   - `docs/testing-audit.md` - Test inventory

3. **Updated Project Documentation**
   - Update `README.md` with new testing approach
   - Update `CLAUDE.md` with testing commands
   - Update contributing guide with testing guidelines

---

## Conclusion

This consolidation strategy eliminates test infrastructure duplication, focuses on API contract validation, and establishes clear boundaries between test types. By migrating all e2e tests to Rust and refocusing Ruby specs on framework concerns, we create a maintainable, language-agnostic testing architecture that scales to future workers (Python, etc.) without additional test harness development.

**Key Outcomes**:
- Single source of truth for e2e tests (`tests/e2e/`)
- Clear test boundaries (e2e vs framework vs unit)
- Faster CI feedback (framework tests without Docker)
- Language-agnostic testing (supports any worker language)
- Reduced maintenance burden (one test infrastructure)

This approach transforms TAS-42 from "Ruby integration works" to "comprehensive, production-ready testing strategy with unified infrastructure and clear separation of concerns."
