# Handler Registration Guide

This guide explains how to register and use step handlers with the new composition-based architecture and TaskHandlerRegistry system.

## Overview

The new composition system introduces a unified approach to handler registration that bridges Ruby business logic with Rust orchestration. This system provides:

- **Automatic Registration**: Handlers register themselves during initialization
- **Registry Management**: Centralized handler discovery and management
- **Configuration Integration**: YAML-based configuration loading
- **Cross-Language Support**: Seamless Ruby-Rust integration

## Architecture Components

### 1. TaskHandlerRegistry (Rust Core)

The Rust core provides a singleton `TaskHandlerRegistry` that manages handler discovery and registration:

```rust
use tasker_core::registry::TaskHandlerRegistry;

// Get the global registry instance
let registry = TaskHandlerRegistry::global();

// Register a handler
registry.register_handler(
    "MyStepHandler".to_string(),
    "my_step".to_string(),
    handler_config
);

// Discover handlers
let handler = registry.get_handler("my_step");
```

### 2. Ruby Base Class Integration

Ruby step handlers automatically register with the orchestration system:

```ruby
class MyStepHandler < TaskerCore::StepHandler::Base
  def initialize(config: {}, logger: nil)
    super(config: config, logger: logger)
    # Handler is automatically registered during initialization
  end

  def process(task, sequence, step)
    # Business logic here
  end
end
```

### 3. FFI Bridge Functions

FFI functions provide Ruby access to the registry:

```ruby
# Check if handler is registered
TaskerCore.get_step_handler("MyStepHandler")

# List all registered handlers
TaskerCore.list_step_handlers

# Register a handler programmatically
TaskerCore.register_step_handler("MyStepHandler", "my_step")
```

## Handler Registration Patterns

### 1. Automatic Registration (Recommended)

Handlers automatically register themselves when instantiated:

```ruby
class PaymentProcessingStepHandler < TaskerCore::StepHandler::Base
  def initialize(config: {}, logger: nil)
    super(config: config, logger: logger)
    # Automatically registers as "payment_processing" step
  end

  def process(task, sequence, step)
    # Process payment logic
    {
      status: 'completed',
      payment_id: generate_payment_id,
      amount: step.inputs['amount']
    }
  end
end

# Handler is registered when instantiated
handler = PaymentProcessingStepHandler.new
```

### 2. Manual Registration

For advanced scenarios, handlers can be registered manually:

```ruby
class CustomStepHandler < TaskerCore::StepHandler::Base
  def initialize(config: {}, logger: nil)
    # Skip automatic registration
    @config = config || {}
    @logger = logger || default_logger
    
    # Custom registration logic
    register_custom_handler
  end

  private

  def register_custom_handler
    result = TaskerCore.register_step_handler(
      self.class.name,
      "custom_step_name"
    )
    
    if result['status'] == 'registered'
      @logger.info "Custom handler registered successfully"
    else
      @logger.error "Registration failed: #{result['error']}"
    end
  end
end
```

### 3. Configuration-Based Registration

Handlers can be registered based on YAML configuration:

```yaml
# config/handlers.yaml
handlers:
  - name: payment_processing
    class: PaymentProcessingStepHandler
    config:
      timeout: 30
      retry_limit: 3
      
  - name: notification_sender
    class: NotificationStepHandler
    config:
      timeout: 15
      retry_limit: 5
```

```ruby
class ConfigurationRegistrar
  def self.register_from_yaml(config_path)
    config = YAML.load_file(config_path)
    
    config['handlers'].each do |handler_config|
      handler_class = Object.const_get(handler_config['class'])
      handler = handler_class.new(config: handler_config['config'])
      
      # Handler registers itself during initialization
      puts "Registered #{handler_config['name']} handler"
    end
  end
end

# Register all handlers from configuration
ConfigurationRegistrar.register_from_yaml('config/handlers.yaml')
```

## Registry Management

### Checking Registration Status

```ruby
class MyStepHandler < TaskerCore::StepHandler::Base
  def setup_complete?
    registered? && dependencies_available?
  end

  def registered?
    result = TaskerCore.get_step_handler(self.class.name)
    result['exists'] == true
  end

  def dependencies_available?
    # Check if required services are available
    database_available? && external_services_available?
  end
end
```

### Listing Registered Handlers

```ruby
def audit_registered_handlers
  handlers = TaskerCore::StepHandler::Base.list_registered_handlers
  
  puts "Registered Handlers: #{handlers['count']}"
  handlers['handlers'].each do |handler_name|
    puts "  - #{handler_name}"
  end
  
  handlers
end

# Example output:
# Registered Handlers: 3
#   - PaymentProcessingStepHandler
#   - NotificationStepHandler
#   - AuditLogStepHandler
```

### Registry Cleanup

