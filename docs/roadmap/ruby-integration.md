# Ruby Integration Architecture

**Consolidated Ruby FFI Integration Strategy**

## Overview

Ruby integration follows a delegation pattern where:
- **Ruby handles**: Business logic, framework integration, job queuing
- **Rust handles**: Orchestration, state management, dependency resolution, performance-critical operations

## Performance Targets
- **10-100x faster** dependency resolution vs PostgreSQL functions
- **<1ms FFI overhead** per orchestration call  
- **>10k events/sec** cross-language event processing
- **<10% penalty** vs native Ruby execution for delegation

## Architecture Pattern

```
Rails Engine (Business Logic) ↔ tasker-core-rb (FFI Bridge) ↔ tasker-core-rs (Performance Core)
```

### Core Components Exposed

**1. Handler Foundation**
- `TaskerCore::BaseTaskHandler` - Task orchestration foundation
- `TaskerCore::BaseStepHandler` - Step execution with process/process_results hooks
- `TaskerCore::TaskContext` - Task-level execution context
- `TaskerCore::StepContext` - Step-level execution context

**2. Orchestration Core**
- `TaskerCore::WorkflowCoordinator` - Main orchestration engine
- `TaskerCore::TaskResult` - Task completion states
- `TaskerCore::StepResult` - Individual step execution results
- `TaskerCore::ViableStep` - Ready-to-execute steps

**3. Event Publishing System**
- `TaskerCore::EventPublisher` - Subscribe and publish lifecycle events
- `TaskerCore::EventSubscriber` - Ruby event subscriber interface

**4. Analytics & Insights**
- Performance metrics and system health monitoring
- Real-time bottleneck identification
- Task-level performance analysis

## Ruby-Rust Integration Workflow

### Phase 1: Task Discovery & Registration
```ruby
# Rails discovers and instantiates handlers
registry = TaskerCore::TaskHandlerRegistry.new
result = registry.find_handler_for_task_request(task_request)
rails_handler = result["ruby_class_name"].constantize.new(result["yaml_template"])
```

### Phase 2: Task Initialization  
```ruby
# Rails calls handler, delegates to Rust for task creation
result = rails_handler.initialize_task(task_request)
# Creates Task + WorkflowSteps + DAG dependencies in database
```

### Phase 3: Task Execution
```ruby
# Rails job executes, Rust orchestrates concurrently
result = handler.handle(task_id)
# WorkflowCoordinator processes steps concurrently up to max_parallel_steps
```

### Phase 4: Concurrent Step Processing
```rust
// Rust orchestration loop with concurrent Ruby step execution
loop {
    let viable_steps = ViableStepDiscovery::find_ready_steps(pool, task_id).await?;
    if viable_steps.is_empty() { break; }
    
    // Process steps concurrently up to limit
    let concurrent_futures: Vec<_> = viable_steps
        .into_iter()
        .take(max_parallel_steps)
        .map(|step| framework_integration.execute_single_step(&step, &task_context))
        .collect();
    
    let step_results = futures::future::join_all(concurrent_futures).await;
    // Process results and loop for newly viable steps
}
```

### Phase 5: TaskFinalizer & Intelligent Backoff
```rust
// Analyzes completion status and determines next action
match finalization_outcome {
    FinalizationOutcome::TaskComplete => /* finalize successful */,
    FinalizationOutcome::TaskFailed => /* finalize failed */,
    FinalizationOutcome::RetryViable => {
        // Calculate intelligent delay with BackoffCalculator
        let delay = backoff_calculator.calculate_optimal_delay(retry_steps).await?;
        task_enqueuer.reenqueue_with_delay(task, delay).await?;
    }
}
```

### Phase 6: Event Publishing
```ruby
# Rails subscribes to Rust events
TaskerCore::EventPublisher.subscribe do |event|
  case event.name
  when "task.enqueue" then TaskRunnerJob.perform_later(event.data["task_id"])
  when "task.reenqueue" then TaskRunnerJob.set(wait: delay).perform_later(task_id)
  when "task.failed" then ErrorNotificationService.alert(event.data)
  end
end
```

## Implementation Status

### ✅ Implemented
- **Handler Foundation**: Ruby base classes with proper hooks
- **Task Initialization**: Complete TaskRequest → Task creation with state machines
- **Database Integration**: Task, WorkflowStep, and state transition creation
- **TaskHandlerRegistry Core**: Handler lookup with Ruby class names and YAML templates (needs singleton pattern)
- **Result Conversion**: TaskOrchestrationResult → Ruby conversion
- **Event Publishing Core**: Unified event system ready for FFI bridge to Rails dry-events

### 🚧 Critical Missing (Current Focus)
- **TaskHandlerRegistry Singleton Pattern**: FFI wrapper functions create new instances losing all registered handlers
- **Step Delegation**: FrameworkIntegration trait has placeholder for step execution
- **Queue Integration**: No TaskEnqueuer for Rails job queue integration
- **Event Publishing FFI Bridge**: Core unified events need Ruby FFI bridge to Rails dry-events

### 🚧 TODO (Next Phase)
- **Error Translation**: Rust → Ruby exception hierarchy
- **Configuration**: Remove hardcoded values
- **Testing**: End-to-end workflow validation

## Type Conversion Strategy

### Rust → Ruby Conversions
| Rust Type | Ruby Type | Method |
|-----------|-----------|---------|
| `i64` | `Integer` | Direct |
| `String` | `String` | UTF-8 |
| `serde_json::Value` | `Hash`/`Array` | Recursive JSON |
| `chrono::DateTime<Utc>` | `Time` | UTC conversion |
| `Vec<T>` | `Array` | Element-wise |

### Error Handling
- Rust `OrchestrationError` → Ruby exceptions
- Ruby step handler exceptions → Rust `StepResult::Failed`
- Proper error context and stack trace preservation

## Memory Management

### Ruby GC Integration
- Stack-only Ruby objects (Magnus requirement)
- `Value` wrapper for Ruby object lifetime management
- Proper cleanup on Ruby object finalization

### Async Runtime Management  
- Single `tokio::Runtime` instance per coordinator
- `Arc<Runtime>` for thread-safe sharing
- Block on async operations in FFI layer

## Development Phases

### Phase 1: Foundation (✅ Complete)
- Ruby gem structure and Magnus FFI
- Base handler classes and context objects
- Task initialization and database integration

### Phase 2: Orchestration (🚧 Current)
- Complete event publishing system
- State machine integration in client handlers
- Ruby step handler delegation
- Queue integration for task enqueuing

### Phase 3: Production (🚧 Next)
- Error handling and translation
- Configuration management
- Performance validation
- Comprehensive testing

---
**Source**: Consolidated from docs/RUBY.md and bindings/ruby/RUBY.md  
**Last Updated**: 2025-01-13  
**Status**: Active Development - Phase 2