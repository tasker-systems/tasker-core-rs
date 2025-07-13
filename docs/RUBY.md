# Ruby FFI Integration Plan

This document outlines the comprehensive plan for creating Ruby bindings for the Tasker Core Rust engine using the Magnus crate and Ruby gem packaging.

## Overview

The Ruby integration strategy involves creating a separate **`tasker-core-rb`** gem that provides high-performance Ruby bindings for the `tasker-core-rs` orchestration engine. This approach enables the Rails Tasker engine to leverage Rust's performance for dependency resolution and state management while maintaining Ruby's flexibility for business logic.

### Performance Targets
- **10-100x faster** dependency resolution vs PostgreSQL functions
- **<1ms FFI overhead** per orchestration call
- **>10k events/sec** cross-language event processing
- **<10% penalty** vs native Ruby execution for delegation

## Architecture Design

### Integration Pattern
```
Rails Engine (Business Logic) ↔ tasker-core-rb (FFI Bridge) ↔ tasker-core-rs (Performance Core)
```

**Delegation-based Architecture:**
1. Rails handles task queuing and business logic
2. Rust performs high-performance orchestration decisions
3. Rails executes individual steps via framework handlers
4. Rust finalizes tasks and manages state transitions

### Core Components to Expose

Based on the Rust `src/client` foundation, the Ruby bindings will expose:

**1. Handler Foundation (mirroring src/client/)**
- `TaskerCore::BaseTaskHandler` - Task orchestration foundation
- `TaskerCore::BaseStepHandler` - Step execution with process/process_results hooks
- `TaskerCore::TaskContext` - Task-level execution context
- `TaskerCore::StepContext` - Step-level execution context
- `TaskerCore::RubyStepHandler` - Ruby trait equivalent for step handlers
- `TaskerCore::RubyTaskHandler` - Ruby trait equivalent for task handlers

**2. Orchestration Core**
- `TaskerCore::WorkflowCoordinator` - Main orchestration engine
- `TaskerCore::TaskResult` - Task completion states (Complete/Error/Reenqueue)
- `TaskerCore::StepResult` - Individual step execution results
- `TaskerCore::ViableStep` - Ready-to-execute steps from discovery

**3. Event Publishing System**
- `TaskerCore::EventPublisher` - Subscribe and publish lifecycle events
- `TaskerCore::EventSubscriber` - Ruby event subscriber interface
- Support for task/step lifecycle events with JSON context

**4. Analytics & Insights Models**
- `TaskerCore::Insights::AnalyticsMetrics` - System-wide performance metrics
- `TaskerCore::Insights::SystemHealthCounts` - Real-time health monitoring
- `TaskerCore::Insights::SlowestSteps` - Performance bottleneck identification
- `TaskerCore::Insights::SlowestTasks` - Task-level performance analysis

**5. Developer-Friendly State Management**
- Read-only state inspection and history traversal
- Safe state manipulation methods (reset attempts, mark pending, update results)
- Template and runtime dependency graph access
- State history and audit trail inspection

**Rust FFI Layer Implementation:**
- Ruby wrappers for each core component
- Async runtime management for blocking on futures
- Type conversion utilities (Rust ↔ Ruby)
- Error translation layer (OrchestrationError → Ruby exceptions)
- Event channel bridging for pub/sub

### Safe State Management API Design

Instead of exposing raw state machines, we provide developer-friendly operations:

#### Read-Only State Inspection
```ruby
# Task state access
task.current_state              # "in_progress", "complete", "failed", etc.
task.state_history             # Array of state transitions with timestamps
task.created_at                # When task was created
task.updated_at                # Last state change

# Step state access  
step.current_state             # Current step state
step.attempts                  # Number of attempts made
step.retry_limit              # Maximum retry attempts allowed
step.last_failure_at          # Timestamp of last failure
step.results                  # Current step results/output
```

#### Safe State Manipulation
```ruby
# Reset step for retry (validates current state allows reset)
step.reset_for_retry!(new_retry_limit: 10)

# Mark step back to pending (validates preconditions)
step.mark_pending!

# Update step results (validates step is in appropriate state)
step.update_results!(new_data)

# Mark step as manually completed (with validation)
step.mark_completed!(final_results)
```

