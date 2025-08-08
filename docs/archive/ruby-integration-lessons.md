# Ruby Integration Lessons: FFI Design Patterns and Hard-Won Insights

## Executive Summary

This document captures critical lessons learned from implementing Ruby-Rust FFI integration for workflow orchestration systems. These insights represent hard-won knowledge from multiple architectural iterations, failed approaches, and successful patterns that emerged through production experience.

## Core FFI Design Principles

### 1. Minimize FFI Surface Area

**Principle**: The fewer functions crossing the FFI boundary, the simpler the system.

**Anti-Pattern**: Exposing every Rust function to Ruby
```rust
// DON'T: Too many FFI functions
#[magnus::function]
fn get_task_id(handle: i64) -> i64 { /* ... */ }

#[magnus::function] 
fn get_task_status(handle: i64) -> String { /* ... */ }

#[magnus::function]
fn get_task_metadata(handle: i64) -> Value { /* ... */ }
```

**Good Pattern**: Consolidated data access
```rust
// DO: Single function returning rich data
#[magnus::function]
fn get_task_context(task_id: i64) -> HashMap<String, Value> {
    // Return all task data in one call
    build_complete_task_context(task_id)
}
```

**Benefits**:
- Reduced FFI call overhead
- Simpler error handling
- Fewer serialization points
- Less surface area for bugs

### 2. Hash-Based Data Transfer

**Discovery**: Complex object marshaling is a major source of FFI complexity and bugs.

**Failed Approach**: Magnus object registration
```rust
// AVOID: Complex object registration
#[magnus::wrap(class = "TaskerCore::TaskResult")]
pub struct TaskResult {
    task_id: i64,
    status: String,
    step_count: usize,
}

// Problems:
// - Magnus registration complexity
// - Memory management issues
// - Ruby GC interaction problems
// - Debugging difficulties
```

**Successful Approach**: Simple hash transfer with Ruby wrappers
```rust
// SUCCESS: Hash-based transfer
#[magnus::function]
fn initialize_task(request_hash: HashMap<String, Value>) -> HashMap<String, Value> {
    let result = process_task_initialization(request_hash)?;
    
    // Return simple hash
    hashmap! {
        "task_id" => json!(result.task_id),
        "step_count" => json!(result.step_count),
        "status" => json!(result.status.to_string()),
        "errors" => json!(result.errors),
    }
}
```

```ruby
# Ruby wrapper for familiar interface
class InitializeResult
  def initialize(hash)
    @data = hash
  end
  
  def task_id
    @data["task_id"]
  end
  
  def step_count
    @data["step_count"]
  end
  
  def status
    @data["status"]
  end
end

# Usage feels natural to Ruby developers
result = InitializeResult.new(TaskerCore.initialize_task(request.to_h))
puts "Created task #{result.task_id} with #{result.step_count} steps"
```

**Key Benefits**:
- No Magnus object registration required
- Ruby-native error handling
- Simpler debugging (inspect hash contents)
- Backward compatibility with existing Ruby code
- Performance: Hash serialization is faster than object marshaling

### 3. Handle-Based Architecture

**Problem**: Global state management across FFI boundaries creates race conditions and complexity.

**Failed Approach**: Global registries with timeouts
```rust
// AVOID: Global state with complex lifecycle
static ORCHESTRATION_REGISTRY: Lazy<Mutex<HashMap<String, OrchestrationCoordinator>>> = 
    Lazy::new(|| Mutex::new(HashMap::new()));

#[magnus::function]
fn create_coordinator() -> String {
    let id = generate_id();
    let coordinator = OrchestrationCoordinator::new();
    ORCHESTRATION_REGISTRY.lock().unwrap().insert(id.clone(), coordinator);
    id // Return handle, hope it doesn't get lost
}
```

**Successful Approach**: Arc-based persistent handles
```rust
// SUCCESS: Handle-based architecture with Arc
pub struct SharedOrchestrationCoordinator {
    inner: Arc<OrchestrationCoordinator>,
    pool: Arc<PgPool>,
}

impl SharedOrchestrationCoordinator {
    pub fn new(pool: Arc<PgPool>) -> Self {
        Self {
            inner: Arc::new(OrchestrationCoordinator::new()),
            pool,
        }
    }
}

#[magnus::function]
fn create_shared_coordinator(database_url: String) -> SharedOrchestrationCoordinator {
    let pool = Arc::new(create_pool(&database_url).unwrap());
    SharedOrchestrationCoordinator::new(pool)
}

// Handle passed by value, no global state needed
#[magnus::function]
fn orchestrate_task(
    coordinator: &SharedOrchestrationCoordinator,
    task_id: i64
) -> HashMap<String, Value> {
    // Use coordinator directly, no lookup required
    coordinator.inner.orchestrate(task_id, &coordinator.pool)
}
```

