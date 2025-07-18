# Ruby Integration Examples

This directory contains examples demonstrating how to integrate Ruby step handlers with the Tasker Core Rust orchestration system using the composition-based architecture.

## Examples

### 1. Complete Integration Example

**File:** `integration_example.rb`

This comprehensive example demonstrates the full Ruby-Rust integration workflow:

- **Step Handler Implementation**: Shows how to create Ruby step handlers that inherit from `TaskerCore::StepHandler::Base`
- **Handler Registration**: Automatic registration with the TaskHandlerRegistry
- **FFI Bridge Testing**: Demonstrates the process_with_context bridge methods
- **Factory System Integration**: Using the factory system to create test data
- **Error Handling**: Proper error classification with `PermanentError` and `RetryableError`
- **Result Processing**: Using the `process_results` hook for result transformation
- **Configuration and Metadata**: Handler configuration and introspection
- **Performance Testing**: Benchmarking handler execution
- **Registry Integration**: Working with the TaskHandlerRegistry system

### 2. Composition Integration Example

**File:** `composition_integration_example.rb`

This example demonstrates:

- **Step Handler Implementation**: Shows how to create Ruby step handlers that inherit from `TaskerCore::StepHandler::Base`
- **Handler Registration**: Automatic registration with the orchestration system
- **Factory System Integration**: Using the factory system to create test data
- **Error Handling**: Proper error classification with `PermanentError` and `RetryableError`
- **Result Processing**: Using the `process_results` hook for result transformation
- **Configuration and Metadata**: Handler configuration and introspection

#### Key Features Demonstrated

1. **Payment Processing Handler**:
   - Validates payment amounts
   - Handles different payment methods
   - Throws appropriate errors for different scenarios
   - Returns structured payment results

2. **Notification Handler**:
   - Supports multiple notification types (email, SMS, push)
   - Simulates external service failures
   - Demonstrates retry logic with `RetryableError`

3. **Integration Testing**:
   - Creates test data using factory wrappers
   - Executes step handlers with mock data
   - Demonstrates handler metadata and capabilities
   - Cleans up test data

#### Usage

```bash
# Run the example
cd /path/to/tasker-core-rs/bindings/ruby/examples
ruby composition_integration_example.rb
```

### 3. Production-Ready Task Configuration

**File:** `config/payment_workflow.yaml`

This comprehensive YAML configuration demonstrates:

- **Task Template Structure**: Complete task configuration with 5 step workflow
- **Step Dependencies**: Complex dependency chains between steps
- **Retry Configuration**: Exponential backoff strategies and retry limits
- **Handler Configuration**: Detailed step-specific configuration
- **Security Settings**: PCI compliance and encryption configuration
- **Performance Optimization**: Connection pooling, caching, and concurrency settings
- **Event Configuration**: Custom events and monitoring
- **Metrics and Alerts**: Production-ready monitoring configuration
- **Error Handling**: Comprehensive error classification and escalation rules
- **Testing Configuration**: Test scenarios and mock services

### 4. Handler Registration Guide

**File:** `HANDLER_REGISTRATION_GUIDE.md`

This comprehensive guide demonstrates:

- **Automatic Registration**: How handlers register themselves during initialization
- **Registry Management**: Working with the TaskHandlerRegistry system
- **Configuration-Based Registration**: Loading handlers from YAML configuration
- **TaskConfigFinder Integration**: Using configuration files to resolve handlers
- **Error Handling**: Registration errors and recovery strategies
- **Best Practices**: Naming conventions, lifecycle management, and testing
- **Migration Guide**: Moving from legacy patterns to composition architecture

### 5. Legacy Task Configuration Example

**File:** `payment_processing_task.yaml`

This YAML file demonstrates:

- **Task Template Structure**: Complete task configuration with steps
- **Step Dependencies**: How steps depend on each other
- **Retry Configuration**: Backoff strategies and retry limits
- **Handler Configuration**: Step-specific configuration
- **Environment Overrides**: Different settings per environment
- **Event Configuration**: Custom events and monitoring
- **Metrics and Alerts**: Monitoring configuration

#### Key Sections

1. **Step Templates**: Define the workflow structure
2. **Handler Configuration**: Step-specific settings
3. **Retry Policies**: Backoff and retry strategies
4. **Environment Overrides**: Environment-specific configurations
5. **Events**: Custom event definitions
6. **Monitoring**: Metrics and alerting configuration

## Architecture Overview

### Composition-Based Design

The new architecture follows a composition pattern where:

1. **Rust Core**: Provides orchestration, state management, and performance optimization
2. **Ruby Handlers**: Implement business logic through inheritance from `TaskerCore::StepHandler::Base`
3. **FFI Bridge**: Seamless integration between Rust and Ruby
4. **Configuration Management**: YAML-based configuration with environment overrides

### Handler Development Pattern

```ruby
class MyStepHandler < TaskerCore::StepHandler::Base
  def initialize(config: {}, logger: nil)
    super(config: config, logger: logger)
    # Custom initialization
  end

  def process(task, sequence, step)
    # Business logic here
    # Return structured result
  end

  def process_results(step, process_output, initial_results = nil)
    # Optional result transformation
    # Return enhanced results
  end
end
```

### Error Handling

The system provides two main error types:

1. **PermanentError**: For validation errors and permanent failures
2. **RetryableError**: For transient errors that should be retried

```ruby
# Permanent error (won't be retried)
raise TaskerCore::PermanentError.new(
  "Invalid input data",
  error_code: 'VALIDATION_ERROR',
  error_category: 'validation'
)

# Retryable error (will be retried with backoff)
raise TaskerCore::RetryableError.new(
  "External service temporarily unavailable",
  error_category: 'external_service',
  retry_after: 60  # seconds
)
```