#### Template and Runtime Dependency Graph Access
```ruby
# Template-level analysis (similar to Rails TemplateGraphAnalyzer)
template_analyzer = task.template_graph_analyzer
analysis = template_analyzer.analyze

puts "Has cycles: #{analysis[:cycles].any?}"
puts "Root steps: #{analysis[:roots]}"
puts "Execution levels: #{analysis[:levels]}"
puts "Topology: #{analysis[:topology]}"

# Template dependency map
task.template_dependency_graph
# => { "step_a" => [], "step_b" => ["step_a"], "step_c" => ["step_a", "step_b"] }

# Runtime analysis (similar to Rails RuntimeGraphAnalyzer)  
runtime_analyzer = task.runtime_graph_analyzer
runtime_analysis = runtime_analyzer.analyze

# Critical paths and bottlenecks
puts "Longest path: #{runtime_analysis[:critical_paths][:longest_path_length]} steps"
puts "Parallelism opportunities: #{runtime_analysis[:parallelism_opportunities][:current_parallel_opportunities]}"
puts "Error chains: #{runtime_analysis[:error_chains][:total_error_steps]}"

# Runtime dependency graph with actual step IDs
task.runtime_dependency_graph
# => { 123 => [], 124 => [123], 125 => [123, 124] }

# Individual step dependency checking
step.dependencies_satisfied?   # Boolean check
step.blocking_dependencies     # Array of unsatisfied dependencies
step.downstream_impact        # Steps that depend on this one
```

#### Audit and History
```ruby
# Complete state transition history
task.state_transitions.each do |transition|
  puts "#{transition.from_state} → #{transition.to_state} at #{transition.created_at}"
  puts "  Reason: #{transition.reason}"
  puts "  Context: #{transition.context}"
end

# Step execution attempts
step.execution_attempts.each do |attempt|
  puts "Attempt #{attempt.attempt_number}: #{attempt.status}"
  puts "  Duration: #{attempt.duration_ms}ms"
  puts "  Output: #{attempt.output}" if attempt.completed?
  puts "  Error: #{attempt.error_message}" if attempt.failed?
end
```

**Guardrails and Validation:**
- All state changes validate current state allows the transition
- Dependency modifications require appropriate permissions
- State history is immutable (append-only)
- All operations are logged for audit trail
- Concurrent modification protection

## Project Structure (Monorepo Approach)

### Updated Repository Layout
```
tasker-core-rs/                    # Root monorepo
├── Cargo.toml                     # Workspace configuration
├── .github/workflows/             # Multi-project CI/CD
│   ├── rust-core.yml             # Core Rust testing
│   ├── ruby-gem.yml              # Ruby gem build/test
│   └── release.yml               # Coordinated releases
├── src/                          # Main Rust core
├── tests/                        # Core Rust tests
├── bindings/                     # Language bindings
│   └── ruby/                     # tasker-core-rb gem
│       ├── Cargo.toml            # Ruby binding Rust config
│       ├── tasker-core-rb.gemspec # Gem specification
│       ├── ext/tasker_core/extconf.rb # Ruby extension build
│       ├── lib/
│       │   ├── tasker_core.rb    # Main Ruby interface
│       │   └── tasker_core/
│       │       ├── version.rb
│       │       └── (other Ruby classes)
│       ├── src/
│       │   └── lib.rs            # Rust FFI implementation
│       ├── spec/                 # RSpec tests
│       ├── Gemfile               # Ruby dependencies
│       └── Rakefile              # Build tasks
├── docs/
│   ├── RUBY.md                   # This document
│   └── ARCHITECTURE.md
└── examples/                     # Cross-language examples
    ├── ruby_rails_integration/
    └── benchmarks/
```

### Benefits of Monorepo Structure
✅ **Coordinated Development**: Changes to Rust core immediately update bindings  
✅ **Unified CI/CD**: Test compatibility between core and bindings together  
✅ **Shared Documentation**: Cross-reference between projects easily  
✅ **Version Synchronization**: Keep core and bindings in sync  
✅ **Code Reuse**: Share test fixtures, benchmarks, and utilities

## Configuration Files

### Cargo.toml
```toml
[package]
name = "tasker-core-rb"
version = "0.1.0"
edition = "2021"

[lib]
name = "tasker_core_rb"
crate-type = ["cdylib"]

[dependencies]
magnus = "0.7"
tasker-core = { path = "../tasker-core-rs" }
tokio = { version = "1.46", features = ["rt-multi-thread"] }
serde_json = "1.0"
chrono = "0.4"
async-trait = "0.1"
```

