# Ruby Step Handler Bridge Implementation

## Overview

You were absolutely right to call out the original approach! The initial implementation was flawed because it was trying to store Magnus `Value` objects in a Rust struct that needed to implement `Send + Sync` for the `StepHandler` trait. This created thread safety issues because Magnus `Value` objects are not thread-safe.

## The Problem

The original approach tried to:
1. Store a Ruby `Value` in a Rust struct (`RubyStepHandler`)
2. Implement `StepHandler` trait on that struct
3. Use `unsafe impl Send + Sync` to bypass thread safety

This was problematic because:
- Magnus `Value` objects contain raw Ruby pointers that aren't thread-safe
- The `unsafe impl Send + Sync` was a hack that could lead to memory safety issues
- It went against Magnus' design principles

## The Solution

Instead of trying to store Ruby Values in structs, we created a **bridge approach**:

### Architecture

```
Rust StepHandler ──→ Registry ──→ Ruby Handler Instance
                                      ↓
                              Ruby process() method
                                      ↓
                              Ruby process_results() method
```

### Key Components

1. **Bridge Functions**: Instead of a struct, we have standalone functions that:
   - Take Ruby handler instances as parameters (not stored)
   - Convert Rust data to Ruby hashes
   - Call Ruby methods directly
   - Convert results back to Rust

2. **FFI Functions**: Ruby-callable functions that:
   - Accept Ruby Values as parameters
   - Handle async execution using `execute_async`
   - Provide proper error handling

3. **Data Conversion**: Utility functions that:
   - Convert `Task` and `WorkflowStep` models to Ruby hashes
   - Build step sequences using `get_dependencies()`
   - Handle JSON serialization/deserialization

### Ruby API

Ruby developers implement handlers with these methods:

```ruby
class MyStepHandler
  def process(task, sequence, step)
    # Business logic here
    { result: "success", data: process_data(task, step) }
  end

  def process_results(step, process_output, initial_results = nil)
    # Optional result transformation
    process_output
  end
end
```

### Usage

```ruby
# Create handler instance
handler = MyStepHandler.new

# Convert context to JSON
context_json = {
  step_id: 123,
  task_id: 456,
  step_name: "process_payment",
  input_data: { amount: 100.0 },
  # ... other context fields
}

# Call through bridge
result = TaskerCore.call_ruby_process_method(handler, context_json)
```

## Benefits

1. **Thread Safety**: No stored Ruby Values, no thread safety issues
2. **Proper Architecture**: Follows Magnus design principles
3. **Flexibility**: Ruby handlers can be created and destroyed as needed
4. **Performance**: Direct function calls without struct overhead
5. **Maintainability**: Clear separation between Rust orchestration and Ruby business logic

## How It Works

1. **Ruby Registration**: Ruby handlers are registered with the orchestration system
2. **Bridge Calls**: When a step needs execution, the bridge functions are called
3. **Data Conversion**: Rust models are converted to Ruby hashes on-demand
4. **Ruby Execution**: Ruby methods are called with converted data
5. **Result Handling**: Ruby results are converted back to Rust JSON

## Integration with Orchestration

The bridge functions can be integrated with the existing orchestration system by:
1. Creating a Rust `StepHandler` implementation that calls the bridge functions
2. Registering Ruby handlers in a thread-safe registry
3. Using the bridge to delegate to Ruby when needed

This approach maintains the orchestration benefits while enabling Ruby business logic without thread safety issues.

## Next Steps

1. **Registry Integration**: Implement a thread-safe registry for Ruby handlers
2. **Error Handling**: Enhance error propagation between Rust and Ruby
3. **Performance Optimization**: Add caching for frequently converted data
4. **Testing**: Create comprehensive tests for the bridge functionality

This solution properly separates concerns: Rust handles orchestration and thread safety, while Ruby handles business logic through a clean, safe bridge interface.
