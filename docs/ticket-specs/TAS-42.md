# TAS-42: Ruby Binding Simplification

## Executive Summary

Transform the Ruby bindings (`workers/ruby/`) from a complex infrastructure-heavy implementation to a simplified, pure business logic system that leverages the worker foundation from TAS-40. This involves removing 70%+ of the current Ruby codebase while maintaining complete compatibility with existing step handlers and workflows.

## Context and Dependencies

### Prerequisites
- **TAS-40 Complete**: Worker foundation (`tasker-worker-foundation/`) fully implemented
- **TAS-41 Validated**: Pure Rust worker proves foundation works correctly
- **FFI Bridge**: Ruby FFI bridge component of worker foundation functional

### Migration Strategy
- **Backward Compatibility**: Existing step handlers continue to work unchanged
- **Gradual Migration**: Phased approach allowing rollback at any stage
- **Zero Downtime**: Migration possible without service interruption
- **Configuration Preserved**: Existing task template YAML files remain compatible

## Current Ruby Implementation Analysis

### Infrastructure Code to Remove (70%+ of codebase)

#### Database Connection Management
```ruby
# REMOVE: workers/ruby/lib/tasker_core/database/
- connection.rb              # ActiveRecord connection management
- connection_pool.rb         # Thread-safe connection pooling
- migration_manager.rb       # Database migration handling
- models/                    # Database model definitions (moving to shared)
```

#### Queue Worker Threading
```ruby
# REMOVE: workers/ruby/lib/tasker_core/messaging/
- worker_threads.rb          # Concurrent Ruby threading
- queue_manager.rb           # Queue polling and management
- message_processor.rb       # Message parsing and claiming
- pgmq_client.rb            # PGMQ client implementation
```

#### State Management Infrastructure
```ruby
# REMOVE: workers/ruby/lib/tasker_core/orchestration/
- task_state_manager.rb      # Task state transitions
- step_claimer.rb           # Step claiming logic
- result_persister.rb       # Result persistence
- finalization_handler.rb   # Task finalization logic
```

#### Resource Management
```ruby
# REMOVE: workers/ruby/lib/tasker_core/system/
- resource_validator.rb      # Resource validation
- health_monitor.rb         # Health monitoring
- metrics_collector.rb      # Metrics collection
- graceful_shutdown.rb      # Shutdown coordination
```

### Business Logic to Preserve

#### Step Handler Framework
```ruby
# KEEP: workers/ruby/lib/tasker_core/handlers/
- base_step_handler.rb       # Step handler base class
- handler_registry.rb       # Handler registration (simplified)
- execution_context.rb      # Step execution context
```

#### Workflow Examples
```ruby
# KEEP: workers/ruby/spec/handlers/examples/
- All workflow step handlers  # Pure business logic
- All task template YAML files # Configuration unchanged
```

#### Domain Types
```ruby
# KEEP: workers/ruby/lib/tasker_core/types/
- step_handler_call_result.rb # Result types
- execution_error.rb         # Error types (simplified)
- sequence.rb               # Sequence data structure
```

## Implementation Plan

### Phase 1: Foundation Integration Setup (Week 7.1)

#### 1.1 Create New Simplified Architecture
```ruby
# workers/ruby/lib/tasker_core/worker/event_bridge.rb
module TaskerCore
  module Worker
    class EventBridge
      include Dry::Events::Publisher[:worker_events]

      def self.setup!
        # Subscribe to step execution events from Rust foundation
        register_event('step.execute')
        register_event('step.complete')
        register_event('step.error')

        # Set up event handlers
        subscribe('step.execute', StepExecutionHandler.new)

        TaskerCore.logger.info "🔗 Ruby EventBridge initialized"
      end

      # Called by Rust foundation via FFI
      def self.execute_step_via_ffi(step_data)
        event_data = StepExecutionData.from_ffi(step_data)
        publish('step.execute', event_data)
      end

      # Callback to Rust foundation
      def self.step_completed(result)
        # Convert Ruby result to FFI-compatible format
        ffi_result = result.to_ffi_format

        # Call back to Rust foundation (via Magnus FFI)
        TaskerCore::FFI.step_execution_completed(ffi_result)
      end

      def self.step_failed(error)
        # Convert Ruby error to FFI-compatible format
        ffi_error = error.to_ffi_format

        # Call back to Rust foundation
        TaskerCore::FFI.step_execution_failed(ffi_error)
      end
    end
  end
end
```

