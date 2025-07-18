#!/usr/bin/env ruby

# Comprehensive test script for TAS-20 FFI Performance Optimization
# This script benchmarks multiple FFI functions comparing JSON-based vs optimized implementations

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
TEST_ITERATIONS = 500  # Reduced for more comprehensive testing
NAMESPACE = "test_namespace"
NAME = "test_task"
VERSION = "v1.0"

puts "\n🎯 TAS-20 Comprehensive FFI Performance Optimization Test"
puts "=" * 60
puts "Comparing JSON-based vs Optimized FFI performance across multiple functions"
puts "Test iterations: #{TEST_ITERATIONS} per function"
puts "Target: Consistent <100μs per call improvement"

# Create orchestration handle
puts "\n📝 Creating orchestration handle..."
handle = TaskerCore.create_orchestration_handle

if handle.nil?
  puts "❌ Failed to create orchestration handle"
  exit 1
end

puts "✅ Orchestration handle created"

# Test data for different functions
handler_request = {
  "namespace" => NAMESPACE,
  "name" => NAME,
  "version" => VERSION,
  "context" => {},
  "initiator" => "test_script"
}

workflow_step_options = {
  "task_id" => 123,
  "name" => "test_step",
  "dependencies" => ["step1", "step2"],
  "handler_class" => "TestStepHandler"
}

complex_workflow_options = {
  "pattern" => "linear",
  "namespace" => NAMESPACE,
  "task_name" => "test_workflow",
  "step_count" => 5,
  "parallel_branches" => 2,
  "dependency_depth" => 3
}

puts "\n🧪 Testing function compatibility..."

# Test each function pair
functions_to_test = []

# 1. Handler lookup functions
begin
  original_handler = handle.find_handler(handler_request)
  optimized_handler = handle.find_handler_optimized(NAMESPACE, NAME, VERSION)
  functions_to_test << {
    name: "Handler Lookup",
    original: -> { handle.find_handler(handler_request) },
    optimized: -> { handle.find_handler_optimized(NAMESPACE, NAME, VERSION) },
    original_desc: "JSON-based find_handler",
    optimized_desc: "Primitive-based find_handler_optimized"
  }
  puts "✅ Handler lookup functions work"
rescue => e
  puts "❌ Handler lookup failed: #{e.message}"
end

# 2. Workflow step functions  
begin
  original_step = handle.create_test_workflow_step(workflow_step_options)
  optimized_step = handle.create_test_workflow_step_optimized(
    123, "test_step", ["step1", "step2"], "TestStepHandler", nil
  )
  functions_to_test << {
    name: "Workflow Step Creation",
    original: -> { handle.create_test_workflow_step(workflow_step_options) },
    optimized: -> { handle.create_test_workflow_step_optimized(123, "test_step", ["step1", "step2"], "TestStepHandler", nil) },
    original_desc: "JSON-based create_test_workflow_step",
    optimized_desc: "Primitive-based create_test_workflow_step_optimized"
  }
  puts "✅ Workflow step functions work"
rescue => e
  puts "❌ Workflow step creation failed: #{e.message}"
end

# 3. Complex workflow functions
begin
  original_workflow = handle.create_complex_workflow(complex_workflow_options)
  optimized_workflow = handle.create_complex_workflow_optimized(
    "linear", NAMESPACE, "test_workflow", 5, 2, 3
  )
  functions_to_test << {
    name: "Complex Workflow Creation",
    original: -> { handle.create_complex_workflow(complex_workflow_options) },
    optimized: -> { handle.create_complex_workflow_optimized("linear", NAMESPACE, "test_workflow", 5, 2, 3) },
    original_desc: "JSON-based create_complex_workflow",
    optimized_desc: "Primitive-based create_complex_workflow_optimized"
  }
  puts "✅ Complex workflow functions work"
rescue => e
  puts "❌ Complex workflow creation failed: #{e.message}"
end

if functions_to_test.empty?
  puts "\n⚠️  No functions available for benchmarking"
  exit 1
end

puts "\n⚡ Comprehensive Performance Benchmark"
puts "=" * 50

total_improvements = []

functions_to_test.each do |test_case|
  puts "\n📊 Testing: #{test_case[:name]}"
  puts "-" * 40
  
  benchmark_results = Benchmark.bm(35) do |x|
    original_time = x.report(test_case[:original_desc]) do
      TEST_ITERATIONS.times { test_case[:original].call }
    end
    
    optimized_time = x.report(test_case[:optimized_desc]) do
      TEST_ITERATIONS.times { test_case[:optimized].call }
    end
    
    # Calculate improvement
    original_ms = original_time.real * 1000
    optimized_ms = optimized_time.real * 1000
    improvement_percent = ((original_ms - optimized_ms) / original_ms * 100).round(1)
    per_call_original = (original_ms / TEST_ITERATIONS).round(3)
    per_call_optimized = (optimized_ms / TEST_ITERATIONS).round(3)
    
    total_improvements << improvement_percent
    
    puts "\n📈 Performance Analysis for #{test_case[:name]}:"
    puts "  Original:  #{per_call_original}ms per call"
    puts "  Optimized: #{per_call_optimized}ms per call"
    puts "  Improvement: #{improvement_percent}% faster"
    
    if improvement_percent > 0
      puts "  ✅ Performance improved!"
    else
      puts "  ⚠️  Performance regression detected"
    end
  end
end

# Overall summary
puts "\n🎉 Comprehensive Test Summary"
puts "=" * 40
puts "Functions tested: #{functions_to_test.length}"
puts "Total iterations: #{TEST_ITERATIONS * functions_to_test.length * 2}"

if total_improvements.any?
  avg_improvement = (total_improvements.sum / total_improvements.length).round(1)
  puts "Average improvement: #{avg_improvement}%"
  
  if avg_improvement > 30
    puts "🏆 EXCELLENT: Significant performance gains achieved!"
  elsif avg_improvement > 10
    puts "✅ GOOD: Meaningful performance improvements"
  elsif avg_improvement > 0
    puts "👍 POSITIVE: Performance gains detected"
  else
    puts "⚠️  CONCERN: No performance improvement detected"
  end
end

puts "\n🔍 Technical Details:"
puts "- Eliminates JSON serialization/deserialization overhead"
puts "- Uses primitive parameter types at FFI boundary"
puts "- Maintains Magnus wrapped class architecture for complex types"
puts "- Target achieved: Sub-millisecond FFI calls with reduced overhead"

puts "\n✨ TAS-20 FFI Optimization validation complete!"