### tasker-core-rb.gemspec
```ruby
Gem::Specification.new do |spec|
  spec.name          = "tasker-core-rb"
  spec.version       = "0.1.0"
  spec.authors       = ["Pete Taylor"]
  spec.email         = ["pete.jc.taylor@hey.com"]
  
  spec.summary       = "High-performance Ruby bindings for Tasker workflow orchestration"
  spec.description   = "Ruby FFI bindings for tasker-core-rs, providing 10-100x performance improvements"
  spec.homepage      = "https://github.com/tasker-systems/tasker-core-rb"
  spec.license       = "MIT"
  
  spec.files         = Dir["lib/**/*", "src/**/*", "ext/**/*", "Cargo.toml", "README.md"]
  spec.require_paths = ["lib"]
  spec.extensions    = ["ext/tasker_core/extconf.rb"]
  
  # Magnus dependencies
  spec.add_dependency "rb_sys", "~> 0.9.39"
  spec.add_development_dependency "rake-compiler", "~> 1.2.0"
  spec.add_development_dependency "rspec", "~> 3.0"
end
```

### ext/tasker_core/extconf.rb
```ruby
require "mkmf"
require "rb_sys/mkmf"

create_rust_makefile("tasker_core/tasker_core_rb")
```

## Type Conversion Strategy

### Rust → Ruby Conversions
| Rust Type | Ruby Type | Conversion Method |
|-----------|-----------|-------------------|
| `i64` | `Integer` | Direct conversion |
| `String` | `String` | UTF-8 string conversion |
| `serde_json::Value` | `Hash`/`Array`/etc | Recursive JSON conversion |
| `chrono::DateTime<Utc>` | `Time` | UTC time conversion |
| `Duration` | `Float` | Convert to seconds |
| `Vec<T>` | `Array` | Element-wise conversion |
| `HashMap<String, V>` | `Hash` | Key-value conversion |

### Ruby → Rust Conversions
| Ruby Type | Rust Type | Validation |
|-----------|-----------|------------|
| `Integer` | `i64` | Range checking |
| `String` | `String` | UTF-8 validation |
| `Hash` | `HashMap<String, serde_json::Value>` | Key type validation |
| `Array` | `Vec<serde_json::Value>` | Element conversion |
| `Time` | `chrono::DateTime<Utc>` | Timezone conversion |

## Error Handling Strategy

### Error Translation Layer
```rust
// Rust errors → Ruby exceptions
match rust_error {
    OrchestrationError::Database(_) => TaskerCore::DatabaseError,
    OrchestrationError::StateTransition(_) => TaskerCore::StateTransitionError,
    OrchestrationError::Validation(_) => TaskerCore::ValidationError,
    _ => TaskerCore::OrchestrationError,
}
```

### Ruby Exception Hierarchy
```ruby
module TaskerCore
  class Error < StandardError; end
  class OrchestrationError < Error; end
  class DatabaseError < Error; end
  class StateTransitionError < Error; end
  class ValidationError < Error; end
  class TimeoutError < Error; end
end
```

### Safe Error Propagation
- Use `rb_protect` for Ruby API calls
- Convert Rust `Result<T, E>` to Ruby values or exceptions
- Maintain error context and stack traces
- Graceful degradation on FFI failures

## Memory Management

### Ruby GC Integration
- **Stack-only Ruby objects** (magnus requirement)
- Use `Value` wrapper for Ruby object lifetime management
- Register complex objects with GC when necessary
- Avoid heap allocation of Ruby objects

### Async Runtime Management
- Single `tokio::Runtime` instance per coordinator
- `Arc<Runtime>` for thread-safe sharing
- Proper cleanup on Ruby object finalization
- Block on async operations in FFI layer

## Implementation Roadmap

### Phase 1: Handler Foundation & Basic FFI (Week 1)

**Deliverables:**
- [x] `tasker-core-rb` gem structure created ✅ 
- [x] Magnus FFI initialization working ✅
- [x] Build system configured ✅
- [ ] `BaseStepHandler` and `BaseTaskHandler` Ruby classes
- [ ] `TaskContext` and `StepContext` with proper serialization
- [ ] Basic async runtime management for handler execution
- [ ] Type conversion for handler inputs/outputs

**Implementation Focus:**
```ruby
# Ruby step handler pattern matching Rust client
class MyStepHandler < TaskerCore::BaseStepHandler
  def process(context)
    # Business logic here
    { status: "processed", data: context.input_data }
  end
  
  def process_results(context, process_output, initial_results = nil)
    # Optional result transformation
    process_output
  end
end
```

**Success Criteria:**
- Ruby handlers can be defined following Rust patterns
- Context objects properly serialize between Ruby/Rust
- Basic step execution working through FFI

### Phase 2: Orchestration & Developer-Friendly State Access (Week 2)

**Deliverables:**
- [ ] `WorkflowCoordinator` with full `execute_task_workflow`
- [ ] Event publishing system with Ruby subscribers
- [ ] Safe state inspection and manipulation methods
- [ ] Dependency graph access (template and runtime)
- [ ] Complete error handling and exception translation

