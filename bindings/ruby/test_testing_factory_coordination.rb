#!/usr/bin/env ruby

require_relative 'lib/tasker_core'

puts '🎯 Testing TestingFactory Coordination Fix'

puts "\n1. Testing OrchestrationManager initialization..."
orchestration_manager = TaskerCore::OrchestrationManager.instance
puts "   OrchestrationManager initialized: #{orchestration_manager.initialized?}"

puts "\n2. Testing TestingFactoryManager coordination with OrchestrationManager..."
factory_manager = TaskerCore::TestingFactoryManager.instance
puts "   Before coordination - initialized: #{factory_manager.initialized?}"

result = factory_manager.initialize_factory_coordination!
puts "   Coordination result: #{result}"
puts "   After coordination - initialized: #{factory_manager.initialized?}"

puts "\n3. Testing factory operations..."
begin
  foundation_result = factory_manager.create_test_foundation({namespace: 'coordination_test'})
  if foundation_result.is_a?(Hash) && foundation_result["error"]
    puts "   Foundation creation failed: #{foundation_result['error']}"
  else
    puts "   ✅ Foundation created successfully"
  end
rescue => e
  puts "   ❌ Foundation creation error: #{e.message}"
end

puts "\n✅ TestingFactory coordination test completed!"
puts "📊 Expected: Reduced calls to initialize_unified_orchestration_system"