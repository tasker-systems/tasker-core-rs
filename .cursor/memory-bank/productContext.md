# Product Context: Tasker Core Rust

## Why This Project Exists

The Rails Tasker engine is a production workflow orchestration system that handles complex DAG-based task execution. While the Rails implementation provides excellent developer ergonomics and rich domain-specific language, it faces performance bottlenecks in computationally intensive operations:

1. **Dependency Resolution**: PostgreSQL functions for DAG traversal become slow with complex workflows
2. **Step Handler Overhead**: Ruby step handler lifecycle management adds significant latency
3. **State Management**: Database round-trips for state transitions add latency
4. **Event Processing**: High-throughput event publishing hits Ruby performance limits
5. **Framework Lock-in**: Current architecture ties orchestration to Rails exclusively

## Problems This Project Solves

### Performance Bottlenecks
- **Slow Dependency Resolution**: PostgreSQL-based DAG traversal doesn't scale
- **Step Handler Inefficiency**: Ruby step lifecycle management with GIL limitations
- **State Transition Overhead**: Multiple database queries for state changes
- **Event Processing Limits**: Ruby event publishing can't handle high throughput

### Integration Challenges
- **Multi-Framework Support**: Need same orchestration across Rails, Python FastAPI, Node.js
- **Language Boundaries**: Efficient step handler foundation across different languages
- **Queue System Flexibility**: Support for different queue backends (Sidekiq, Celery, Bull)
- **Performance vs Maintainability**: Framework ergonomics with high-performance core

## How It Should Work

### User Experience Goals

#### For Framework Developers (Rails, Python, Node.js)
- **Familiar Step Handler Pattern**: Same `process()` and `process_results()` methods as current Rails
- **Transparent Foundation**: Rust handles all lifecycle logic invisibly
- **Framework-Native Feel**: Step handlers feel like native framework classes
- **Queue Integration**: Works with existing queue systems (Sidekiq, Celery, Bull)
- **Zero Migration**: Existing step handler logic works unchanged

#### For System Administrators
- **Universal Orchestration**: Same performance across Rails, Python, Node.js applications
- **Performance Monitoring**: Comprehensive metrics across all framework integrations
- **Resource Efficiency**: Better CPU and memory utilization with Rust foundation
- **Scalability**: Handle larger, more complex workflows consistently

#### For Integration Partners
- **Multi-Language Support**: Ruby, Python, Node.js, and C FFI interfaces
- **Queue Flexibility**: Dependency injection for different queue backends
- **Event Streaming**: High-throughput event publishing for external systems
- **Standard Foundation**: Consistent behavior across all language bindings

### Control Flow Design (Corrected)

```
┌─────────────┐    ┌─────────────────────────────────────┐    ┌─────────────────┐
│   Queue     │───▶│           Rust Core                 │───▶│ Re-enqueue      │
│ (Framework) │    │ ┌─────────────────────────────────┐ │    │ (Framework)     │
└─────────────┘    │ │     Step Handler Foundation     │ │    └─────────────────┘
                   │ │  • handle() logic               │ │             ▲
                   │ │  • backoff calculations         │ │             │
                   │ │  • retry analysis               │ │             │
                   │ │  • step output processing       │ │             │
                   │ │  • task finalization            │ │             │
                   │ └─────────────────────────────────┘ │             │
                   │              │                      │             │
                   │              ▼                      │             │
                   │ ┌─────────────────────────────────┐ │             │
                   │ │   Framework Step Handler        │ │             │
                   │ │   (Rails/Python/Node subclass)  │ │             │
                   │ │  • process() - user logic       │ │             │
                   │ │  • process_results() - post     │ │─────────────┘
                   │ └─────────────────────────────────┘ │
                   └─────────────────────────────────────┘
```

### Core User Workflows

#### Step Handler Development (Framework Developer)
1. **Subclass Foundation**: Extend Rust step handler base class in framework of choice
2. **Implement Business Logic**: Override `process()` method with domain-specific logic
3. **Handle Results**: Override `process_results()` for post-processing if needed
4. **Foundation Handles Rest**: Rust manages lifecycle, retries, backoff, finalization
5. **Queue Integration**: Framework-specific queue injection handles re-enqueuing

#### Task Execution (System)
1. **Queue Receives Task**: Framework queue system (Sidekiq/Celery/Bull) receives task
2. **Rust Foundation**: High-speed dependency analysis and step discovery
3. **Step Handler Execution**: Rust foundation calls framework `process()` methods
4. **Result Processing**: Rust handles output processing and step finalization
5. **Re-enqueue Decision**: Rust decides next action, delegates queuing to framework

