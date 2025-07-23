#!/usr/bin/env ruby

require_relative 'lib/tasker_core'
require 'json'

# Initialize the orchestration system
puts "Initializing orchestration system..."
handle = TaskerCore.create_orchestration_handle

# Register step handlers
puts "Registering step handlers..."
handle.register_step_handlers

# Create a simple test task
puts "Creating test task..."
factory_options = {
  "task_name" => "OrderFulfillmentTaskHandler",
  "step_count" => 4,
  "pattern" => "linear"
}

result = handle.create_test_task(factory_options)
puts "Created task with ID: #{result.task_id}"

task_id = result.task_id

# Execute the first step with specific results
puts "\nExecuting first step..."
step_handle_result = handle.handle_one_step_by_name(task_id, "validate_order")

puts "Step execution result:"
puts "  Status: #{step_handle_result.status}"
puts "  Step ID: #{step_handle_result.step_id}"

# Check if results were persisted by trying to access them through the next step
puts "\nChecking step results persistence..."
puts "Getting step sequence for second step..."

begin
  next_step_result = handle.handle_one_step_by_name(task_id, "reserve_inventory")
  puts "Next step status: #{next_step_result.status}"
  
  # If results are persisted, the second step should be able to access results from first step
  puts "Results accessible: Results should now be persisted in database"
rescue => e
  puts "Error: #{e.message}"
  puts "This indicates step results may not be persisted properly"
end

puts "\nTest completed."