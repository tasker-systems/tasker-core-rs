#!/usr/bin/env ruby
# frozen_string_literal: true

require_relative 'spec_helper'

# Pool analysis script to understand why orchestration_pool_size is 10 instead of 150
class PoolAnalysisDebugger
  def run
    puts "🔍 Pool Analysis - Understanding Configuration vs Reality"
    puts "=" * 60
    
    # Get handle info
    handle_info = TaskerCore::Factory.handle_info
    puts "📊 Handle Info:"
    handle_info.each do |key, value|
      puts "  #{key}: #{value}"
    end
    
    # Check if there are multiple pools
    puts "\n🔍 Checking pool configuration flow..."
    
    # Test with different operations to see if they use different pools
    test_operations = [
      "TaskerCore::Factory.task(name: 'pool_test_1')",
      "TaskerCore::Factory.foundation(task_name: 'pool_test_2', namespace: 'pool_test')",
      "TaskerCore::Performance.system_health",
      "TaskerCore::Registry.list"
    ]
    
    test_operations.each do |operation|
      puts "\n🔍 Testing #{operation}..."
      start_time = Time.now
      
      begin
        result = eval(operation)
        elapsed = Time.now - start_time
        
        if result.is_a?(Hash) && result['error']
          puts "  ❌ Error: #{result['error']}"
          puts "  ⏱️ Time: #{elapsed.round(3)}s"
        else
          puts "  ✅ Success"
          puts "  ⏱️ Time: #{elapsed.round(3)}s"
        end
      rescue => e
        elapsed = Time.now - start_time
        puts "  ❌ Exception: #{e.message}"
        puts "  ⏱️ Time: #{elapsed.round(3)}s"
      end
    end
    
    # Test rapid successive operations to see connection pattern
    puts "\n🔍 Testing rapid successive operations..."
    
    5.times do |i|
      start_time = Time.now
      result = TaskerCore::Factory.task(name: "rapid_#{i}")
      elapsed = Time.now - start_time
      
      if result['error']
        puts "  #{i}: ❌ #{result['error'][0..60]}... (#{elapsed.round(3)}s)"
      else
        puts "  #{i}: ✅ Task ID #{result['task_id']} (#{elapsed.round(3)}s)"
      end
      
      # Check handle info after each operation
      info = TaskerCore::Factory.handle_info
      puts "    Pool size: #{info['orchestration_pool_size']}"
    end
    
    puts "\n📋 Analysis Summary:"
    puts "  🎯 Expected pool size: 150 (from config)"
    puts "  📊 Actual pool size: #{handle_info['orchestration_pool_size']}"
    puts "  🔍 This suggests a pool configuration mismatch"
    puts "  💡 Need to trace where the pool of size 10 is being created"
  end
end

# Run the pool analysis
debugger = PoolAnalysisDebugger.new

begin
  debugger.run
rescue => e
  puts "❌ Fatal error: #{e.message}"
  puts "🔍 Backtrace:"
  puts e.backtrace.first(10).join("\n")
end