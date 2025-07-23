#!/usr/bin/env ruby

require_relative 'lib/tasker_core'
require 'yaml'

# Set up test environment with simple logging
puts "=== Dependency Edge Creation Test ==="

# Register the handler with the current (correct) config
config_path = File.expand_path('spec/handlers/examples/order_fulfillment/config/order_fulfillment_handler.yaml', __dir__)
config = YAML.load_file(config_path)

TaskerCore::Registry.register(
  namespace: config['namespace_name'],
  name: config['name'],
  version: config['version'],
  handler_class: config['task_handler_class'],
  config_schema: config
)

# Create handler instance and task
handler_result = TaskerCore::Registry.find_handler_and_initialize(
  name: "fulfillment/process_order",
  version: "1.0.0",
  config_path: config_path
)

handler_instance = handler_result['handler_instance']

# Create a simple task request
task_request = TaskerCore::Types::TaskRequest.build_test(
  namespace: "fulfillment",
  name: "process_order",
  version: "1.0.0",
  context: {'test' => 'dependency_creation'},
  initiator: "dependency_test",
  source_system: "test",
  reason: "testing dependency creation",
  tags: ["test"]
)

# Initialize task (this should create dependencies)
puts "Creating task..."
init_result = handler_instance.initialize_task(task_request)
task_id = init_result.task_id

puts "Task ID: #{task_id}"
puts "Initialization successful: #{init_result.success?}"

exit 0