#### 1.2 Simplified Step Handler Base Class
```ruby
# workers/ruby/lib/tasker_core/handlers/base_step_handler.rb (SIMPLIFIED)
module TaskerCore
  module Handlers
    class BaseStepHandler
      # Removed: Database connection management
      # Removed: Queue handling
      # Removed: State transition logic
      # Removed: Resource management

      def initialize
        # Minimal initialization - no infrastructure concerns
      end

      # Main execution method - pure business logic only
      def call(task, sequence, step)
        raise NotImplementedError, "Subclasses must implement #call"
      end

      protected

      # Simplified result creation
      def success(data = {})
        TaskerCore::Types::StepHandlerCallResult.success(data)
      end

      def retryable_error(message, details = {})
        TaskerCore::Types::StepHandlerCallResult.retryable_error(message, details)
      end

      def permanent_error(message, details = {})
        TaskerCore::Types::StepHandlerCallResult.permanent_error(message, details)
      end

      # Access sequence results (no database queries)
      def sequence_results(sequence, step_name)
        sequence.get_results(step_name)
      end
    end
  end
end
```

#### 1.3 Event-Driven Step Execution Handler
```ruby
# workers/ruby/lib/tasker_core/worker/step_execution_handler.rb
module TaskerCore
  module Worker
    class StepExecutionHandler
      def initialize
        @handler_registry = TaskerCore::Registry.step_handler_registry
      end

      def call(event)
        step_data = event.payload

        begin
          # Resolve step handler (no database lookup)
          handler = @handler_registry.resolve_step_handler(step_data.step_name)

          unless handler
            return EventBridge.step_failed(
              ExecutionError.new("No handler found for step: #{step_data.step_name}")
            )
          end

          # Execute pure business logic
          result = handler.call(
            step_data.task,
            step_data.sequence,
            step_data.step
          )

          # Send result back to foundation
          EventBridge.step_completed(result)

        rescue => error
          # Convert any Ruby exception to structured error
          execution_error = ExecutionError.from_exception(error)
          EventBridge.step_failed(execution_error)
        end
      end
    end
  end
end
```

### Phase 2: Remove Infrastructure Components (Week 7.2)

#### 2.1 Database Model Migration to Shared Workspace
```bash
# Move database models to shared workspace
mv workers/ruby/lib/tasker_core/database/models/* ../tasker-shared/src/models/ruby/

# Remove database infrastructure
rm -rf workers/ruby/lib/tasker_core/database/
rm -rf workers/ruby/lib/tasker_core/messaging/pgmq_client.rb
rm -rf workers/ruby/lib/tasker_core/orchestration/
```

#### 2.2 Simplified Configuration System
```ruby
# workers/ruby/lib/tasker_core/config/simple_config.rb (NEW)
module TaskerCore
  class SimpleConfig
    # Removed: Complex TOML configuration management
    # Removed: Database configuration
    # Removed: Queue configuration
    # Removed: Resource limits

    attr_reader :task_templates_path, :handler_namespace

    def initialize
      @task_templates_path = ENV['TASKER_TASK_TEMPLATES_PATH'] ||
                           File.join(Dir.pwd, 'config', 'tasks')
      @handler_namespace = ENV['TASKER_HANDLER_NAMESPACE'] || 'TaskerCore::Handlers'

      # Worker foundation handles all other configuration
    end

    def task_template_configs
      Dir.glob(File.join(@task_templates_path, '**', '*.yaml')).map do |file|
        YAML.load_file(file)
      end
    end
  end
end
```

#### 2.3 Simplified Handler Registry
```ruby
# workers/ruby/lib/tasker_core/registry/step_handler_registry.rb (SIMPLIFIED)
module TaskerCore
  module Registry
    class StepHandlerRegistry
      # Removed: Database persistence
      # Removed: Cache management
      # Removed: Template loading from database

      def initialize
        @handlers = {}
        @config = SimpleConfig.new
        load_handlers_from_task_templates
      end

      def resolve_step_handler(step_name)
        handler_class = @handlers[step_name]
        return nil unless handler_class

        # Simple instantiation - no infrastructure setup
        handler_class.new
      end

      private

      def load_handlers_from_task_templates
        @config.task_template_configs.each do |template|
          template['step_templates']&.each do |step_template|
            step_name = step_template['name']
            handler_class_name = step_template['handler_class']

            # Resolve handler class
            handler_class = resolve_handler_class(handler_class_name)
            @handlers[step_name] = handler_class if handler_class
          end
        end
      end

      def resolve_handler_class(class_name)
        # Simple class resolution
        Object.const_get("#{@config.handler_namespace}::#{class_name}")
      rescue NameError
        nil
      end
    end
  end
end
```

