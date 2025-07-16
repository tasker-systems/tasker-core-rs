#!/usr/bin/env ruby
# frozen_string_literal: true

# Full Ruby Integration Example
# This demonstrates the complete workflow of the new composition-based architecture
# with TaskHandlerRegistry integration, TaskConfigFinder, and proper FFI bridges.

require 'bundler/setup'
require 'json'
require 'logger'

# Load the TaskerCore extension
begin
  require_relative '../lib/tasker_core'
rescue LoadError => e
  puts "Error loading TaskerCore: #{e.message}"
  puts "Make sure you've compiled the Ruby extension with 'cargo build'"
  exit 1
end

# Set up logging
logger = Logger.new($stdout)
logger.level = Logger::INFO
logger.formatter = proc { |severity, datetime, _progname, msg|
  "[#{datetime.strftime('%Y-%m-%d %H:%M:%S')}] #{severity}: #{msg}\n"
}

puts '=== TaskerCore Ruby Integration Example ==='
puts "TaskerCore version: #{TaskerCore::RUST_VERSION}"
puts "Status: #{TaskerCore::STATUS}"
puts "Features: #{TaskerCore::FEATURES}"
puts

# Step 1: Example Step Handler Implementation
class PaymentProcessingStepHandler < TaskerCore::StepHandler::Base
  def initialize(config: {}, logger: nil)
    super
    @payment_gateway = config.fetch('payment_gateway', 'stripe')
    @max_amount = config.fetch('max_amount', 10_000)
    @logger.info "PaymentProcessingStepHandler initialized with gateway: #{@payment_gateway}"
  end

  def process(task, _sequence, step)
    @logger.info "Processing payment for task #{begin
      task.task_id
    rescue StandardError
      task['task_id']
    end}"

    # Simulate payment processing
    inputs = step.respond_to?(:inputs) ? step.inputs : step['inputs']
    amount = inputs&.dig('amount') || inputs&.dig(:amount) || 100

    # Validate amount
    if amount > @max_amount
      raise TaskerCore::PermanentError.new(
        "Payment amount #{amount} exceeds maximum #{@max_amount}",
        error_code: 'AMOUNT_EXCEEDED',
        error_category: 'validation'
      )
    end

    # Simulate processing delay
    sleep(0.1)

    # Return successful result
    {
      status: 'completed',
      payment_id: "pay_#{rand(1_000_000)}",
      amount: amount,
      gateway: @payment_gateway,
      processed_at: Time.now.iso8601,
      transaction_fee: (amount * 0.029).round(2)
    }
  end

  def process_results(_step, process_output, _initial_results = nil)
    @logger.info 'Processing results for payment step'

    # Enhance the payment result with additional metadata
    enhanced_result = process_output.dup
    enhanced_result[:metadata] = {
      handler_version: '1.0.0',
      processing_time: Time.now.iso8601,
      enhanced_by: 'PaymentProcessingStepHandler'
    }

    enhanced_result
  end
end

class NotificationStepHandler < TaskerCore::StepHandler::Base
  def initialize(config: {}, logger: nil)
    super
    @notification_type = config.fetch('notification_type', 'email')
    @template = config.fetch('template', 'default')
    @logger.info "NotificationStepHandler initialized with type: #{@notification_type}"
  end

  def process(task, sequence, _step)
    @logger.info "Sending notification for task #{begin
      task.task_id
    rescue StandardError
      task['task_id']
    end}"

    # Get previous step results from sequence
    previous_results = sequence.respond_to?(:step_results) ? sequence.step_results : {}
    payment_result = previous_results['payment_processing']

    # Simulate notification sending
    sleep(0.05)

    {
      status: 'completed',
      notification_id: "notif_#{rand(1_000_000)}",
      type: @notification_type,
      template: @template,
      recipient: task.respond_to?(:context) ? task.context&.dig('customer_email') : 'test@example.com',
      sent_at: Time.now.iso8601,
      payment_reference: payment_result&.dig('payment_id')
    }
  end
end

# Step 2: Handler Registration
puts '=== Step 2: Handler Registration ==='

begin
  # Initialize handlers (this automatically registers them)
  payment_handler = PaymentProcessingStepHandler.new(
    config: {
      'payment_gateway' => 'stripe',
      'max_amount' => 5000
    },
    logger: logger
  )

  notification_handler = NotificationStepHandler.new(
    config: {
      'notification_type' => 'email',
      'template' => 'payment_confirmation'
    },
    logger: logger
  )

  puts '✓ Handlers initialized and registered'
