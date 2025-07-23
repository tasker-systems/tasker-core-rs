#!/usr/bin/env ruby

require_relative 'lib/tasker_core'
require 'yaml'

puts "=== Final Dependency Creation Check ==="

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

# Create task
task_request = TaskerCore::Types::TaskRequest.build_test(
  namespace: "fulfillment",
  name: "process_order",
  version: "1.0.0",
  context: {'test' => 'final_check'},
  initiator: "final_test",
  source_system: "test",
  reason: "final dependency test",
  tags: ["test"]
)

init_result = handler_result['handler_instance'].initialize_task(task_request)
task_id = init_result.task_id

puts "\n1. Task created successfully: #{init_result.success?}"
puts "2. Task ID: #{task_id}"

puts "\n3. YAML Configuration Dependencies:"
config['step_templates'].each do |step|
  puts "   #{step['name']}: #{step['depends_on_steps'] || []}"
end

puts "\n=== Test Complete - Task ID #{task_id} created ==="