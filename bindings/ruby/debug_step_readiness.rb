#!/usr/bin/env ruby

require_relative 'lib/tasker_core'
require 'yaml'

puts "=== Debug Step Readiness ==="

# Set test environment
ENV['TASKER_ENV'] = 'test'

# Load the actual handler classes
require_relative 'spec/handlers/examples/order_fulfillment/handlers/order_fulfillment_handler'
require_relative 'spec/handlers/examples/order_fulfillment/step_handlers/validate_order_handler'
require_relative 'spec/handlers/examples/order_fulfillment/step_handlers/reserve_inventory_handler'
require_relative 'spec/handlers/examples/order_fulfillment/step_handlers/process_payment_handler'
require_relative 'spec/handlers/examples/order_fulfillment/step_handlers/ship_order_handler'

# Register handler with correct config
config_path = File.expand_path('spec/handlers/examples/order_fulfillment/config/order_fulfillment_handler.yaml', __dir__)
config = YAML.load_file(config_path)

TaskerCore::Registry.register(
  namespace: config['namespace_name'],
  name: config['name'],
  version: config['version'],
  handler_class: config['task_handler_class'],
  config_schema: config
)

handler_result = TaskerCore::Registry.find_handler_and_initialize(
  name: "fulfillment/process_order",
  version: "1.0.0",
  config_path: config_path
)

handler_instance = handler_result['handler_instance']

# Create test task
task_request = TaskerCore::Types::TaskRequest.build_test(
  namespace: "fulfillment",
  name: "process_order", 
  version: "1.0.0",
  context: {
    'customer_info' => { 'id' => 12345, 'email' => 'test@debug.com', 'tier' => 'standard' },
    'order_items' => [{ 'product_id' => 101, 'quantity' => 1, 'price' => 29.99 }],
    'payment_info' => { 'method' => 'credit_card', 'token' => 'tok_debug', 'amount' => 29.99 },
    'shipping_info' => { 'address' => '123 Debug St', 'method' => 'standard' }
  },
  initiator: "debug_script",
  source_system: "debug",
  reason: "debugging step readiness",
  tags: ["debug"]
)

puts "1. Initializing task..."
init_result = handler_instance.initialize_task(task_request)
task_id = init_result.task_id
puts "   Task ID: #{task_id}"
puts "   Success: #{init_result.success?}"
puts "   Step count: #{init_result.step_count}"

# Check step readiness using Performance.viable_steps
puts "\n2. Checking step readiness status..."

begin
  # Use the correct method - Performance.viable_steps
  step_readiness = TaskerCore::Performance.viable_steps(task_id)
  
  puts "   Found #{step_readiness.length} viable steps:"
  step_readiness.each do |step|
    puts "   - #{step['name']}: ready=#{step['is_ready']}, reason=#{step['readiness_reason']}, state=#{step['current_state']}"
  end
rescue => e
  puts "   Error checking step readiness: #{e.message}"
  puts "   #{e.backtrace.first}"
end

# Also check task execution context for broader view
puts "\n3. Checking task execution context..."
begin
  context = TaskerCore::Performance.task_execution_context(task_id)
  puts "   Task state: #{context.task_state}"
  puts "   Has ready steps: #{context.has_ready_steps?}"
  puts "   Ready steps count: #{context.ready_steps}"
  puts "   Total steps: #{context.total_steps}"
  puts "   Completed steps: #{context.completed_steps}"
rescue => e
  puts "   Error checking task context: #{e.message}"
  puts "   #{e.backtrace.first}"
end

puts "\n=== Debug Complete ==="