```ruby
class HandlerRegistryManager
  def self.cleanup_test_handlers
    # Get all registered handlers
    handlers = TaskerCore::StepHandler::Base.list_registered_handlers
    
    # Filter test handlers
    test_handlers = handlers['handlers'].select do |handler|
      handler.include?('Test') || handler.include?('Mock')
    end
    
    # Unregister test handlers
    test_handlers.each do |handler|
      TaskerCore.unregister_step_handler(handler)
      puts "Unregistered test handler: #{handler}"
    end
  end
end
```

## Task Template Integration

### YAML Task Configuration

Task templates define which handlers to use for each step:

```yaml
# config/tasks/payment_workflow.yaml
name: payment_workflow
version: "1.0.0"
description: "Payment processing workflow"

step_templates:
  - name: payment_processing
    description: "Process customer payment"
    handler_class: PaymentProcessingStepHandler
    depends_on_steps: []
    default_retryable: true
    default_retry_limit: 3
    timeout_seconds: 30
    handler_config:
      max_amount: 10000
      payment_gateway: "stripe"
      
  - name: send_confirmation
    description: "Send payment confirmation"
    handler_class: NotificationStepHandler
    depends_on_steps: ["payment_processing"]
    default_retryable: true
    default_retry_limit: 5
    timeout_seconds: 15
    handler_config:
      notification_type: "email"
      template: "payment_confirmation"
```

### TaskConfigFinder Integration

The TaskConfigFinder loads task templates and uses the registry to resolve handlers:

```ruby
class TaskExecutor
  def initialize
    @config_finder = TaskerCore::TaskConfigFinder.new
    @registry = TaskerCore::TaskHandlerRegistry.global
  end

  def execute_task(task_name, task_version = "1.0.0")
    # Load task template
    template = @config_finder.find_task_template(
      "default",  # namespace
      task_name,
      task_version
    )
    
    # Execute each step
    template.step_templates.each do |step_template|
      handler = @registry.get_handler(step_template.handler_class)
      
      if handler
        puts "Executing step: #{step_template.name}"
        # Execute step with handler
      else
        puts "Handler not found: #{step_template.handler_class}"
      end
    end
  end
end
```

## Error Handling

### Registration Errors

```ruby
class RobustStepHandler < TaskerCore::StepHandler::Base
  def initialize(config: {}, logger: nil)
    super(config: config, logger: logger)
    
    # Verify registration succeeded
    unless registered?
      @logger.error "Handler registration failed"
      raise TaskerCore::RegistrationError, "Failed to register #{self.class.name}"
    end
  end

  def register_with_orchestration_system
    begin
      result = super
      
      # Additional validation
      if result['status'] != 'registered'
        @logger.error "Registration returned error: #{result['error']}"
        
        # Retry registration once
        sleep(1)
        result = super
      end
      
      result
    rescue => e
      @logger.error "Registration exception: #{e.message}"
      { 'status' => 'error', 'error' => e.message }
    end
  end
end
```

### Handler Discovery Errors

```ruby
class SafeTaskExecutor
  def find_handler(handler_class)
    result = TaskerCore.get_step_handler(handler_class)
    
    if result['exists']
      result['handler']
    else
      @logger.warn "Handler not found: #{handler_class}"
      
      # Try to instantiate handler dynamically
      begin
        handler_class_obj = Object.const_get(handler_class)
        handler = handler_class_obj.new
        
        if handler.registered?
          @logger.info "Handler registered dynamically: #{handler_class}"
          handler
        else
          nil
        end
      rescue => e
        @logger.error "Failed to instantiate handler: #{e.message}"
        nil
      end
    end
  end
end
```

## Best Practices

### 1. Handler Naming Conventions

```ruby
# Good: Clear, descriptive names
class PaymentProcessingStepHandler < TaskerCore::StepHandler::Base
  # Step name: "payment_processing"
end

class EmailNotificationStepHandler < TaskerCore::StepHandler::Base
  # Step name: "email_notification"
end

# Avoid: Generic or ambiguous names
class ProcessorStepHandler < TaskerCore::StepHandler::Base
  # Step name: "processor" (too generic)
end
```

### 2. Configuration Management

```ruby
class ConfigurableStepHandler < TaskerCore::StepHandler::Base
  def initialize(config: {}, logger: nil)
    super(config: config, logger: logger)
    
    # Validate required configuration
    validate_required_config!
    
    # Apply configuration defaults
    apply_config_defaults
  end

  private

  def validate_required_config!
    required_keys = ['api_key', 'endpoint_url', 'timeout']
    missing_keys = required_keys - @config.keys
    
    unless missing_keys.empty?
      raise TaskerCore::ConfigurationError, 
            "Missing required configuration: #{missing_keys.join(', ')}"
    end
  end

  def apply_config_defaults
    @config['timeout'] ||= 30
    @config['retry_limit'] ||= 3
    @config['log_level'] ||= 'info'
  end
end
```

### 3. Handler Lifecycle Management

