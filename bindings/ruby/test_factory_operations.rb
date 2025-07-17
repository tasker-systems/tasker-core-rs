#!/usr/bin/env ruby

require_relative 'lib/tasker_core'

puts '🎯 Testing Factory Operations with Restructured Singletons'

# Initialize the factory coordination
puts "\n1. Initializing factory coordination..."
factory_manager = TaskerCore::TestingFactoryManager.instance
factory_manager.initialize_factory_coordination!
puts "✅ Factory coordination initialized"

# Test basic task creation
puts "\n2. Testing task creation..."
begin
  task_result = factory_manager.create_test_task({
    initiator: "test_singleton",
    context: { test_type: "singleton_coordination" }
  })
  
  if task_result.is_a?(Hash) && task_result["error"]
    puts "❌ Task creation failed: #{task_result['error']}"
  else
    puts "✅ Task created successfully"
    puts "   Task ID: #{task_result['task_id']}"
    puts "   Status: #{task_result['status']}"
  end
rescue => e
  puts "❌ Task creation error: #{e.message}"
  puts "   Backtrace: #{e.backtrace.first}"
end

# Test workflow step creation
puts "\n3. Testing workflow step creation..."
begin
  step_result = factory_manager.create_test_workflow_step({
    inputs: { test_input: "singleton_test" }
  })
  
  if step_result.is_a?(Hash) && step_result["error"]
    puts "❌ Step creation failed: #{step_result['error']}"
  else
    puts "✅ Workflow step created successfully"
    puts "   Step ID: #{step_result['workflow_step_id']}"
    puts "   Task ID: #{step_result['task_id']}"
  end
rescue => e
  puts "❌ Step creation error: #{e.message}"
  puts "   Backtrace: #{e.backtrace.first}"
end

# Test foundation data creation
puts "\n4. Testing foundation data creation..."
begin
  foundation_result = factory_manager.create_test_foundation({
    namespace: "singleton_test",
    task_name: "test_task",
    step_name: "test_step"
  })
  
  if foundation_result.is_a?(Hash) && foundation_result["error"]
    puts "❌ Foundation creation failed: #{foundation_result['error']}"
  else
    puts "✅ Foundation data created successfully"
    puts "   Namespace: #{foundation_result['namespace']['name']}"
    puts "   Named Task: #{foundation_result['named_task']['name']}"
    puts "   Named Step: #{foundation_result['named_step']['name']}"
  end
rescue => e
  puts "❌ Foundation creation error: #{e.message}"
  puts "   Backtrace: #{e.backtrace.first}"
end

puts "\n✅ Factory operations test completed!"