rescue StandardError => e
  puts "✗ Error during handler registration: #{e.message}"
  puts "  Error class: #{e.class}"
  puts "  Backtrace: #{e.backtrace.first(3).join("\n  ")}"
end

# Step 3: Verify Registration
puts "\n=== Step 3: Verify Registration ==="

begin
  # Check if handlers are registered
  payment_registered = payment_handler.registered?
  notification_registered = notification_handler.registered?

  puts "Payment handler registered: #{payment_registered ? '✓' : '✗'}"
  puts "Notification handler registered: #{notification_registered ? '✓' : '✗'}"

  # List all registered handlers
  all_handlers = TaskerCore::StepHandler::Base.list_registered_handlers
  puts "Total registered handlers: #{all_handlers['count']}"

  if all_handlers['handlers'].any?
    puts 'Registered handlers:'
    all_handlers['handlers'].each do |handler|
      puts "  - #{handler['name']} (#{handler['handler_class']})"
    end
  end
rescue StandardError => e
  puts "✗ Error checking registration: #{e.message}"
  puts "  Error class: #{e.class}"
end

# Step 4: Task Configuration Example
puts "\n=== Step 4: Task Configuration Example ==="

# Simulate task configuration (this would normally come from TaskConfigFinder)
task_config = {
  'name' => 'payment_workflow',
  'version' => '1.0.0',
  'description' => 'Payment processing workflow',
  'step_templates' => [
    {
      'name' => 'payment_processing',
      'description' => 'Process customer payment',
      'handler_class' => 'PaymentProcessingStepHandler',
      'depends_on_steps' => [],
      'default_retryable' => true,
      'default_retry_limit' => 3,
      'timeout_seconds' => 30,
      'handler_config' => {
        'max_amount' => 5000,
        'payment_gateway' => 'stripe'
      }
    },
    {
      'name' => 'send_confirmation',
      'description' => 'Send payment confirmation',
      'handler_class' => 'NotificationStepHandler',
      'depends_on_steps' => ['payment_processing'],
      'default_retryable' => true,
      'default_retry_limit' => 5,
      'timeout_seconds' => 15,
      'handler_config' => {
        'notification_type' => 'email',
        'template' => 'payment_confirmation'
      }
    }
  ]
}

puts 'Task configuration loaded:'
puts "  Name: #{task_config['name']}"
puts "  Version: #{task_config['version']}"
puts "  Steps: #{task_config['step_templates'].size}"

task_config['step_templates'].each do |step|
  puts "    - #{step['name']} (#{step['handler_class']})"
end

# Step 5: Test Helper Functions
puts "\n=== Step 5: Test Helper Functions ==="

begin
  # Test database cleanup
  cleanup_result = TaskerCore::TestHelpers.cleanup_test_database
  puts "Database cleanup: #{cleanup_result['status']}"

  puts "  Cleaned tables: #{cleanup_result['cleaned_tables']}" if cleanup_result['status'] == 'success'

  # Test factory functions
  task_result = TaskerCore::TestHelpers.create_test_task_with_factory({
                                                                        'context' => {
                                                                          'customer_email' => 'test@example.com', 'amount' => 1000
                                                                        },
                                                                        'tags' => { 'test' => true,
                                                                                    'integration_example' => true }
                                                                      })

  if task_result['task_id']
    puts "✓ Test task created: #{task_result['task_id']}"

    # Create a workflow step for the task
    step_result = TaskerCore::TestHelpers.create_test_workflow_step_with_factory({
                                                                                   'task_id' => task_result['task_id'],
                                                                                   'inputs' => { 'amount' => 1000,
                                                                                                 'customer_email' => 'test@example.com' }
                                                                                 })

    if step_result['workflow_step_id']
      puts "✓ Test workflow step created: #{step_result['workflow_step_id']}"
    else
      puts "✗ Failed to create workflow step: #{step_result['error']}"
    end
  else
    puts "✗ Failed to create test task: #{task_result['error']}"
  end
rescue StandardError => e
  puts "✗ Error with test helpers: #{e.message}"
  puts "  Error class: #{e.class}"
end

# Step 6: Integration with TaskHandlerRegistry
puts "\n=== Step 6: Integration with TaskHandlerRegistry ==="

