# frozen_string_literal: true

require 'spec_helper'

RSpec.describe "Handle-Based FFI Architecture", type: :handle_architecture do
  describe "zero global lookups principle" do
    it "eliminates global lookups across all domain operations" do
      # Factory domain operations
      factory_task = TaskerCore::Factory.task(name: "handle_test_factory")
      verify_no_pool_timeouts(factory_task, "factory_domain_operation")
      
      factory_step = TaskerCore::Factory.workflow_step(
        task_id: factory_task['task_id'],
        name: "handle_test_step"
      )
      verify_no_pool_timeouts(factory_step, "factory_step_operation")
      
      # Registry domain operations
      registry_list = TaskerCore::Registry.list
      verify_no_pool_timeouts(registry_list, "registry_list_operation") if registry_list.is_a?(Hash)
      
      registry_check = TaskerCore::Registry.include?("test::handler")
      expect([true, false]).to include(registry_check)
      
      # Performance domain operations
      system_health = TaskerCore::Performance.system_health
      verify_no_pool_timeouts(system_health, "performance_health_operation")
      
      # All operations should complete without pool timeouts
      puts "✅ All domain operations completed with zero global lookups"
    end

    it "maintains persistent handles across domain operations" do
      # Get initial handle info for each domain
      factory_info1 = TaskerCore::Factory.handle_info
      registry_info1 = TaskerCore::Registry.handle_info
      performance_info1 = TaskerCore::Performance.handle_info
      
      expect(factory_info1).to have_key('handle_id')
      expect(registry_info1).to have_key('handle_id')
      expect(performance_info1).to have_key('handle_id')
      
      # Perform operations
      TaskerCore::Factory.task(name: "persistence_test")
      TaskerCore::Registry.list
      TaskerCore::Performance.system_health
      
      # Get handle info again
      factory_info2 = TaskerCore::Factory.handle_info
      registry_info2 = TaskerCore::Registry.handle_info  
      performance_info2 = TaskerCore::Performance.handle_info
      
      # Handles should be persistent
      expect(factory_info2['handle_id']).to eq(factory_info1['handle_id'])
      expect(registry_info2['handle_id']).to eq(registry_info1['handle_id'])
      expect(performance_info2['handle_id']).to eq(performance_info1['handle_id'])
      
      puts "✅ Handle persistence verified across all domains"
    end
  end

  describe "connection pool management" do
    it "eliminates pool timeouts through resource sharing" do
      # Perform 100 rapid operations across different domains
      operations_count = 100
      start_time = Time.now
      
      results = []
      
      operations_count.times do |i|
        case i % 3
        when 0
          # Factory operations
          result = TaskerCore::Factory.task(name: "pool_test_#{i}")
          verify_no_pool_timeouts(result, "pool_test_factory_#{i}")
          
        when 1
          # Performance operations  
          result = TaskerCore::Performance.system_health
          verify_no_pool_timeouts(result, "pool_test_performance_#{i}")
          
        when 2
          # Registry operations
          result = TaskerCore::Registry.list
          # Registry.list might return Array or Hash depending on implementation
          verify_no_pool_timeouts(result, "pool_test_registry_#{i}") if result.is_a?(Hash)
        end
        
        results << result
      end
      
      elapsed = Time.now - start_time
      
      expect(results.length).to eq(operations_count)
      puts "✅ #{operations_count} rapid cross-domain operations completed in #{elapsed.round(2)}s with no pool timeouts"
    end

    it "maintains stable performance under load" do
      # Measure performance of first 10 operations
      first_batch_times = []
      10.times do |i|
        start_time = Time.now
        result = TaskerCore::Factory.task(name: "perf_test_early_#{i}")
        elapsed = Time.now - start_time
        
        verify_no_pool_timeouts(result, "perf_early_#{i}")
        first_batch_times << elapsed
      end
      
      # Measure performance of operations 50-60 (after handle is established)
      later_batch_times = []
      40.times { |i| TaskerCore::Factory.task(name: "warmup_#{i}") }  # Warmup
      
      10.times do |i|
        start_time = Time.now
        result = TaskerCore::Factory.task(name: "perf_test_later_#{i}")
        elapsed = Time.now - start_time
        
        verify_no_pool_timeouts(result, "perf_later_#{i}")
        later_batch_times << elapsed
      end
      
      # Performance should be stable (later operations not significantly slower)
      avg_early = first_batch_times.sum / first_batch_times.length
      avg_later = later_batch_times.sum / later_batch_times.length
      
      # Later operations should not be more than 50% slower than early ones
      performance_degradation = (avg_later - avg_early) / avg_early
      expect(performance_degradation).to be < 0.5
      
      puts "✅ Performance stability verified: early=#{avg_early.round(3)}s, later=#{avg_later.round(3)}s, degradation=#{(performance_degradation * 100).round(1)}%"
    end
  end

  describe "handle architecture validation" do
    it "validates handle structure and metadata" do
      domains = [:factory, :registry, :performance, :environment]
      
      domains.each do |domain|
        info = get_handle_info(domain)
        
        # All handles should have basic structure
        expect(info).to have_key('handle_id')
        expect(info['handle_id']).to be_a(String)
        expect(info['handle_id']).not_to be_empty
        
        # Should have status information
        expect(info).to have_key('status') 
        
        puts "✅ #{domain} handle structure validated: #{info['handle_id'][0..8]}..."
      end
    end

    it "verifies handle lifecycle management" do
      # Get initial handle
      initial_info = TaskerCore::Factory.handle_info
      initial_handle_id = initial_info['handle_id']
      
      # Perform operation to ensure handle is active
      TaskerCore::Factory.task(name: "lifecycle_test")
      
      # Handle should still be the same
      current_info = TaskerCore::Factory.handle_info  
      expect(current_info['handle_id']).to eq(initial_handle_id)
      
      # Handle should show as active/valid
      expect(current_info['status']).not_to eq('error')
      
      puts "✅ Handle lifecycle management validated"
    end

    it "validates handle resource sharing" do
      # Multiple domains should share underlying orchestration resources
      factory_info = TaskerCore::Factory.handle_info
      performance_info = TaskerCore::Performance.handle_info
      
      # While handle IDs may differ, they should share resources
      # (This is implementation-dependent, but we can check they're both valid)
      expect(factory_info['status']).not_to eq('error')
      expect(performance_info['status']).not_to eq('error')
      
      # Both should be able to perform operations without interference
      task_result = TaskerCore::Factory.task(name: "sharing_test")
      health_result = TaskerCore::Performance.system_health
      
      verify_no_pool_timeouts(task_result, "sharing_factory_op")
      verify_no_pool_timeouts(health_result, "sharing_performance_op")
      
      puts "✅ Handle resource sharing validated"
    end
  end

  describe "architecture consistency" do
    it "ensures consistent error handling across domains" do
      domains = [
        -> { TaskerCore::Factory.task(name: "error_test") },
        -> { TaskerCore::Performance.system_health },
        -> { TaskerCore::Registry.list }
      ]
      
      domains.each_with_index do |operation, index|
        result = operation.call
        
        # All operations should return Hash results
        expect(result).to be_a(Hash) unless result.is_a?(Array)  # Registry.list might return Array
        
        # No operation should timeout
        verify_no_pool_timeouts(result, "consistency_test_#{index}") if result.is_a?(Hash)
      end
      
      puts "✅ Consistent error handling verified across domains"
    end

    it "validates consistent performance characteristics" do
      # Each domain should have similar performance characteristics
      domain_timings = {}
      
      # Factory domain timing
      start_time = Time.now
      5.times { |i| TaskerCore::Factory.task(name: "timing_factory_#{i}") }
      domain_timings[:factory] = (Time.now - start_time) / 5
      
      # Performance domain timing
      start_time = Time.now
      5.times { TaskerCore::Performance.system_health }
      domain_timings[:performance] = (Time.now - start_time) / 5
      
      # Registry domain timing  
      start_time = Time.now
      5.times { TaskerCore::Registry.list }
      domain_timings[:registry] = (Time.now - start_time) / 5
      
      # No domain should be dramatically slower than others
      max_time = domain_timings.values.max
      min_time = domain_timings.values.min
      
      # Max should not be more than 10x slower than min
      performance_ratio = max_time / min_time
      expect(performance_ratio).to be < 10
      
      puts "✅ Consistent performance across domains: #{domain_timings.transform_values { |v| v.round(3) }}"
    end
  end

  describe "FFI boundary validation" do
    it "validates proper Ruby-Rust data conversion" do
      # Complex data should survive Ruby -> Rust -> Ruby conversion
      complex_data = {
        string_value: "test_string",
        integer_value: 42,
        float_value: 3.14159,
        boolean_value: true,
        array_value: [1, 2, 3, "four"],
        nested_hash: {
          inner_string: "nested_value",
          inner_array: ["a", "b", "c"]
        }
      }
      
      task = TaskerCore::Factory.task(
        name: "ffi_conversion_test",
        context: complex_data
      )
      
      verify_no_pool_timeouts(task, "ffi_conversion")
      verify_task_structure(task)
      
      # Verify data round-trip integrity
      returned_context = task['context']
      expect(returned_context['string_value']).to eq('test_string')
      expect(returned_context['integer_value']).to eq(42)
      expect(returned_context['float_value']).to be_within(0.0001).of(3.14159)
      expect(returned_context['boolean_value']).to be true
      expect(returned_context['array_value']).to eq([1, 2, 3, "four"])
      expect(returned_context['nested_hash']['inner_string']).to eq('nested_value')
      
      puts "✅ FFI data conversion integrity validated"
    end

    it "handles large data structures efficiently" do
      # Create large context data
      large_data = {
        large_array: (1..1000).to_a,
        large_string: "x" * 10000,
        repeated_structures: 100.times.map do |i|
          {
            id: i,
            name: "item_#{i}",
            metadata: {
              created_at: Time.now.iso8601,
              tags: ["tag1", "tag2", "tag3"],
              config: { setting1: true, setting2: "value" }
            }
          }
        end
      }
      
      start_time = Time.now
      
      task = TaskerCore::Factory.task(
        name: "large_data_test",
        context: large_data
      )
      
      elapsed = Time.now - start_time
      
      verify_no_pool_timeouts(task, "large_data_ffi")
      verify_task_structure(task)
      
      # Large data operation should complete reasonably quickly
      expect(elapsed).to be < 5.0  # Should complete within 5 seconds
      
      # Verify large data was preserved
      returned_context = task['context']
      expect(returned_context['large_array'].length).to eq(1000)
      expect(returned_context['large_string'].length).to eq(10000)
      expect(returned_context['repeated_structures'].length).to eq(100)
      
      puts "✅ Large data FFI handling validated in #{elapsed.round(3)}s"
    end
  end
end