**Implementation Focus:**
```ruby
# Event subscription from Ruby
subscriber = TaskerCore::EventSubscriber.new do |event|
  case event.name
  when "step.completed"
    Rails.logger.info "Step #{event.context['step_id']} completed"
  when "task.failed"
    ErrorNotifier.alert(event.context)
  end
end

publisher = TaskerCore::EventPublisher.new
publisher.subscribe(subscriber)

# Workflow execution
coordinator = TaskerCore::WorkflowCoordinator.new(database_url)
result = coordinator.execute_task_workflow(task_id) do |config|
  config.on_step_execute = ->(step) { MyStepHandler.new.process(step) }
  config.max_parallel_steps = 10
end

# Safe state management operations
task = TaskerCore::Task.find(task_id)
puts "Current state: #{task.current_state}"
puts "State history: #{task.state_history.map(&:state).join(' → ')}"

# Dependency graph inspection
puts "Template dependencies:"
task.template_dependency_graph.each do |step_name, deps|
  puts "  #{step_name} depends on: #{deps.join(', ')}"
end

puts "Runtime dependencies (with actual step IDs):"
task.runtime_dependency_graph.each do |step_id, dep_ids|
  puts "  Step #{step_id} depends on: #{dep_ids.join(', ')}"
end

# Safe step manipulation
step = task.steps.find_by(name: "payment_processing")
if step.failed?
  # Reset for retry with increased limit
  step.reset_for_retry!(new_retry_limit: 5)
  
  # Or mark back to pending with corrected data
  step.update_results!(corrected_payment_data)
  step.mark_pending!
end
```

**Success Criteria:**
- Complete workflow execution through Rust orchestration
- Ruby can subscribe to and receive all lifecycle events
- Developers can safely inspect and manipulate task/step states
- Dependency graphs accessible for debugging and visualization

### Phase 3: Analytics & Production Integration (Week 3)

**Deliverables:**
- [ ] Analytics models exposed (SystemHealth, SlowestSteps, etc.)
- [ ] Rails integration adapter with existing Tasker engine
- [ ] Performance benchmarks validating 10-100x improvement
- [ ] Production monitoring and observability
- [ ] Comprehensive RSpec test suite

**Implementation Focus:**
```ruby
# Analytics access from Ruby
health = TaskerCore::Insights::SystemHealthCounts.current
puts "Tasks in progress: #{health.tasks_in_progress}"
puts "Steps pending: #{health.steps_pending}"
puts "System load: #{health.system_load_percentage}%"

# Performance analysis
slowest_steps = TaskerCore::Insights::SlowestSteps.query do |q|
  q.namespace = "payments"
  q.time_range = 24.hours
  q.limit = 10
end

slowest_steps.each do |step|
  puts "#{step.step_name}: avg #{step.avg_duration_ms}ms (#{step.execution_count} runs)"
end

# Rails integration
class Tasker::PerformanceCoordinator < Tasker::WorkflowCoordinator
  def execute_task_workflow(task_id)
    # Delegate to Rust for performance-critical workflows
    if high_performance_needed?(task_id)
      rust_coordinator.execute_task_workflow(task_id)
    else
      super # Use existing Rails implementation
    end
  end
  
  private
  
  def rust_coordinator
    @rust_coordinator ||= TaskerCore::WorkflowCoordinator.new(
      ActiveRecord::Base.connection_db_config.url
    )
  end
end
```

**Success Criteria:**
- Analytics models provide real-time insights from Ruby
- Seamless integration with existing Rails Tasker engine
- Performance improvements validated in production workloads
- Zero disruption migration path demonstrated

## Cross-Platform Build Strategy

### Development Workflow
```bash
# Build the extension
rake compile

# Run tests
bundle exec rspec

# Package gem
gem build tasker-core-rb.gemspec

# Install locally for testing
gem install tasker-core-rb-0.1.0.gem
```

### GitHub Actions CI/CD
```yaml
strategy:
  matrix:
    os: [ubuntu-latest, macos-latest, windows-latest]
    ruby: ['3.0', '3.1', '3.2', '3.3']

steps:
- uses: actions/checkout@v4
- uses: ruby/setup-ruby@v1
  with:
    ruby-version: ${{ matrix.ruby }}
    bundler-cache: true
- uses: actions-rs/toolchain@v1
  with:
    toolchain: stable
- run: bundle exec rake compile
- run: bundle exec rspec
```

### Distribution Strategy
- **Development**: Local gem builds and path dependencies
- **Testing**: Pre-release gems to test environments
- **Production**: Published gems to RubyGems.org
- **Cross-compilation**: Use `rake-compiler-dock` for platform gems