### Factory System Integration

The factory system provides test data creation:

```ruby
# Create foundation data (namespace, named_task, named_step)
foundation = TaskerCore::TestHelpers.create_test_foundation_with_factory({
  namespace: 'my_namespace',
  task_name: 'my_task',
  step_name: 'my_step'
})

# Create a task
task = TaskerCore::TestHelpers.create_test_task_with_factory({
  context: { key: 'value' },
  tags: { environment: 'test' }
})

# Create a workflow step
step = TaskerCore::TestHelpers.create_test_workflow_step_with_factory({
  task_id: task['task_id'],
  inputs: { param: 'value' }
})
```

## Running Examples

### Prerequisites

1. **Database Setup**: Ensure PostgreSQL is running
2. **Environment Variables**: Set `DATABASE_URL` if needed
3. **Ruby Dependencies**: Install required gems

### Steps

1. **Setup Database**:
   ```bash
   # Run migrations
   result = TaskerCore::TestHelpers.run_migrations(DATABASE_URL)
   ```

## Testing Integration

### Unit Testing

```ruby
RSpec.describe PaymentProcessingStepHandler do
  let(:handler) { PaymentProcessingStepHandler.new }
  let(:mock_task) { double('task', task_id: 123) }
  let(:mock_sequence) { double('sequence', total_steps: 2) }
  let(:mock_step) { double('step', workflow_step_id: 456, inputs: { amount: 100 }) }

  it 'processes payment successfully' do
    result = handler.process(mock_task, mock_sequence, mock_step)
    expect(result[:status]).to eq('completed')
    expect(result[:payment_id]).to be_present
  end
end
```

### Integration Testing

```ruby
RSpec.describe 'Payment Processing Integration' do
  it 'creates and processes payment workflow' do
    # Create test data
    foundation = TaskerCore::TestHelpers.create_test_foundation_with_factory({
      namespace: 'payment_processing'
    })

    task = TaskerCore::TestHelpers.create_test_task_with_factory({
      context: { customer_id: 'cust_123' }
    })

    # Test handler execution
    handler = PaymentProcessingStepHandler.new
    # ... test logic
  end
end
```

## Configuration Management

### Task Configuration

Task configuration is loaded from YAML files that define:

- **Task metadata**: Name, version, description
- **Step templates**: Step definitions with dependencies
- **Handler configuration**: Step-specific settings
- **Environment overrides**: Different settings per environment
- **Event configuration**: Custom events and monitoring
- **Retry policies**: Backoff and retry strategies

### Handler Configuration

Step handlers receive configuration through their `initialize` method:

```ruby
class MyStepHandler < TaskerCore::StepHandler::Base
  def initialize(config: {}, logger: nil)
    super(config: config, logger: logger)

    @timeout = config['timeout'] || 30
    @max_retries = config['max_retries'] || 3
    @api_key = config['api_key'] || ENV['API_KEY']
  end
end
```

### Environment Overrides

Configuration can be overridden per environment:

```yaml
environments:
  development:
    task_config:
      timeout_seconds: 60
      retry_limit: 1

  production:
    task_config:
      timeout_seconds: 300
      retry_limit: 5
```

## Monitoring and Observability

### Events

The system publishes events for monitoring:

- **Step Events**: `step_started`, `step_completed`, `step_failed`, `step_retried`
- **Task Events**: `task_started`, `task_completed`, `task_failed`
- **Custom Events**: Application-specific events

### Metrics

Built-in metrics include:

- **Duration Metrics**: Step and task execution times
- **Success Rates**: Percentage of successful executions
- **Error Rates**: Failure rates by error type
- **Retry Metrics**: Retry counts and success rates

### Alerting

Configure alerts based on metrics:

```yaml
alerts:
  - name: high_failure_rate
    condition: "success_rate < 0.95"
    severity: "warning"
    description: "Success rate has dropped below 95%"
```

## Best Practices

### 1. Error Handling

- Use `PermanentError` for validation errors and permanent failures
- Use `RetryableError` for transient errors with appropriate retry intervals
- Include error codes and categories for better error tracking

### 2. Configuration

- Use YAML configuration files for step templates
- Override configuration per environment
- Include validation rules in handler configuration

### 3. Testing

- Use the factory system for test data creation
- Clean up test data after each test
- Test both success and failure scenarios

### 4. Monitoring

- Define custom events for business-specific scenarios
- Configure metrics for key performance indicators
- Set up alerts for critical failures

### 5. Performance

- Use appropriate timeouts for external service calls
- Configure retry policies based on service characteristics
- Monitor step execution times and optimize as needed

## Troubleshooting

### Common Issues

1. **Handler Registration Failures**: Check that handlers inherit from `TaskerCore::StepHandler::Base`
2. **Database Connection Issues**: Verify `DATABASE_URL` is set correctly
3. **Configuration Loading**: Ensure YAML files are valid and accessible
4. **Error Handling**: Use proper error types for different scenarios

### Debugging

1. **Enable Logging**: Set log level to `DEBUG` for detailed output
2. **Check Handler Metadata**: Use `handler.metadata` to inspect handler configuration
3. **Test Factory System**: Verify test data creation works correctly
4. **Monitor Events**: Check that events are being published correctly

## Contributing

When adding new examples:

1. **Follow Patterns**: Use existing examples as templates
2. **Include Tests**: Add both unit and integration tests
3. **Document Configuration**: Include YAML configuration files
4. **Add Error Handling**: Demonstrate proper error handling
5. **Update README**: Document new examples and features
