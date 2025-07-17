#!/usr/bin/env ruby

require_relative 'lib/tasker_core'

puts '🎯 Testing Restructured Singleton Implementation'

# Test 1: OrchestrationManager singleton
puts "\n1. Testing OrchestrationManager singleton..."
manager1 = TaskerCore::OrchestrationManager.instance
puts "Manager 1 object_id: #{manager1.object_id}"

manager2 = TaskerCore::OrchestrationManager.instance
puts "Manager 2 object_id: #{manager2.object_id}"
puts "Same instance? #{manager1.object_id == manager2.object_id}"

# Test 2: TestingFactoryManager singleton coordination
puts "\n2. Testing TestingFactoryManager singleton coordination..."
factory_manager1 = TaskerCore::TestingFactoryManager.instance
puts "Factory Manager 1 object_id: #{factory_manager1.object_id}"

factory_manager2 = TaskerCore::TestingFactoryManager.instance
puts "Factory Manager 2 object_id: #{factory_manager2.object_id}"
puts "Same instance? #{factory_manager1.object_id == factory_manager2.object_id}"

# Test 3: Initialize factory coordination ONCE
puts "\n3. Testing factory coordination initialization..."
puts "Before initialization - status: #{factory_manager1.status}"
result = factory_manager1.initialize_factory_coordination!
puts "Initialization result: #{result}"
puts "After initialization - status: #{factory_manager1.status}"
puts "Info: #{factory_manager1.info}"

# Test 4: Verify subsequent calls don't re-initialize
puts "\n4. Testing no re-initialization..."
result2 = factory_manager1.initialize_factory_coordination!
puts "Second initialization result: #{result2}"
puts "Status still: #{factory_manager1.status}"

puts "\n✅ Singleton coordination test completed!"