## Testing Strategy

### Test Coverage Areas
1. **FFI Integration Tests**
   - Ruby ↔ Rust type conversions
   - Error handling and exception translation
   - Memory safety and GC integration

2. **Orchestration Tests**
   - Basic workflow execution
   - Complex dependency resolution (Diamond, Parallel, Tree patterns)
   - State transition management
   - Event publishing and handling

3. **Performance Tests**
   - Dependency resolution performance vs PostgreSQL functions
   - FFI overhead measurements
   - Memory usage and leak detection
   - Concurrent execution stress tests

4. **Integration Tests**
   - Rails Tasker engine integration
   - Database operations and transactions
   - Error recovery and rollback scenarios

### Test Structure
```ruby
# spec/tasker_core_spec.rb
RSpec.describe TaskerCore::WorkflowCoordinator do
  let(:coordinator) { TaskerCore::WorkflowCoordinator.new(database_url) }
  let(:rails_adapter) { TaskerCore::RailsAdapter.new }

  describe '#execute_task_workflow' do
    it 'executes simple linear workflow' do
      result = coordinator.execute_task_workflow(task_id, rails_adapter)
      expect(result['status']).to eq('complete')
    end

    it 'handles complex dependency resolution' do
      # Test diamond pattern workflow
    end
  end
end
```

## Performance Monitoring

### Key Metrics
- **FFI Call Latency**: Time spent in Rust ↔ Ruby boundary
- **Dependency Resolution Time**: Performance vs existing SQL functions
- **Memory Usage**: Heap allocation and GC pressure
- **Throughput**: Tasks and steps processed per second
- **Error Rates**: FFI failures and recovery statistics

### Monitoring Integration
```ruby
# Performance instrumentation
module TaskerCore::Instrumentation
  def self.time_ffi_call(operation)
    start_time = Time.now
    result = yield
    duration = Time.now - start_time
    
    # Send metrics to monitoring system
    Metrics.timer('tasker_core.ffi_call_duration', duration, tags: { operation: operation })
    
    result
  end
end
```

## Production Considerations

### Thread Safety
- `WorkflowCoordinator` is `Send + Sync`
- Use `Arc<>` for shared state across Ruby threads
- Async runtime compatibility with Rails thread pools
- Proper synchronization for database connections

### Deployment
- Gem installation in production environments
- Native extension compilation requirements
- Database migration compatibility
- Feature flag rollout strategy

### Monitoring and Observability
- FFI performance metrics
- Error rate tracking
- Memory leak detection
- Performance regression alerts

## Migration Strategy

### Gradual Rollout
1. **Phase 1**: Optional Rust orchestration behind feature flag
2. **Phase 2**: A/B testing between Ruby and Rust implementations
3. **Phase 3**: Default to Rust with Ruby fallback
4. **Phase 4**: Remove Ruby implementation (if desired)

### Compatibility
- Maintain backward compatibility with existing Rails handlers
- Identical API surface for seamless switching
- Data format compatibility between implementations
- Rollback capability at any migration step

### Feature Flags
```ruby
# Rails configuration
config.tasker.use_rust_orchestration = ENV.fetch('USE_RUST_ORCHESTRATION', 'false') == 'true'

# Conditional usage
coordinator = if Rails.configuration.tasker.use_rust_orchestration
                TaskerCore::WorkflowCoordinator.new(database_url)
              else
                Tasker::WorkflowCoordinator.new
              end
```

## Success Criteria

### Technical Goals
- ✅ **Working Rails Integration**: Seamless integration with existing Tasker engine
- ✅ **Performance Targets Met**: 10-100x improvement in dependency resolution
- ✅ **Memory Safe**: No memory leaks or GC integration issues
- ✅ **Error Handling**: Proper Ruby exception translation and recovery
- ✅ **Cross-Platform**: Working builds on Linux, macOS, and Windows

### Quality Metrics
- **Test Coverage**: >95% for FFI layer and integration points
- **Performance**: Dependency resolution <10ms vs >100ms current
- **Reliability**: <0.1% error rate in production
- **Maintainability**: Clear separation of concerns and documentation

### Business Impact
- **Improved User Experience**: Faster workflow execution
- **Reduced Infrastructure Costs**: Lower database load and CPU usage
- **Enhanced Scalability**: Support for higher task throughput
- **Developer Productivity**: Maintained Ruby flexibility with Rust performance

---

This comprehensive plan provides a systematic approach to implementing high-performance Ruby bindings for the Tasker Core Rust engine, enabling the Rails Tasker engine to leverage Rust's performance while maintaining Ruby's flexibility and ease of development.