### Phase 3: FFI Integration Layer (Week 7.3)

#### 3.1 Magnus FFI Bridge Interface
```ruby
# workers/ruby/ext/tasker_core/src/lib.rs (UPDATED)
use magnus::{Error, Ruby};

// Simplified FFI interface - no infrastructure management
#[magnus::init]
fn init(ruby: &Ruby) -> Result<(), Error> {
    // Register FFI callbacks for step execution
    let module = ruby.define_module("TaskerCore")?;
    let ffi_module = module.define_module("FFI")?;

    // Method called by worker foundation to execute Ruby step
    ffi_module.define_singleton_method("execute_step", execute_step_ffi)?;

    // Method called by Ruby to return step result to foundation
    ffi_module.define_singleton_method("step_execution_completed", step_completed_callback)?;
    ffi_module.define_singleton_method("step_execution_failed", step_failed_callback)?;

    Ok(())
}

fn execute_step_ffi(ruby: &Ruby, step_data: Value) -> Result<(), Error> {
    // Convert Rust data to Ruby format
    let ruby_step_data = convert_ffi_to_ruby(step_data)?;

    // Call Ruby event system
    let event_bridge = ruby.eval("TaskerCore::Worker::EventBridge")?;
    event_bridge.funcall("execute_step_via_ffi", (ruby_step_data,))?;

    Ok(())
}

fn step_completed_callback(result: Value) -> Result<(), Error> {
    // Convert Ruby result back to Rust format
    let rust_result = convert_ruby_to_ffi(result)?;

    // Call worker foundation callback
    // This would call into the worker foundation Rust code
    worker_foundation_step_completed(rust_result);

    Ok(())
}
```

#### 3.2 Data Type Conversions
```ruby
# workers/ruby/lib/tasker_core/types/ffi_conversion.rb (NEW)
module TaskerCore
  module Types
    class StepExecutionData
      attr_reader :step_name, :task, :sequence, :step

      def self.from_ffi(ffi_data)
        # Convert FFI data structure to Ruby objects
        new(
          step_name: ffi_data['step_name'],
          task: Task.from_hash(ffi_data['task']),
          sequence: Sequence.from_hash(ffi_data['sequence']),
          step: Step.from_hash(ffi_data['step'])
        )
      end

      def initialize(step_name:, task:, sequence:, step:)
        @step_name = step_name
        @task = task
        @sequence = sequence
        @step = step
      end
    end

    class StepHandlerCallResult
      # Enhanced to support FFI conversion
      def to_ffi_format
        {
          'success' => @success,
          'data' => @data,
          'error' => @error&.to_hash,
          'metadata' => {
            'executed_at' => Time.now.iso8601,
            'execution_environment' => 'ruby',
            'duration_ms' => @duration_ms
          }
        }
      end
    end
  end
end
```

### Phase 4: Workflow Migration (Week 7.4)

#### 4.1 Migrate Existing Step Handlers
The existing step handlers need minimal changes:

```ruby
# workers/ruby/spec/handlers/examples/linear_workflow/step_handlers/linear_step_1_handler.rb
# BEFORE (infrastructure-heavy):
class LinearStep1Handler < TaskerCore::BaseTaskHandler
  def call(task, sequence, step)
    # OLD: Database queries, state management, etc.
    even_number = task.context['even_number'] || 6

    # Complex infrastructure code removed...

    success(generated_value: even_number * 2)
  end
end

# AFTER (pure business logic):
module TaskerCore::Handlers
  class LinearStep1Handler < BaseStepHandler
    def call(task, sequence, step)
      even_number = task.context['even_number'] || 6

      success({
        generated_value: even_number * 2,
        step_1_timestamp: Time.now.iso8601,
        processing_info: 'Generated by Ruby linear step 1'
      })
    end
  end
end
```

