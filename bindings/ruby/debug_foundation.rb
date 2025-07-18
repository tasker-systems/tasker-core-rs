#!/usr/bin/env ruby
# frozen_string_literal: true

require_relative 'lib/tasker_core'

puts "🔍 Examining foundation method result..."

begin
  result = TaskerCore::Factory.foundation(task_name: 'test_task', namespace: 'test_namespace')
  puts "✅ Foundation result: #{result.inspect}"
  puts "📋 Foundation keys: #{result.keys}"
  
  if result['task']
    puts "📋 Task structure: #{result['task'].inspect}"
    puts "🎯 Task ID: #{result['task']['task_id']}"
  end
  
  if result['named_task']
    puts "📋 Named task structure: #{result['named_task'].inspect}"  
    puts "🎯 Named task ID: #{result['named_task']['task_id']}"
  end
  
rescue => e
  puts "❌ Error: #{e.message}"
  puts "🔍 Backtrace: #{e.backtrace.first(5).join("\n")}"
end