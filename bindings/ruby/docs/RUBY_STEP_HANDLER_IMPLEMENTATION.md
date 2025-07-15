# RubyStepHandler Implementation

## Overview

The `RubyStepHandler` provides a clean bridge between the Rust `StepHandler` trait and Ruby business logic methods. This implementation follows the architectural principle where:

- **Rust handles**: Orchestration, state management, database operations, concurrency
- **Ruby handles**: Business logic in simple `process()` and `process_results()` methods

## Architecture

### Core Components

1. **`RubyStepHandler` struct**: Implements the Rust `StepHandler` trait
2. **Data conversion functions**: Convert Rust models to Ruby hashes
3. **Sequence building**: Uses `WorkflowStep::get_dependencies()` to build step navigation
4. **Thread safety**: Handles Magnus Value thread safety requirements

### Flow Diagram

```
Rust Orchestration → RubyStepHandler → Ruby Business Logic
     ↓                    ↓                    ↓
StepExecutor        implements         process(task, sequence, step)
     ↓              StepHandler              ↓
WorkflowCoordinator      ↓            process_results(step, output, initial)
     ↓                   ↓                    ↓
Database Models    Type Conversion      Ruby Hash/JSON
```

## Ruby API

Ruby developers implement these simple methods:

```ruby
class MyStepHandler
  def process(task, sequence, step)
    # Business logic here
    # task: Hash with task data from database
    # sequence: Hash with step dependencies and navigation
    # step: Hash with current step data
    { result: "success" }
  end

  def process_results(step, process_output, initial_results = nil)
    # Optional result transformation
    # step: Hash with current step data
    # process_output: Result from process() method
    # initial_results: Previous results if this is a retry
    process_output
  end
end
```

## Data Structures

### Task Hash
```ruby
{
  "task_id" => 123,
  "named_task_id" => 456,
  "complete" => false,
  "context" => { "order_id" => 789 },
  "tags" => ["payment", "urgent"],
  "created_at" => "2025-01-13T10:30:00.000000Z",
  # ... other task fields
}
```

### Sequence Hash
```ruby
{
  "steps" => [
    { "workflow_step_id" => 1, "named_step_id" => 10, ... },  # Dependencies
    { "workflow_step_id" => 2, "named_step_id" => 11, ... },  # Current step
  ],
  "current_step_index" => 1,
  "total_steps" => 2
}
```

### Step Hash
```ruby
{
  "workflow_step_id" => 123,
  "task_id" => 456,
  "named_step_id" => 789,
  "inputs" => { "amount" => 100.0 },
  "results" => { "previous_result" => "data" },
  "attempts" => 1,
  "retry_limit" => 3,
  "retryable" => true,
  # ... other step fields
}
```

## Implementation Details

### Thread Safety
- Uses `unsafe impl Send + Sync` for Magnus Values
- Stores Ruby method references as strings for thread safety
- Actual Ruby method calls will be implemented with proper synchronization

### Database Integration
- Loads `Task` and `WorkflowStep` models from database
- Builds step sequence using `get_dependencies()`
- Converts Rust models to Ruby-friendly hashes

### Error Handling
- Comprehensive error mapping with specific error codes
- Proper `OrchestrationError` integration
- Detailed error context for debugging

## Current Status

### ✅ Completed
- `RubyStepHandler` struct implementing `StepHandler` trait
- Data conversion functions for Task and WorkflowStep
- Step sequence building using dependencies
- Thread safety implementation
- Compilation and basic structure

### 🚧 TODO
- Actual Ruby method calling implementation
- Proper Magnus Value thread synchronization
- Integration with orchestration system registration
- Error handling refinements
- Performance optimizations

## Usage Example

```ruby
# Define handler class
class PaymentStepHandler
  def process(task, sequence, step)
    amount = step['inputs']['amount']
    # Process payment logic
    { status: "success", payment_id: "pay_123" }
  end

  def process_results(step, output, initial)
    # Add audit trail
    output.merge(processed_at: Time.now.iso8601)
  end
end

# Register with system (future implementation)
handler = PaymentStepHandler.new
TaskerCore.register_step_handler(
  handler.method(:process),
  handler.method(:process_results)
)
```

## Benefits

1. **Clean Separation**: Ruby focuses on business logic, Rust handles orchestration
2. **Simple API**: Familiar Ruby hash/JSON data structures
3. **Type Safety**: Rust ensures data integrity and proper error handling
4. **Performance**: Database operations and concurrency handled in Rust
5. **Flexibility**: Ruby developers can use any Ruby patterns/libraries

## Next Steps

1. Implement actual Ruby method calling with proper thread synchronization
2. Add registration mechanism to connect handlers with step names
3. Integrate with `FrameworkIntegration` trait for orchestration
4. Add comprehensive error handling and logging
5. Performance testing and optimization