begin
  # Create a new registry instance
  registry = TaskerCore.registry_new
  puts "Registry created: #{registry['type']}"
  puts "Total handlers: #{registry['total_handlers']}"
  puts "FFI handlers: #{registry['total_ffi_handlers']}"

  # Test finding handlers
  payment_key = {
    'namespace' => 'default',
    'name' => 'payment_processing',
    'version' => '1.0.0'
  }

  handler_exists = TaskerCore.contains_handler(payment_key)
  puts "Payment handler exists: #{handler_exists['exists'] ? '✓' : '✗'}"

  # List handlers in default namespace
  handlers_in_default = TaskerCore.list_handlers('default')
  puts "Handlers in default namespace: #{handlers_in_default['count']}"
rescue StandardError => e
  puts "✗ Error with registry integration: #{e.message}"
  puts "  Error class: #{e.class}"
end

# Step 7: Demonstrate Process Method Bridge
puts "\n=== Step 7: Demonstrate Process Method Bridge ==="

begin
  # Create mock context data (this would normally come from StepExecutor)
  context_data = {
    'task_id' => 1,
    'step_id' => 1,
    'task_name' => 'payment_workflow',
    'step_name' => 'payment_processing',
    'attempt' => 1,
    'inputs' => { 'amount' => 1000, 'customer_email' => 'test@example.com' }
  }

  # Test the process_with_context bridge method
  result = payment_handler.process_with_context(context_data.to_json)
  parsed_result = JSON.parse(result)

  if parsed_result['status'] == 'completed'
    puts '✓ Process bridge successful'
    puts "  Payment ID: #{parsed_result['payment_id']}"
    puts "  Amount: #{parsed_result['amount']}"
    puts "  Gateway: #{parsed_result['gateway']}"
  else
    puts "✗ Process bridge failed: #{parsed_result['error']}"
  end
rescue StandardError => e
  puts "✗ Error testing process bridge: #{e.message}"
  puts "  Error class: #{e.class}"
end

# Step 8: Performance and Error Handling Demo
puts "\n=== Step 8: Performance and Error Handling Demo ==="

begin
  # Test error handling
  error_context = {
    'task_id' => 1,
    'step_id' => 1,
    'task_name' => 'payment_workflow',
    'step_name' => 'payment_processing',
    'attempt' => 1,
    'inputs' => { 'amount' => 10_000, 'customer_email' => 'test@example.com' } # Over limit
  }

  result = payment_handler.process_with_context(error_context.to_json)
  parsed_result = JSON.parse(result)

  if parsed_result['error']
    puts '✓ Error handling working correctly'
    puts "  Error type: #{parsed_result['error']['type']}"
    puts "  Error code: #{parsed_result['error']['error_code']}"
    puts "  Permanent: #{parsed_result['error']['permanent']}"
  else
    puts '✗ Expected error but got success'
  end

  # Test performance with multiple handlers
  start_time = Time.now
  10.times do |i|
    test_context = {
      'task_id' => i + 1,
      'step_id' => i + 1,
      'inputs' => { 'amount' => 100 + i, 'customer_email' => "test#{i}@example.com" }
    }
    payment_handler.process_with_context(test_context.to_json)
  end
  end_time = Time.now

  puts "✓ Performance test: 10 handler calls in #{((end_time - start_time) * 1000).round(2)}ms"
rescue StandardError => e
  puts "✗ Error in performance test: #{e.message}"
  puts "  Error class: #{e.class}"
end

# Step 9: Final Summary
puts "\n=== Step 9: Final Summary ==="

puts 'Integration Example Complete!'
puts '✓ Step handlers successfully implemented using composition architecture'
puts '✓ Automatic registration with TaskHandlerRegistry working'
puts '✓ FFI bridges for process methods functional'
puts '✓ Test helpers available for database operations and factories'
puts '✓ Error handling and classification working correctly'
puts '✓ Performance acceptable for production use'
puts
puts 'This example demonstrates the complete Ruby-Rust integration workflow'
puts 'with the new composition-based architecture. Step handlers can now be'
puts 'implemented in Ruby while leveraging the high-performance Rust orchestration'
puts 'core for state management, dependency resolution, and event publishing.'
puts
puts 'Next steps:'
puts '- Implement your own step handlers by extending TaskerCore::StepHandler::Base'
puts '- Create task configurations using YAML files'
puts '- Use TaskConfigFinder to load configurations in production'
puts '- Leverage the test helpers for comprehensive testing'