**Benefits**:
- No global state management
- Automatic cleanup when Ruby object is GC'd
- Thread-safe sharing via Arc
- No timeout or cleanup complexity

### 4. Error Handling Patterns

**Principle**: Rust errors should become Ruby exceptions, but FFI should never panic.

**Error Translation Strategy**:
```rust
// Rust error types
#[derive(Debug)]
pub enum OrchestrationError {
    DatabaseError(sqlx::Error),
    ConfigurationError(String),
    ValidationError(Vec<String>),
    TaskNotFound(i64),
}

// FFI error handling - never panic
#[magnus::function]
fn safe_orchestrate_task(
    coordinator: &SharedOrchestrationCoordinator,
    task_id: i64
) -> Result<HashMap<String, Value>, magnus::Error> {
    match coordinator.orchestrate_task(task_id) {
        Ok(result) => Ok(result.to_hash()),
        Err(OrchestrationError::TaskNotFound(id)) => {
            Err(magnus::Error::new(
                magnus::exception::arg_error(),
                format!("Task not found: {}", id)
            ))
        },
        Err(OrchestrationError::DatabaseError(e)) => {
            Err(magnus::Error::new(
                magnus::exception::runtime_error(),
                format!("Database error: {}", e)
            ))
        },
        Err(e) => {
            // Log unexpected errors but don't expose internals
            log::error!("Orchestration error: {:?}", e);
            Err(magnus::Error::new(
                magnus::exception::runtime_error(),
                "Internal orchestration error"
            ))
        }
    }
}
```

```ruby
# Ruby error handling
begin
  result = coordinator.orchestrate_task(task_id)
  handle_success(result)
rescue ArgumentError => e
  # Handle validation errors
  handle_not_found(e.message)
rescue RuntimeError => e
  # Handle system errors
  handle_system_error(e.message)
end
```

**Key Patterns**:
- Map Rust errors to appropriate Ruby exception types
- Never expose internal Rust details in error messages
- Log detailed errors in Rust, simple messages to Ruby
- Use Result<T, magnus::Error> for all FFI functions

### 5. Async Runtime Management

**Challenge**: Ruby is synchronous, Rust orchestration is async.

**Solution**: Single runtime with blocking bridges
```rust
use tokio::runtime::Runtime;
use std::sync::OnceLock;

static ASYNC_RUNTIME: OnceLock<Runtime> = OnceLock::new();

fn get_runtime() -> &'static Runtime {
    ASYNC_RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(4)
            .max_blocking_threads(16)
            .thread_name("tasker-orchestration")
            .enable_all()
            .build()
            .expect("Failed to create async runtime")
    })
}

#[magnus::function]
fn orchestrate_task_sync(
    coordinator: &SharedOrchestrationCoordinator,
    task_id: i64
) -> Result<HashMap<String, Value>, magnus::Error> {
    // Block on async operation
    get_runtime().block_on(async {
        coordinator.orchestrate_task_async(task_id).await
    })
    .map_err(|e| magnus::Error::from(e))
}
```

**Benefits**:
- Single runtime shared across all FFI calls
- No runtime creation overhead per call
- Proper async operation within Rust
- Ruby sees synchronous interface

### 6. Configuration Management

**Lesson**: Hardcoded values in Rust create deployment inflexibility.

**Pattern**: Ruby-driven configuration
```rust
// Rust accepts configuration from Ruby
#[derive(Debug, Deserialize)]
pub struct OrchestrationConfig {
    pub max_parallel_steps: usize,
    pub step_timeout_ms: u64,
    pub retry_limits: HashMap<String, u32>,
    pub database_pool_size: u32,
}

#[magnus::function]
fn create_coordinator_with_config(
    config_hash: HashMap<String, Value>
) -> Result<SharedOrchestrationCoordinator, magnus::Error> {
    let config: OrchestrationConfig = serde_json::from_value(
        serde_json::to_value(config_hash)?
    )?;
    
    Ok(SharedOrchestrationCoordinator::new_with_config(config))
}
```

```ruby
# Ruby manages configuration
class TaskerCore::Configuration
  def self.load_from_env
    {
      "max_parallel_steps" => ENV.fetch("TASKER_MAX_PARALLEL_STEPS", "10").to_i,
      "step_timeout_ms" => ENV.fetch("TASKER_STEP_TIMEOUT_MS", "30000").to_i,
      "database_pool_size" => ENV.fetch("TASKER_DB_POOL_SIZE", "20").to_i,
      "retry_limits" => {
        "payment_processing" => 3,
        "notification_delivery" => 5,
        "inventory_check" => 2
      }
    }
  end
end

# Usage
config = TaskerCore::Configuration.load_from_env
coordinator = TaskerCore.create_coordinator_with_config(config)
```