#### 4.2 Workflow Test Updates
```ruby
# workers/ruby/spec/integration/simplified_workflow_spec.rb
RSpec.describe 'Simplified Ruby Worker Integration' do
  before(:each) do
    # Setup simplified worker (no infrastructure setup needed)
    TaskerCore::Worker::EventBridge.setup!
  end

  it 'executes linear workflow through event system' do
    # Simulate step execution event from worker foundation
    step_data = TaskerCore::Types::StepExecutionData.new(
      step_name: 'linear_step_1',
      task: create_test_task,
      sequence: create_test_sequence,
      step: create_test_step
    )

    # Publish event and verify handler execution
    expect {
      TaskerCore::Worker::EventBridge.publish('step.execute', step_data)
    }.not_to raise_error

    # Verify result was sent back to foundation
    expect(last_ffi_callback).to have_received(:step_execution_completed)
  end
end
```

### Phase 5: Configuration and Deployment (Week 7.5)

#### 5.1 Simplified Gemfile
```ruby
# workers/ruby/Gemfile (SIMPLIFIED)
source 'https://rubygems.org'

# Removed: Database gems (pg, activerecord, etc.)
# Removed: Queue gems (pgmq bindings, etc.)
# Removed: Threading gems (concurrent-ruby, etc.)

# Keep only business logic dependencies
gem 'dry-events', '~> 1.0'
gem 'dry-struct', '~> 1.6'
gem 'dry-types', '~> 1.7'

# Testing
group :test do
  gem 'rspec', '~> 3.12'
  gem 'factory_bot', '~> 6.4'
end

# Development
group :development do
  gem 'rubocop', '~> 1.60'
  gem 'ruby-lsp', '~> 0.13'
end
```

#### 5.2 Simplified Docker Configuration
```dockerfile
# workers/ruby/Dockerfile (SIMPLIFIED)
FROM ruby:3.2-slim

# No longer need: PostgreSQL client, system dependencies, etc.

WORKDIR /app

# Copy only business logic
COPY Gemfile Gemfile.lock ./
RUN bundle install --without development test

COPY lib/ ./lib/
COPY config/ ./config/

# Simple startup - worker foundation handles infrastructure
CMD ["ruby", "-e", "require_relative 'lib/tasker_core'; TaskerCore::Worker::EventBridge.setup!; sleep"]
```

### Phase 6: Migration and Testing (Week 8)

#### 6.1 Migration Script
```ruby
# scripts/migrate_to_simplified_ruby.rb
#!/usr/bin/env ruby

require 'fileutils'

class RubySimplificationMigrator
  def migrate!
    puts "🔄 Starting Ruby simplification migration..."

    # Backup current implementation
    backup_current_implementation

    # Remove infrastructure components
    remove_infrastructure_code

    # Update remaining files
    update_handler_base_classes
    update_step_handlers
    update_configuration

    # Setup new event-driven architecture
    setup_event_bridge

    # Run tests to verify migration
    run_migration_tests

    puts "✅ Ruby simplification migration completed!"
  end

  private

  def backup_current_implementation
    backup_dir = "backup_ruby_implementation_#{Time.now.strftime('%Y%m%d_%H%M%S')}"
    FileUtils.cp_r('workers/ruby', backup_dir)
    puts "📦 Backed up current implementation to #{backup_dir}"
  end

  def remove_infrastructure_code
    infrastructure_dirs = [
      'workers/ruby/lib/tasker_core/database',
      'workers/ruby/lib/tasker_core/messaging',
      'workers/ruby/lib/tasker_core/orchestration',
      'workers/ruby/lib/tasker_core/system'
    ]

    infrastructure_dirs.each do |dir|
      if Dir.exist?(dir)
        FileUtils.rm_rf(dir)
        puts "🗑️  Removed infrastructure directory: #{dir}"
      end
    end
  end

  def update_handler_base_classes
    # Update inheritance from BaseTaskHandler to BaseStepHandler
    Dir.glob('workers/ruby/**/*.rb').each do |file|
      content = File.read(file)
      updated_content = content.gsub(/< TaskerCore::BaseTaskHandler/, '< TaskerCore::Handlers::BaseStepHandler')

      if content != updated_content
        File.write(file, updated_content)
        puts "🔄 Updated base class in #{file}"
      end
    end
  end

  def run_migration_tests
    puts "🧪 Running migration verification tests..."
    system('cd workers/ruby && bundle exec rspec spec/integration/simplified_workflow_spec.rb')
  end
end

# Run migration
RubySimplificationMigrator.new.migrate!
```

