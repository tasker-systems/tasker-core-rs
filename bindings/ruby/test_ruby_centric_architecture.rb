#!/usr/bin/env ruby
# Test script for Ruby-centric step handler architecture

require_relative 'lib/tasker_core'

# Load example step handlers
Dir[File.join(__dir__, 'spec/handlers/examples/order_fulfillment', '**', '*.rb')].each { |f| require f }

puts "🧪 Testing Ruby-Centric Step Handler Architecture"
puts "=" * 60

# 1. Create TaskHandler with YAML config
config_path = File.join(__dir__, 'spec/handlers/examples/order_fulfillment/config/order_fulfillment_handler.yaml')
puts "📄 Loading config: #{config_path}"

handler = TaskerCore::TaskHandler::Base.new(task_config_path: config_path)
puts "✅ Created TaskHandler: #{handler.class.name}"
puts "🔥 Pre-instantiated step handlers: #{handler.step_handlers.size}"

handler.step_handlers.each do |step_name, step_handler|
  puts "   • #{step_name}: #{step_handler.class.name}"
end

# 2. Check OrchestrationManager registry
orchestration_manager = TaskerCore::Internal::OrchestrationManager.instance
ruby_handlers = orchestration_manager.list_ruby_task_handlers

puts "\n📝 Ruby TaskHandler Registry:"
if ruby_handlers.any?
  ruby_handlers.each do |handler_info|
    puts "   • #{handler_info[:key]}: #{handler_info[:handler_class]} (#{handler_info[:step_handler_count]} step handlers)"
  end
else
  puts "   (No handlers registered)"
end

# 3. Test step handler lookup
puts "\n🔍 Testing Step Handler Lookup:"
test_steps = ['validate_order', 'reserve_inventory', 'process_payment', 'ship_order']

test_steps.each do |step_name|
  step_handler = handler.get_step_handler_from_name(step_name)
  if step_handler
    puts "   ✅ #{step_name}: #{step_handler.class.name}"
  else
    puts "   ❌ #{step_name}: Not found"
  end
end

# 4. Test get_task_handler_for_task method
puts "\n🎯 Testing Task Handler Lookup by Task ID:"
found_handler = orchestration_manager.get_task_handler_for_task(12345)
if found_handler
  puts "   ✅ Found handler for task_id 12345: #{found_handler.class.name}"
  puts "   🔥 Handler has #{found_handler.step_handlers.size} pre-instantiated step handlers"
else
  puts "   ❌ No handler found for task_id 12345"
end

# 5. Test process_step_with_handler method
puts "\n🚀 Testing Direct Step Execution:"
begin
  # Create mock task, sequence, and step objects
  task = {
    'task_id' => 12345,
    'data' => { 'customer_info' => { 'id' => 1, 'email' => 'test@example.com' } },
    'metadata' => {}
  }
  
  sequence = {
    'step_dependencies' => []
  }
  
  step = {
    'step_id' => 67890,
    'name' => 'validate_order'
  }
  
  # Call process_step_with_handler directly
  result = handler.process_step_with_handler(task, sequence, step)
  puts "   ✅ Step execution successful!"
  puts "   📊 Result: #{result.inspect}"
  
rescue StandardError => e
  puts "   ❌ Step execution failed: #{e.message}"
  puts "   🔍 Error details: #{e.class.name}"
end

puts "\n🎉 Ruby-Centric Architecture Test Complete!"
puts "=" * 60