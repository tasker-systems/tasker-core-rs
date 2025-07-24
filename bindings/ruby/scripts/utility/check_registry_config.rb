#!/usr/bin/env ruby

require_relative 'lib/tasker_core'
require 'yaml'
require 'json'

# Disable Rust logging for cleaner output
ENV['RUST_LOG'] = 'off'

puts "=== Checking Registry Configuration ==="

# Load the YAML config (like the test does)
config_path = File.expand_path('spec/handlers/examples/order_fulfillment/config/order_fulfillment_handler.yaml', __dir__)
config = YAML.load_file(config_path)

puts "\n1. YAML Configuration Analysis:"
puts "   Namespace: #{config['namespace_name']}"
puts "   Name: #{config['name']}"
puts "   Version: #{config['version']}"

puts "\n2. Step Templates in YAML:"
config['step_templates'].each do |step|
  puts "   #{step['name']}:"
  puts "     depends_on_steps: #{step['depends_on_steps'].inspect}"
end

# Register the handler (like the test does)
puts "\n3. Registering handler with updated config..."
registration_result = TaskerCore::Registry.register(
  namespace: config['namespace_name'],
  name: config['name'],
  version: config['version'],
  handler_class: config['task_handler_class'],
  config_schema: config
)

puts "   Registration result: #{registration_result.inspect}"

# Try to retrieve the configuration from the registry
puts "\n4. Retrieving configuration from registry..."
begin
  handler_result = TaskerCore::Registry.find_handler_and_initialize(
    name: "fulfillment/process_order",
    version: "1.0.0",
    config_path: config_path
  )
  
  puts "   Handler found: #{handler_result.is_a?(Hash)}"
  
  if handler_result.is_a?(Hash) && handler_result['metadata']
    metadata = handler_result['metadata']
    puts "   Metadata keys: #{metadata.keys}" if metadata
    
    if metadata['config_schema']
      config_schema = metadata['config_schema']
      puts "\n5. Config Schema in Registry:"
      
      if config_schema['step_templates']
        config_schema['step_templates'].each do |step|
          puts "   #{step['name']}:"
          puts "     depends_on_steps: #{step['depends_on_steps'].inspect}"
          puts "     depends_on: #{step['depends_on'].inspect}" if step['depends_on']
        end
      else
        puts "   No step_templates in config_schema!"
      end
    else
      puts "   No config_schema in metadata!"
    end
  else
    puts "   No metadata in handler result!"
  end
  
rescue => e
  puts "   Error retrieving handler: #{e.message}"
  puts "   #{e.backtrace.first(3).join("\n   ")}"
end

puts "\n=== Registry Check Complete ==="