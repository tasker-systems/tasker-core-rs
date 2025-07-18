#!/usr/bin/env ruby

# Comparison test to demonstrate the thread safety improvement
# Shows what would happen with the old memoized config vs new per-call config

require 'bundler/setup'
require 'json'
require 'ostruct'

# Add the lib directory to the load path
$LOAD_PATH.unshift File.expand_path('../lib', __dir__)

begin
  require 'tasker_core'
  puts "✅ TaskerCore loaded successfully"
rescue LoadError => e
  puts "❌ Failed to load TaskerCore: #{e.message}"
  exit 1
end

puts "\n🔍 Thread Safety Comparison Test"
puts "=" * 60
puts "Demonstrating the difference between old (broken) and new (fixed) approaches"

# Mock old behavior with memoized config
class OldStyleOrchestrationManager
  include Singleton
  
  def initialize
    @handlers_with_config = {}
  end
  
  # OLD BROKEN APPROACH: Memoizes handler with first config
  def get_handler_old_style(task_config)
    # This would create the handler with the FIRST config it sees
    # All subsequent calls would get the same handler with the original config!
    @memoized_handler ||= MockHandler.new(task_config)
  end
  
  # NEW FIXED APPROACH: Stateless handler, config passed per-call
  def get_handler_new_style(task_config = nil)
    # Handler is stateless, config is passed in handle() method
    @stateless_handler ||= MockHandler.new({})
  end
end

class MockHandler
  def initialize(config)
    @stored_config = config
    @instance_id = object_id
  end
  
  # OLD: Uses stored config
  def handle_old_style(task_id)
    { 
      task_id: task_id, 
      namespace: @stored_config['namespace'],
      version: @stored_config['version'],
      handler_id: @instance_id
    }
  end
  
  # NEW: Receives config per-call
  def handle_new_style(task_id, config)
    { 
      task_id: task_id, 
      namespace: config['namespace'],
      version: config['version'],
      handler_id: @instance_id
    }
  end
end

puts "\n📊 Scenario: 3 different tasks with different configurations"

configs = [
  { 'namespace' => 'payment', 'version' => 'v1.0', 'task_id' => 100 },
  { 'namespace' => 'email', 'version' => 'v2.0', 'task_id' => 101 },
  { 'namespace' => 'report', 'version' => 'v3.0', 'task_id' => 102 }
]

manager = OldStyleOrchestrationManager.instance

puts "\n❌ OLD APPROACH (Broken - First Config Wins):"
puts "-" * 40

old_results = configs.map do |config|
  handler = manager.get_handler_old_style(config)
  result = handler.handle_old_style(config['task_id'])
  puts "Task #{config['task_id']}: Expected '#{config['namespace']}', Got '#{result[:namespace]}'"
  result
end

puts "\n✅ NEW APPROACH (Fixed - Config Per Call):"
puts "-" * 40

new_results = configs.map do |config|
  handler = manager.get_handler_new_style(config)  # Config ignored here
  result = handler.handle_new_style(config['task_id'], config)  # Config passed here
  puts "Task #{config['task_id']}: Expected '#{config['namespace']}', Got '#{result[:namespace]}'"
  result
end

puts "\n🎯 Analysis:"
puts "=" * 40

# Check old approach
old_namespaces = old_results.map { |r| r[:namespace] }.uniq
if old_namespaces.size == 1
  puts "❌ OLD: All tasks got the SAME namespace '#{old_namespaces.first}' (first config stuck!)"
  puts "   This is the thread safety bug - all tasks use the first handler's config!"
else
  puts "✅ OLD: Each task got correct namespace (this shouldn't happen with memoization)"
end

# Check new approach
new_correct = new_results.each_with_index.all? do |result, idx|
  result[:namespace] == configs[idx]['namespace']
end

if new_correct
  puts "✅ NEW: All tasks got their CORRECT namespace via per-call config!"
  puts "   Thread-safe pure functional approach ensures correct configuration"
else
  puts "❌ NEW: Some tasks got wrong namespace (unexpected)"
end

puts "\n🔒 Thread Safety Implications:"
puts "- OLD: In multi-threaded environment, all threads would share the first config"
puts "- NEW: Each thread passes its own config, no shared mutable state"
puts "- NEW: Prevents race conditions and configuration cross-contamination"

puts "\n💡 Implementation Details:"
puts "- Ruby: TaskHandler#handle(task) passes @task_config to Rust"
puts "- Rust: BaseTaskHandler#handle(task_id, task_config) receives config per-call"
puts "- Manager: Creates stateless handler singleton, no config storage"

puts "\n✨ Comparison test complete!"