#### 6.2 Integration Testing
```ruby
# workers/ruby/spec/integration/migration_verification_spec.rb
RSpec.describe 'Ruby Simplification Migration' do
  describe 'Infrastructure removal verification' do
    it 'removes database connection management' do
      expect(defined?(TaskerCore::Database::Connection)).to be_nil
    end

    it 'removes queue worker threading' do
      expect(defined?(TaskerCore::Messaging::WorkerThreads)).to be_nil
    end

    it 'removes orchestration state management' do
      expect(defined?(TaskerCore::Orchestration::TaskStateManager)).to be_nil
    end
  end

  describe 'Business logic preservation' do
    it 'preserves step handler functionality' do
      handler = TaskerCore::Handlers::LinearStep1Handler.new

      result = handler.call(
        create_test_task,
        create_test_sequence,
        create_test_step
      )

      expect(result).to be_success
      expect(result.data).to include('generated_value')
    end

    it 'preserves workflow execution patterns' do
      # All existing workflow patterns should work unchanged
      workflow_handlers = [
        'LinearStep1Handler',
        'DiamondStartHandler',
        'TreeRootHandler',
        'ValidateOrderHandler'
      ]

      workflow_handlers.each do |handler_name|
        handler_class = "TaskerCore::Handlers::#{handler_name}".constantize
        expect(handler_class.ancestors).to include(TaskerCore::Handlers::BaseStepHandler)
      end
    end
  end

  describe 'Event-driven integration' do
    it 'processes step execution events' do
      TaskerCore::Worker::EventBridge.setup!

      step_data = create_test_step_execution_data

      expect {
        TaskerCore::Worker::EventBridge.publish('step.execute', step_data)
      }.not_to raise_error
    end
  end

  describe 'FFI callback integration' do
    it 'sends results back to worker foundation' do
      # Mock FFI callback
      allow(TaskerCore::FFI).to receive(:step_execution_completed)

      result = TaskerCore::Types::StepHandlerCallResult.success(test: 'data')
      TaskerCore::Worker::EventBridge.step_completed(result)

      expect(TaskerCore::FFI).to have_received(:step_execution_completed)
    end
  end
end
```

## Success Criteria

### Code Reduction Metrics
- ✅ **70%+ Code Removal**: Infrastructure code eliminated from Ruby bindings
- ✅ **Dependency Reduction**: Gemfile reduced from 50+ gems to < 10 gems
- ✅ **File Count**: Ruby file count reduced by 60%+
- ✅ **Binary Size**: Ruby extension binary size reduced by 80%+

### Functional Preservation
- ✅ **Handler Compatibility**: All existing step handlers work unchanged
- ✅ **Workflow Patterns**: All workflow examples (linear, diamond, tree, etc.) execute correctly
- ✅ **Configuration Compatibility**: Existing task template YAML files work unchanged
- ✅ **Test Coverage**: All existing workflow tests pass with new architecture

### Integration Quality
- ✅ **FFI Performance**: < 5% overhead compared to direct Ruby execution
- ✅ **Event System**: Reliable event delivery between Rust foundation and Ruby
- ✅ **Error Handling**: Comprehensive error propagation across FFI boundary
- ✅ **Resource Usage**: 90% reduction in Ruby memory footprint

### Migration Safety
- ✅ **Zero Downtime**: Migration possible without service interruption
- ✅ **Rollback Capability**: Complete rollback possible at any migration stage
- ✅ **Backward Compatibility**: Existing deployments continue working during transition
- ✅ **Documentation**: Complete migration guides and troubleshooting documentation

### Operational Benefits
- ✅ **Simplified Deployment**: Ruby workers no longer need database configuration
- ✅ **Reduced Dependencies**: Eliminated PostgreSQL, queue, and threading dependencies
- ✅ **Faster Startup**: Ruby worker startup time reduced by 80%+
- ✅ **Easier Debugging**: Pure business logic separation simplifies troubleshooting

## Risk Mitigation

### Technical Risks
- **FFI Complexity**: Comprehensive testing of FFI boundary with error scenarios
- **Event System Reliability**: Robust event delivery with timeout and retry mechanisms
- **Data Serialization**: Careful validation of data conversion between Rust and Ruby

### Migration Risks
- **Breaking Changes**: Maintain compatibility layer during transition period
- **Performance Regression**: Comprehensive benchmarking before and after migration
- **Operational Disruption**: Detailed migration runbooks and rollback procedures

This simplified Ruby implementation demonstrates the power of the worker foundation architecture while maintaining full compatibility with existing business logic, achieving the goal of infrastructure-free binding language implementations.