**Benefits**:
- Environment-specific configuration without Rust rebuilds
- Ruby ecosystem configuration patterns (YAML, ENV vars)
- Runtime configuration changes
- Framework integration (Rails.application.config)

## Performance Lessons

### 1. FFI Call Overhead

**Measurement**: Each FFI call has ~1-5μs overhead
**Implication**: Minimize call frequency, maximize data per call

**Anti-Pattern**: Chatty FFI interface
```ruby
# DON'T: Multiple FFI calls per operation
task_id = TaskerCore.get_task_id(handle)           # FFI call 1
status = TaskerCore.get_task_status(handle)        # FFI call 2  
steps = TaskerCore.get_task_steps(handle)          # FFI call 3
metadata = TaskerCore.get_task_metadata(handle)    # FFI call 4
```

**Good Pattern**: Batch data access
```ruby
# DO: Single FFI call with complete data
task_data = TaskerCore.get_complete_task_context(task_id)  # FFI call 1
# All data available in Ruby hash
```

### 2. Serialization Performance

**Discovery**: JSON serialization significantly faster than complex object marshaling

**Benchmark Results**:
- Simple hash serialization: ~10μs for typical task data
- Complex object marshaling: ~100μs+ for same data
- **10x performance improvement** with hash-based approach

### 3. Memory Management

**Pattern**: Minimize object creation across FFI boundary
```rust
// Efficient: Reuse allocated structures
pub struct TaskContextBuilder {
    base_context: HashMap<String, Value>,
}

impl TaskContextBuilder {
    pub fn build_for_task(&mut self, task_id: i64) -> HashMap<String, Value> {
        self.base_context.clear();
        self.base_context.insert("task_id".to_string(), json!(task_id));
        // ... populate other fields
        self.base_context.clone() // Single allocation
    }
}
```

## Testing Strategies

### 1. FFI Integration Testing

**Pattern**: Test Ruby and Rust together, not in isolation
```ruby
# Integration test example
RSpec.describe "TaskerCore FFI Integration" do
  it "handles complete workflow orchestration" do
    # Test crosses FFI boundary multiple times
    coordinator = TaskerCore.create_coordinator(database_url)
    
    # Create task through FFI
    task_result = coordinator.initialize_task({
      "namespace" => "fulfillment",
      "task_name" => "process_order",
      "payload" => { "order_id" => 12345 }
    })
    
    expect(task_result["task_id"]).to be_present
    expect(task_result["step_count"]).to be > 0
    
    # Orchestrate through FFI
    orchestration_result = coordinator.orchestrate_task(task_result["task_id"])
    
    expect(orchestration_result["status"]).to eq("complete")
  end
end
```

### 2. Error Boundary Testing

**Pattern**: Verify Rust errors become proper Ruby exceptions
```ruby
RSpec.describe "Error Handling" do
  it "converts Rust errors to Ruby exceptions" do
    coordinator = TaskerCore.create_coordinator(database_url)
    
    expect {
      coordinator.orchestrate_task(-1) # Invalid task ID
    }.to raise_error(ArgumentError, /Task not found/)
    
    expect {
      coordinator.orchestrate_task("invalid") # Wrong type
    }.to raise_error(TypeError)
  end
end
```

### 3. Memory Leak Testing

**Pattern**: Verify FFI objects are properly cleaned up
```ruby
RSpec.describe "Memory Management" do
  it "cleans up FFI resources" do
    initial_memory = get_process_memory
    
    1000.times do |i|
      coordinator = TaskerCore.create_coordinator(database_url)
      result = coordinator.orchestrate_task(create_test_task.task_id)
      # Let coordinator go out of scope
    end
    
    GC.start # Force garbage collection
    
    final_memory = get_process_memory
    memory_growth = final_memory - initial_memory
    
    expect(memory_growth).to be < 50.megabytes # Reasonable growth limit
  end
end
```

## Common Pitfalls and Solutions

### 1. Magnus Object Lifetime Issues

**Problem**: Ruby objects can be GC'd while Rust still holds references

**Solution**: Use Arc<> for shared ownership, never store Ruby Value references
```rust
// DON'T: Store Ruby Value in Rust struct
pub struct BadCoordinator {
    ruby_config: Value, // This can become invalid!
}

// DO: Convert to Rust types immediately
pub struct GoodCoordinator {
    config: OrchestrationConfig, // Rust-owned data
}
```

### 2. Thread Safety with Ruby

**Problem**: Ruby objects are not thread-safe

**Solution**: Keep FFI calls on main thread, use async within Rust
```rust
#[magnus::function]
fn safe_orchestrate_task(task_id: i64) -> HashMap<String, Value> {
    // This FFI function runs on Ruby main thread
    get_runtime().block_on(async {
        // Async work happens in Rust thread pool
        orchestrate_async(task_id).await
    })
}
```

