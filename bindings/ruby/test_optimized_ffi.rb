#!/usr/bin/env ruby

# Test script to compare original vs optimized FFI functions
# This script benchmarks the performance difference between JSON-based and Magnus-based FFI

require 'bundler/setup'
require 'benchmark'
require 'json'

# Add the lib directory to the load path
$LOAD_PATH.unshift File.expand_path('../lib', __dir__)

begin
  require 'tasker_core'
  puts "✅ TaskerCore loaded successfully"
rescue LoadError => e
  puts "❌ Failed to load TaskerCore: #{e.message}"
  exit 1
end

# Test configuration
TEST_ITERATIONS = 1000
NAMESPACE = "test_namespace"
NAME = "test_task"
VERSION = "v1.0"

puts "\n🎯 TAS-20 FFI Performance Optimization Test"
puts "=" * 50
puts "Comparing JSON-based vs Magnus-based FFI performance"
puts "Test iterations: #{TEST_ITERATIONS}"
puts "Task: #{NAMESPACE}/#{NAME}:#{VERSION}"

# Create orchestration handle
puts "\n📝 Creating orchestration handle..."
handle = TaskerCore.create_orchestration_handle

if handle.nil?
  puts "❌ Failed to create orchestration handle"
  exit 1
end

puts "✅ Orchestration handle created: #{handle.info}"

# Test data
test_request = {
  "namespace" => NAMESPACE,
  "name" => NAME,
  "version" => VERSION,
  "context" => {},
  "initiator" => "test_script"
}

puts "\n🧪 Testing function compatibility..."

# Test original function
begin
  original_result = handle.find_handler(test_request)
  puts "✅ Original find_handler works"
  puts "   Result: #{original_result.class}"
rescue => e
  puts "❌ Original find_handler failed: #{e.message}"
  original_result = nil
end

# Test optimized function
begin
  optimized_result = handle.find_handler_optimized(NAMESPACE, NAME, VERSION)
  puts "✅ Optimized find_handler_optimized works"
  puts "   Result: #{optimized_result.class}"
  puts "   Found: #{optimized_result.found if optimized_result.respond_to?(:found)}"
rescue => e
  puts "❌ Optimized find_handler_optimized failed: #{e.message}"
  optimized_result = nil
end

# Only proceed with benchmarking if both functions work
if original_result && optimized_result
  puts "\n⚡ Performance Benchmark"
  puts "-" * 30
  
  benchmark_results = Benchmark.bm(20) do |x|
    x.report("JSON-based (original)") do
      TEST_ITERATIONS.times do
        handle.find_handler(test_request)
      end
    end
    
    x.report("Magnus-based (optimized)") do
      TEST_ITERATIONS.times do
        handle.find_handler_optimized(NAMESPACE, NAME, VERSION)
      end
    end
  end
  
  puts "\n📊 Performance Analysis"
  puts "JSON-based approach: Uses JSON serialization/deserialization"
  puts "Magnus-based approach: Uses direct primitive types + Magnus wrapped classes"
  puts "Target: <100μs per call (vs current >1ms)"
  
else
  puts "\n⚠️  Skipping benchmark due to function errors"
end

puts "\n🎉 Test complete!"
puts "This test validates TAS-20 FFI optimization implementation"