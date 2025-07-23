#!/usr/bin/env ruby

require_relative 'lib/tasker_core'
require 'yaml'

# Register the handler with current config
config_path = File.expand_path('spec/handlers/examples/order_fulfillment/config/order_fulfillment_handler.yaml', __dir__)
config = YAML.load_file(config_path)

TaskerCore::Registry.register(
  namespace: config['namespace_name'],
  name: config['name'],
  version: config['version'],
  handler_class: config['task_handler_class'],
  config_schema: config
)

# Create a handler instance
handler_result = TaskerCore::Registry.find_handler_and_initialize(
  name: "fulfillment/process_order",
  version: "1.0.0",
  config_path: config_path
)

handler_instance = handler_result['handler_instance']

# Create test task request
task_request = TaskerCore::Types::TaskRequest.build_test(
  namespace: "fulfillment",
  name: "process_order",
  version: "1.0.0",
  context: {'test' => 'dependency_creation'},
  initiator: "dependency_test",
  source_system: "test",
  reason: "testing dependency creation",
  tags: ["test", "dependency"]
)

# Initialize task (this should create workflow step edges in database)
init_result = handler_instance.initialize_task(task_request)
puts "Task initialization result: #{init_result.success?}"
puts "Task ID: #{init_result.task_id}"

# Check the database for workflow step edges
puts "\nChecking tasker_workflow_step_edges table..."
exit 0