### 3. Error Context Loss

**Problem**: Rich Rust error context gets lost in FFI translation

**Solution**: Structured error responses
```rust
#[magnus::function]
fn orchestrate_with_errors(task_id: i64) -> HashMap<String, Value> {
    match orchestrate_internal(task_id) {
        Ok(result) => hashmap! {
            "success" => json!(true),
            "data" => json!(result),
        },
        Err(e) => hashmap! {
            "success" => json!(false),
            "error_type" => json!(e.error_type()),
            "error_message" => json!(e.user_message()),
            "error_context" => json!(e.context()),
        }
    }
}
```

```ruby
# Ruby can make intelligent decisions based on error structure
result = TaskerCore.orchestrate_with_errors(task_id)

if result["success"]
  handle_success(result["data"])
else
  case result["error_type"]
  when "validation_error"
    handle_validation_error(result["error_context"])
  when "database_error"
    handle_system_error(result["error_message"])
  else
    handle_unknown_error(result["error_message"])
  end
end
```

## Production Deployment Lessons

### 1. Ruby Gem Packaging

**Lesson**: Include compiled Rust libraries in gem for deployment simplicity

```ruby
# tasker_core.gemspec
Gem::Specification.new do |spec|
  spec.name = "tasker_core"
  spec.files = Dir["lib/**/*.rb"] + Dir["ext/**/*.{rs,toml,rb}"]
  spec.extensions = ["ext/tasker_core/Cargo.toml"]
  
  # Include pre-compiled binaries for common platforms
  spec.files += Dir["lib/tasker_core/platforms/**/*.{so,dylib,dll}"]
end
```

### 2. Environment-Specific Configuration

**Pattern**: Ruby manages environment differences, Rust handles business logic
```ruby
# config/tasker_core.yml
development:
  database_pool_size: 5
  max_parallel_steps: 2
  step_timeout_ms: 5000
  
production:
  database_pool_size: 20
  max_parallel_steps: 10
  step_timeout_ms: 30000
  retry_limits:
    payment_processing: 3
    inventory_check: 5
```

### 3. Monitoring and Debugging

**Strategy**: Expose Rust metrics to Ruby monitoring systems
```rust
#[magnus::function]
fn get_performance_metrics() -> HashMap<String, Value> {
    let metrics = ORCHESTRATION_METRICS.read().unwrap();
    hashmap! {
        "tasks_per_second" => json!(metrics.tasks_per_second.rate()),
        "average_completion_time_ms" => json!(metrics.completion_time.mean()),
        "error_rate" => json!(metrics.error_rate.rate()),
        "active_tasks" => json!(metrics.active_tasks.count()),
    }
}
```

```ruby
# Ruby monitoring integration
class TaskerCore::Monitor
  def self.collect_metrics
    metrics = TaskerCore.get_performance_metrics
    
    # Send to monitoring system (DataDog, NewRelic, etc.)
    StatsD.gauge("tasker.tasks_per_second", metrics["tasks_per_second"])
    StatsD.gauge("tasker.completion_time", metrics["average_completion_time_ms"])
    StatsD.gauge("tasker.error_rate", metrics["error_rate"])
  end
end
```

## Future FFI Considerations

### 1. Alternative Approaches

**WebAssembly**: For even simpler integration
- No FFI complexity
- Universal runtime
- Easier debugging
- Performance trade-offs

**HTTP API**: For complete decoupling
- Language agnostic
- Operational complexity
- Network overhead
- Clear service boundaries

### 2. Magnus Alternatives

**PyO3 Pattern**: If Python integration needed
**Node.js N-API**: For JavaScript integration
**JNI**: For Java/Scala integration

## Conclusion

Successful Ruby-Rust FFI integration requires embracing the strengths of both languages while minimizing complexity at the boundary. The patterns outlined here represent proven approaches that balance performance, maintainability, and developer experience.

**Key Success Factors**:

1. **Simplicity First**: Choose simple patterns over clever optimizations
2. **Hash-Based Transfer**: Avoid complex object marshaling
3. **Handle-Based Architecture**: Eliminate global state management
4. **Comprehensive Testing**: Test across the FFI boundary, not around it
5. **Ruby-Driven Configuration**: Let Ruby handle environment complexity

**Remember**: The FFI boundary is not just a technical interface—it's an architectural boundary that should be designed with the same care as any other system interface.

---

**Document Purpose**: FFI design guidance for Ruby-Rust integration
**Audience**: Engineers implementing cross-language orchestration systems
**Last Updated**: August 2025
**Status**: Production-proven patterns from real-world FFI implementations