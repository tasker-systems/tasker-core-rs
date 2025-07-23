#!/usr/bin/env ruby

require_relative 'lib/tasker_core'
require 'json'

# Create a simple test task using TestingFactory
puts "Creating test task using TestingFactory..."
factory_options = {
  "task_name" => "OrderFulfillmentTaskHandler",
  "step_count" => 4,
  "pattern" => "linear"
}

result = TaskerCore::TestingFactoryManager.instance.create_test_task(factory_options)
puts "Created task with ID: #{result.task_id}"

task_id = result.task_id

puts "\nTask created successfully! Now checking dependency relationships with debug script..."
puts "Running debug_validate_order_simple.rb to check if dependencies were created properly..."

# Run the database debug script
exec("ruby debug_validate_order_simple.rb")