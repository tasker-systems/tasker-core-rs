#!/usr/bin/env ruby

require_relative 'lib/tasker_core'

puts '🎯 Testing BaseTaskHandler Memoization Fix'

# Test task configuration
test_config = {
  "name" => "test_task",
  "task_handler_class" => "TestTaskHandler",
  "namespace_name" => "default",
  "version" => "1.0.0",
  "named_steps" => ["test_step"],
  "step_templates" => [
    {
      "name" => "test_step",
      "handler_class" => "TestStepHandler",
      "handler_config" => {"timeout" => 30}
    }
  ]
}

puts "\n1. Creating first TaskHandler instance..."
handler1 = TaskerCore::TaskHandler::Base.new(task_config: test_config)
puts "   Handler 1 object_id: #{handler1.object_id}"

puts "\n2. Creating second TaskHandler instance (should reuse BaseTaskHandler)..."
handler2 = TaskerCore::TaskHandler::Base.new(task_config: test_config)
puts "   Handler 2 object_id: #{handler2.object_id}"

puts "\n3. Creating third TaskHandler instance (should reuse BaseTaskHandler)..."
handler3 = TaskerCore::TaskHandler::Base.new(task_config: test_config)
puts "   Handler 3 object_id: #{handler3.object_id}"

puts "\n4. Testing OrchestrationManager singleton reuse..."
manager1 = TaskerCore::OrchestrationManager.instance
manager2 = TaskerCore::OrchestrationManager.instance
puts "   Manager instances same? #{manager1.object_id == manager2.object_id}"

puts "\n✅ BaseTaskHandler memoization test completed!"
puts "📊 Expected: Only ONE 'Creating NEW BaseTaskHandler' message in logs above"