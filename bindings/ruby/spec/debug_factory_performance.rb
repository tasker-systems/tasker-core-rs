#!/usr/bin/env ruby
# frozen_string_literal: true

require_relative 'spec_helper'

# Debug script to understand factory performance issues
class FactoryPerformanceDebugger
  def initialize
    @start_time = Time.now
    @operation_times = {}
  end

  def time_operation(name, &block)
    puts "🔍 Starting #{name}..."
    start = Time.now
    result = block.call
    elapsed = Time.now - start
    @operation_times[name] = elapsed
    puts "✅ #{name} completed in #{elapsed.round(3)}s"
    result
  rescue => e
    elapsed = Time.now - start
    puts "❌ #{name} failed after #{elapsed.round(3)}s: #{e.message}"
    raise
  end

  def test_simple_task_creation
    puts "\n🏭 Testing simple task creation..."
    
    task = time_operation("Simple task creation") do
      TaskerCore::Factory.task(name: "debug_task_#{Time.now.to_i}")
    end
    
    puts "📊 Task result: #{task.keys}"
    if task['error']
      puts "❌ Task creation error: #{task['error']}"
    else
      puts "✅ Task ID: #{task['task_id']}"
    end
    
    task
  end

  def test_handle_info
    puts "\n🔧 Testing handle info..."
    
    handle_info = time_operation("Handle info retrieval") do
      TaskerCore::Factory.handle_info
    end
    
    puts "📊 Handle info: #{handle_info}"
    handle_info
  end

  def test_foundation_creation
    puts "\n🏗️ Testing foundation creation..."
    
    foundation = time_operation("Foundation creation") do
      TaskerCore::Factory.foundation(
        task_name: "debug_foundation_#{Time.now.to_i}",
        namespace: "debug_namespace"
      )
    end
    
    puts "📊 Foundation result: #{foundation.keys}"
    if foundation['error']
      puts "❌ Foundation creation error: #{foundation['error']}"
    else
      puts "✅ Foundation created successfully"
    end
    
    foundation
  end

  def test_workflow_step_creation
    puts "\n🔄 Testing workflow step creation..."
    
    # First create a task
    task = time_operation("Task for workflow step") do
      TaskerCore::Factory.task(name: "debug_workflow_task_#{Time.now.to_i}")
    end
    
    return if task['error']
    
    step = time_operation("Workflow step creation") do
      TaskerCore::Factory.workflow_step(
        task_id: task['task_id'],
        name: "debug_step_#{Time.now.to_i}",
        inputs: { debug: true }
      )
    end
    
    puts "📊 Step result: #{step.keys}"
    if step['error']
      puts "❌ Step creation error: #{step['error']}"
    else
      puts "✅ Step ID: #{step['workflow_step_id']}"
    end
    
    step
  end

  def test_rapid_operations
    puts "\n🚀 Testing rapid operations..."
    
    count = 10
    results = []
    
    time_operation("#{count} rapid task creations") do
      count.times do |i|
        start = Time.now
        task = TaskerCore::Factory.task(name: "rapid_task_#{i}")
        elapsed = Time.now - start
        
        results << {
          index: i,
          elapsed: elapsed,
          success: !task['error'],
          error: task['error']
        }
        
        if task['error']
          puts "  ❌ Task #{i}: #{task['error']} (#{elapsed.round(3)}s)"
        else
          puts "  ✅ Task #{i}: #{task['task_id']} (#{elapsed.round(3)}s)"
        end
      end
    end
    
    # Analyze results
    successful = results.select { |r| r[:success] }
    failed = results.select { |r| !r[:success] }
    
    puts "\n📊 Rapid operations analysis:"
    puts "  ✅ Successful: #{successful.count}/#{count}"
    puts "  ❌ Failed: #{failed.count}/#{count}"
    
    if successful.any?
      avg_time = successful.sum { |r| r[:elapsed] } / successful.count
      puts "  ⏱️ Average success time: #{avg_time.round(3)}s"
    end
    
    results
  end

  def summary
    puts "\n📋 Performance Summary:"
    puts "  🕒 Total runtime: #{(Time.now - @start_time).round(3)}s"
    puts "  📊 Operation times:"
    @operation_times.each do |name, time|
      puts "    #{name}: #{time.round(3)}s"
    end
  end
end

# Run the debug tests
debugger = FactoryPerformanceDebugger.new

puts "🔍 Factory Performance Debugging Session"
puts "=" * 50

begin
  debugger.test_handle_info
  debugger.test_simple_task_creation
  debugger.test_workflow_step_creation
  debugger.test_foundation_creation
  debugger.test_rapid_operations
rescue => e
  puts "❌ Fatal error: #{e.message}"
  puts "🔍 Backtrace:"
  puts e.backtrace.first(10).join("\n")
ensure
  debugger.summary
end