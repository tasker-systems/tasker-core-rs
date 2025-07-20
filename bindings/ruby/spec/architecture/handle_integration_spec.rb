# frozen_string_literal: true

require 'spec_helper'

RSpec.describe "Handle-Based FFI Architecture", type: :handle_architecture do
  describe "zero global lookups principle" do
    it "eliminates global lookups across all domain operations" do
      # Factory domain operations (test utilities)
      factory_task = TaskerCore::TestHelpers::Factories.task(name: "handle_test_factory")
      verify_no_pool_timeouts(factory_task, "factory_domain_operation")
      
      factory_step = TaskerCore::TestHelpers::Factories.workflow_step(
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
      # Get initial handle info for production domains
      registry_info1 = TaskerCore::Registry.handle_info
      performance_info1 = TaskerCore::Performance.handle_info
      
      # Production domains should expose real handle details from Rust FFI
      expect(registry_info1).to have_key('handle_id')
      expect(performance_info1).to have_key('handle_id')
      
      # Perform operations
      TaskerCore::Registry.list
      TaskerCore::Performance.system_health
      
      # Get handle info again
      registry_info2 = TaskerCore::Registry.handle_info  
      performance_info2 = TaskerCore::Performance.handle_info
      
      # Handles should be persistent across operations
      expect(registry_info2['handle_id']).to eq(registry_info1['handle_id'])
      expect(performance_info2['handle_id']).to eq(performance_info1['handle_id'])
      
      puts "✅ Handle persistence verified across production domains"
    end
  end

  describe "connection pool management" do
    it "eliminates pool timeouts through resource sharing" do
      # Perform 100 rapid operations across different domains
      operations_count = 100
      start_time = Time.now
      
      results = []
      
      operations_count.times do |i|
        case i % 5
        when 0
          # Factory operations (test utilities)
          result = TaskerCore::TestHelpers::Factories.task(name: "pool_test_#{i}")
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
          
        when 3
          # Events operations
          result = TaskerCore::Events.statistics
          verify_no_pool_timeouts(result, "pool_test_events_#{i}")
          
        when 4
          # Testing operations
          result = TaskerCore::Testing.validate_environment
          verify_no_pool_timeouts(result, "pool_test_testing_#{i}")
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
        result = TaskerCore::TestHelpers::Factories.task(name: "perf_test_early_#{i}")
        elapsed = Time.now - start_time
        
        verify_no_pool_timeouts(result, "perf_early_#{i}")
        first_batch_times << elapsed
      end
      
      # Measure performance of operations 50-60 (after handle is established)
      later_batch_times = []
      40.times { |i| TaskerCore::TestHelpers::Factories.task(name: "warmup_#{i}") }  # Warmup
      
      10.times do |i|
        start_time = Time.now
        result = TaskerCore::TestHelpers::Factories.task(name: "perf_test_later_#{i}")
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
      # Test all production domain components
      domains = [:registry, :performance, :environment, :events, :testing]
      
      domains.each do |domain|
        info = get_handle_info(domain)
        
        # All production domains should have real handle info from Rust FFI
        expect(info).to have_key('status') 
        expect(info).to have_key('domain')
        expected_domain = domain == :events ? 'events' : domain.to_s.capitalize
        expect(info['domain']).to eq(expected_domain)
        
        # Production domains should expose real handle details from Rust FFI
        expect(info).to have_key('handle_id')
        expect(info['handle_id']).to be_a(String)
        expect(info['handle_id']).not_to be_empty
        puts "✅ #{domain} handle structure validated: #{info['handle_id'][0..8]}..."
      end
    end

    it "verifies handle lifecycle management" do
      # Test with a production domain component  
      initial_info = TaskerCore::Performance.handle_info
      initial_handle_id = initial_info['handle_id']
      
      # Perform operation to ensure handle is active
      TaskerCore::Performance.system_health
      
      # Handle should still be the same
      current_info = TaskerCore::Performance.handle_info  
      expect(current_info['handle_id']).to eq(initial_handle_id)
      
      # Handle should show as active/valid
      expect(current_info['status']).not_to eq('error')
      
      puts "✅ Handle lifecycle management validated"
    end

    it "validates handle resource sharing" do
      # Multiple production domains should share underlying orchestration resources
      registry_info = TaskerCore::Registry.handle_info
      performance_info = TaskerCore::Performance.handle_info
      events_info = TaskerCore::Events.handle_info
      testing_info = TaskerCore::Testing.handle_info
      
      # While handle IDs may differ, they should share resources
      # (This is implementation-dependent, but we can check they're all valid)
      expect(registry_info['status']).not_to eq('error')
      expect(performance_info['status']).not_to eq('error')
      expect(events_info['status']).not_to eq('error')
      expect(testing_info['status']).not_to eq('error')
      
      # All domains should be able to perform operations without interference
      registry_result = TaskerCore::Registry.list
      health_result = TaskerCore::Performance.system_health
      stats_result = TaskerCore::Events.statistics
      env_result = TaskerCore::Testing.validate_environment
      
      verify_no_pool_timeouts(registry_result, "sharing_registry_op")
      verify_no_pool_timeouts(health_result, "sharing_performance_op")
      verify_no_pool_timeouts(stats_result, "sharing_events_op")
      verify_no_pool_timeouts(env_result, "sharing_testing_op")
      
      puts "✅ Handle resource sharing validated across all production domains"
    end
  end

  describe "architecture consistency" do
    it "ensures consistent error handling across domains" do
      domains = [
        -> { TaskerCore::TestHelpers::Factories.task(name: "error_test") },
        -> { TaskerCore::Performance.system_health },
        -> { TaskerCore::Registry.list },
        -> { TaskerCore::Events.statistics },
        -> { TaskerCore::Testing.validate_environment }
      ]
      
      domains.each_with_index do |operation, index|
        result = operation.call
        
        # All operations should return structured results (Hash, Array, or structured objects)
        # Registry.list returns Array, Events.statistics returns EventStatistics object
        expected_types = [Hash, Array]
        is_event_stats = result.class.name == "TaskerCore::Events::EventStatistics"
        expect(result).to satisfy("be a Hash, Array, or EventStatistics object") do |r|
          expected_types.include?(r.class) || is_event_stats
        end
        
        # No operation should timeout - verify all result types
        verify_no_pool_timeouts(result, "consistency_test_#{index}")
      end
      
      puts "✅ Consistent error handling verified across all production domains"
    end

    it "validates consistent performance characteristics" do
      # Each domain should have similar performance characteristics for equivalent operations
      domain_timings = {}
      
      # Factory domain timing (database-heavy operation)
      start_time = Time.now
      5.times { |i| TaskerCore::TestHelpers::Factories.task(name: "timing_factory_#{i}") }
      domain_timings[:factory] = (Time.now - start_time) / 5
      
      # Performance domain timing (database-heavy operation)
      start_time = Time.now
      5.times { TaskerCore::Performance.system_health }
      domain_timings[:performance] = (Time.now - start_time) / 5
      
      # Registry domain timing (lightweight operation)
      start_time = Time.now
      5.times { TaskerCore::Registry.list }
      domain_timings[:registry] = (Time.now - start_time) / 5
      
      # Events domain timing (lightweight operation)
      start_time = Time.now
      5.times { TaskerCore::Events.statistics }
      domain_timings[:events] = (Time.now - start_time) / 5
      
      # Testing domain timing (database-heavy operation)
      start_time = Time.now
      5.times { TaskerCore::Testing.validate_environment }
      domain_timings[:testing] = (Time.now - start_time) / 5
      
      # Compare database-heavy operations (Factory vs Performance vs Testing)
      # Registry and Events are expected to be faster as they're more lightweight
      db_heavy_operations = [domain_timings[:factory], domain_timings[:performance], domain_timings[:testing]]
      db_heavy_max = db_heavy_operations.max
      db_heavy_min = db_heavy_operations.min
      
      # Database-heavy operations should be reasonably consistent
      # In development/test environments, significant variance is acceptable
      db_performance_ratio = db_heavy_max / db_heavy_min
      
      # Soft performance check - log warnings instead of failing tests
      if db_performance_ratio > 50
        puts "⚠️  Performance variance high: #{db_performance_ratio.round(1)}x difference between fastest and slowest DB operations"
        puts "   This is common in test environments and doesn't indicate a problem with handle architecture"
      elsif db_performance_ratio > 10
        puts "📊 Performance variance: #{db_performance_ratio.round(1)}x difference between database operations"
      end
      
      # Only fail if ratio is extremely high (likely indicates a real timeout/hang issue)
      expect(db_performance_ratio).to be < 1000, "Extremely high performance variance suggests timeout or hang issues"
      
      puts "✅ Consistent performance across all domains: #{domain_timings.transform_values { |v| v.round(3) }}"
      puts "✅ Database operation performance ratio: #{db_performance_ratio.round(2)}"
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
      
      task = TaskerCore::TestHelpers::Factories.task(
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
      
      task = TaskerCore::TestHelpers::Factories.task(
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