```ruby
class ManagedStepHandler < TaskerCore::StepHandler::Base
  def initialize(config: {}, logger: nil)
    super(config: config, logger: logger)
    
    # Setup handler resources
    setup_resources
    
    # Register cleanup
    at_exit { cleanup_resources }
  end

  def setup_resources
    @connection_pool = create_connection_pool
    @cache = create_cache_store
    @metrics = create_metrics_collector
  end

  def cleanup_resources
    @connection_pool&.close
    @cache&.clear
    @metrics&.flush
  end

  def process(task, sequence, step)
    # Use managed resources
    with_connection do |conn|
      # Process step
    end
  end

  private

  def with_connection
    conn = @connection_pool.acquire
    yield(conn)
  ensure
    @connection_pool.release(conn) if conn
  end
end
```

### 4. Testing Handler Registration

```ruby
RSpec.describe PaymentProcessingStepHandler do
  let(:handler) { PaymentProcessingStepHandler.new }

  describe 'registration' do
    it 'registers automatically on initialization' do
      expect(handler.registered?).to be true
    end

    it 'has correct step name' do
      expect(handler.extract_step_name_from_class).to eq('payment_processing')
    end

    it 'appears in registry listing' do
      handlers = TaskerCore::StepHandler::Base.list_registered_handlers
      expect(handlers['handlers']).to include('PaymentProcessingStepHandler')
    end
  end

  describe 'capabilities' do
    it 'reports correct capabilities' do
      expect(handler.capabilities).to include('process', 'process_results')
    end

    it 'provides metadata' do
      metadata = handler.metadata
      expect(metadata[:handler_name]).to eq('payment_processing_step_handler')
      expect(metadata[:handler_class]).to eq('PaymentProcessingStepHandler')
    end
  end
end
```

## Migration Guide

### From Legacy Pattern

If you have existing step handlers using the old pattern:

```ruby
# Legacy pattern (before composition)
class OldStepHandler
  def self.process(task, sequence, step)
    # Business logic
  end
end

# Register manually
TaskerCore.register_step_handler(
  OldStepHandler.method(:process),
  "old_step"
)
```

Migrate to the new composition pattern:

```ruby
# New composition pattern
class NewStepHandler < TaskerCore::StepHandler::Base
  def process(task, sequence, step)
    # Same business logic
  end
end

# Registration happens automatically
handler = NewStepHandler.new
```

### Configuration Migration

Update your task configurations to use the new handler class format:

```yaml
# Before
step_templates:
  - name: old_step
    handler_method: "OldStepHandler.process"

# After
step_templates:
  - name: new_step
    handler_class: NewStepHandler
```

## Troubleshooting

### Common Issues

1. **Handler Not Found**
   ```ruby
   # Check if handler is registered
   handler = TaskerCore.get_step_handler("MyStepHandler")
   if handler['exists']
     puts "Handler is registered"
   else
     puts "Handler not found - check registration"
   end
   ```

2. **Registration Failed**
   ```ruby
   # Check for registration errors
   class DebuggingHandler < TaskerCore::StepHandler::Base
     def initialize(config: {}, logger: nil)
       super(config: config, logger: logger)
       
       # Debug registration
       puts "Registration result: #{register_with_orchestration_system}"
     end
   end
   ```

3. **Configuration Issues**
   ```ruby
   # Validate configuration
   handler = MyStepHandler.new(config: { timeout: 30 })
   puts "Configuration valid: #{handler.validate_config(handler.config)}"
   ```

### Debug Commands

```ruby
# List all registered handlers
TaskerCore::StepHandler::Base.list_registered_handlers

# Check specific handler
TaskerCore.get_step_handler("MyStepHandler")

# Get handler metadata
handler = MyStepHandler.new
puts handler.metadata.inspect

# Check capabilities
puts handler.capabilities.inspect
```

## Advanced Usage

### Dynamic Handler Registration

```ruby
class HandlerManager
  def self.register_from_directory(directory)
    Dir.glob("#{directory}/*_step_handler.rb").each do |file|
      require file
      
      # Extract class name from filename
      class_name = File.basename(file, '.rb')
        .split('_')
        .map(&:capitalize)
        .join + 'StepHandler'
      
      # Instantiate and register
      handler_class = Object.const_get(class_name)
      handler = handler_class.new
      
      puts "Registered handler: #{class_name}"
    end
  end
end
```

### Plugin System Integration

```ruby
class PluginHandler < TaskerCore::StepHandler::Base
  def initialize(plugin_name, config: {}, logger: nil)
    @plugin_name = plugin_name
    super(config: config, logger: logger)
    
    # Load plugin-specific logic
    load_plugin_logic
  end

  def extract_step_name_from_class
    @plugin_name.underscore
  end

  private

  def load_plugin_logic
    plugin_path = "plugins/#{@plugin_name}/step_handler.rb"
    if File.exist?(plugin_path)
      require plugin_path
      extend Object.const_get("#{@plugin_name.camelize}Plugin")
    end
  end
end
```

This guide provides comprehensive documentation for working with the new TaskHandlerRegistry system. The composition-based architecture provides a clean, testable, and maintainable approach to handler registration while maintaining backward compatibility with existing patterns.