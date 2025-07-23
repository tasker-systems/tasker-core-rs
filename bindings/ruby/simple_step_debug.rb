#!/usr/bin/env ruby

require_relative 'lib/tasker_core'
require 'yaml'

puts "=== Simple Step Debug ===" 

# Set test environment with minimal logging
ENV['TASKER_ENV'] = 'test'
ENV['RUST_LOG'] = 'error'

# Load handler classes
require_relative 'spec/handlers/examples/order_fulfillment/handlers/order_fulfillment_handler'
require_relative 'spec/handlers/examples/order_fulfillment/step_handlers/validate_order_handler'
require_relative 'spec/handlers/examples/order_fulfillment/step_handlers/reserve_inventory_handler'
require_relative 'spec/handlers/examples/order_fulfillment/step_handlers/process_payment_handler'
require_relative 'spec/handlers/examples/order_fulfillment/step_handlers/ship_order_handler'

# Register handler
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

puts "\n2. Checking viable steps..."
begin
  viable_steps = TaskerCore::Performance.viable_steps(task_id)
  puts "   Found #{viable_steps.length} viable steps:"
  
  if viable_steps.length > 0
    viable_steps.each do |step|
      puts "   - #{step['name']}: ready=#{step['is_ready']}, reason=#{step['readiness_reason']}"
    end
  else
    puts "   No viable steps found!"
  end
rescue => e
  puts "   Error: #{e.message}"
end

puts "\n3. Checking task execution context..."
begin
  context = TaskerCore::Performance.task_execution_context(task_id)
  puts "   Task state: #{context.task_state}"
  puts "   Ready steps: #{context.ready_steps}"
  puts "   Has ready steps: #{context.has_ready_steps?}"
  puts "   Total steps: #{context.total_steps}"
  puts "   Completed steps: #{context.completed_steps}"
rescue => e
  puts "   Error: #{e.message}"
end

puts "\n4. Direct database check of workflow steps..."
begin
  require 'pg'
  conn = PG.connect(ENV['DATABASE_URL'] || 'postgresql://tasker:tasker@localhost/tasker_rust_test')
  
  result = conn.exec_params("
    SELECT 
      ws.workflow_step_id,
      ns.name,
      COALESCE(current_state.to_state, 'pending') as current_state,
      ws.attempts,
      ws.retry_limit,
      ws.retryable,
      ws.processed,
      ws.in_process
    FROM tasker_workflow_steps ws
    JOIN tasker_named_steps ns ON ns.named_step_id = ws.named_step_id
    LEFT JOIN tasker_workflow_step_transitions current_state
      ON current_state.workflow_step_id = ws.workflow_step_id
      AND current_state.most_recent = true
    WHERE ws.task_id = $1
    ORDER BY ws.workflow_step_id
  ", [task_id])
  
  puts "   Found #{result.ntuples} workflow steps:"
  result.each do |row|
    puts "   - #{row['name']}: state=#{row['current_state']}, attempts=#{row['attempts']}, retry_limit=#{row['retry_limit']}, retryable=#{row['retryable']}, processed=#{row['processed']}, in_process=#{row['in_process']}"
  end
  
  conn.close
rescue => e
  puts "   Database error: #{e.message}"
end

puts "\n=== Debug Complete ==="