#### Multi-Framework Development (Universal)
1. **Rails Application**: Uses Ruby step handler subclasses with Sidekiq queuing
2. **Python FastAPI**: Uses Python step handler subclasses with Celery queuing
3. **Node.js Express**: Uses JavaScript step handler subclasses with Bull queuing
4. **Consistent Behavior**: Same orchestration logic and performance across all frameworks

#### Monitoring and Observability (Operations)
1. **Universal Metrics**: Same performance data across Rails, Python, Node.js
2. **Event Streams**: Cross-language event publishing for monitoring systems
3. **State Visibility**: Comprehensive audit trails across all framework integrations
4. **Error Handling**: Detailed error reporting with framework-specific context

## Expected Benefits

### Performance Improvements
- **10-100x Faster Dependency Resolution**: Replace PostgreSQL functions with optimized Rust
- **Efficient Step Handlers**: Rust foundation eliminates Ruby/Python lifecycle overhead
- **Reduced Latency**: Sub-millisecond state transitions and step processing
- **Higher Throughput**: >10k events/sec processing capability across all frameworks

### Developer Experience
- **Universal Foundation**: Same step handler pattern across Rails, Python, Node.js
- **Zero Migration Effort**: Existing step handler logic works unchanged
- **Better Debugging**: Detailed error messages with framework-specific context
- **Improved Testing**: Consistent testing patterns across all frameworks
- **Enhanced Monitoring**: Rich observability across all language integrations

### System Reliability
- **Memory Safety**: Eliminate memory leaks in long-running processes across all frameworks
- **Concurrent Safety**: Data race elimination in multi-threaded operations
- **Error Recovery**: Robust error handling with automatic retry logic
- **Resource Management**: Efficient CPU and memory utilization universally

### Framework Flexibility
- **Queue Backend Choice**: Support Sidekiq, Celery, Bull, and other queue systems
- **Language Freedom**: Choose Rails, Python, Node.js based on team expertise
- **Gradual Migration**: Adopt Rust foundation incrementally per application
- **Consistent Performance**: Same orchestration capabilities regardless of framework

## Integration Patterns

### Rails Integration
```ruby
# Rails step handler subclasses Rust foundation
class MyStepHandler < Tasker::StepHandler
  def process(context)
    # Business logic in Ruby
    { result: "processed", data: context.inputs }
  end

  def process_results(output)
    # Post-processing in Ruby
    Rails.logger.info "Step completed: #{output}"
    output
  end
end
```

### Python Integration
```python
# Python step handler subclasses Rust foundation
class MyStepHandler(tasker.StepHandler):
    async def process(self, context):
        # Business logic in Python
        return {"result": "processed", "data": context.inputs}

    async def process_results(self, output):
        # Post-processing in Python
        logger.info(f"Step completed: {output}")
        return output
```

### Node.js Integration
```javascript
// Node.js step handler subclasses Rust foundation
class MyStepHandler extends tasker.StepHandler {
    async process(context) {
        // Business logic in JavaScript
        return { result: "processed", data: context.inputs };
    }

    async processResults(output) {
        // Post-processing in JavaScript
        console.log(`Step completed: ${JSON.stringify(output)}`);
        return output;
    }
}
```

### Queue System Integration
```rust
// Dependency injection for different queue backends
pub trait QueueInjector {
    async fn enqueue_task(&self, task_id: i64, delay: Option<Duration>) -> Result<()>;
}

// Rails Sidekiq implementation
impl QueueInjector for SidekiqInjector {
    async fn enqueue_task(&self, task_id: i64, delay: Option<Duration>) -> Result<()> {
        // Delegate to Sidekiq via Ruby FFI
    }
}

// Python Celery implementation
impl QueueInjector for CeleryInjector {
    async fn enqueue_task(&self, task_id: i64, delay: Option<Duration>) -> Result<()> {
        // Delegate to Celery via Python FFI
    }
}
```

## Universal Foundation Benefits

### Consistent Developer Experience
- **Same API**: `process()` and `process_results()` methods across all frameworks
- **Predictable Behavior**: Identical orchestration logic regardless of language
- **Shared Documentation**: Single set of docs for all framework integrations
- **Common Patterns**: Same testing, debugging, and monitoring approaches

### Operational Simplicity
- **Unified Monitoring**: Same metrics and observability across all applications
- **Consistent Performance**: Predictable behavior across Rails, Python, Node.js
- **Simplified Deployment**: Same Rust foundation across all environments
- **Reduced Complexity**: Single orchestration core instead of multiple implementations

### Strategic Flexibility
- **Technology Choice**: Pick framework based on team expertise, not orchestration limits
- **Migration Path**: Move between frameworks without losing orchestration capabilities
- **Scaling Options**: Use different frameworks for different microservices
- **Future-Proofing**: Add new framework support without changing core logic
