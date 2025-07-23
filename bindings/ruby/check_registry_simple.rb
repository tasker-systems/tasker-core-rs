#!/usr/bin/env ruby

require_relative 'lib/tasker_core'
require 'yaml'
require 'json'

puts "=== Registry Configuration Check ==="

# Load the YAML config
config_path = File.expand_path('spec/handlers/examples/order_fulfillment/config/order_fulfillment_handler.yaml', __dir__)
config = YAML.load_file(config_path)

puts "\n1. YAML Step Templates (from file):"
config['step_templates'].each do |step|
  puts "   #{step['name']}: depends_on_steps = #{step['depends_on_steps']}"
end

# Register the handler
TaskerCore::Registry.register(
  namespace: config['namespace_name'],
  name: config['name'],
  version: config['version'],
  handler_class: config['task_handler_class'],
  config_schema: config
)

# Retrieve from registry
handler_result = TaskerCore::Registry.find_handler_and_initialize(
  name: "fulfillment/process_order",
  version: "1.0.0",
  config_path: config_path
)

puts "\n2. Registry Step Templates (from registry):"
if handler_result.is_a?(Hash) && 
   handler_result['metadata'] && 
   handler_result['metadata']['config_schema'] &&
   handler_result['metadata']['config_schema']['step_templates']
   
  handler_result['metadata']['config_schema']['step_templates'].each do |step|
    puts "   #{step['name']}: depends_on_steps = #{step['depends_on_steps']}"
  end
else
  puts "   ERROR: Could not retrieve step templates from registry"
end

puts "\n=== Check Complete ==="