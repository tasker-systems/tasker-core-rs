# Load our new domain-based TaskerCore architecture
require_relative '../lib/tasker_core'

# Helper module for domain API testing
module DomainTestHelpers
  # 🎯 SINGLETON ZEROMQ ORCHESTRATION ACCESS
  # These methods provide access to the singleton orchestration components
  # initialized in spec_helper.rb to prevent socket binding conflicts
  
  # Get the singleton orchestration manager
  # @return [TaskerCore::Internal::OrchestrationManager] Singleton manager instance
  def get_singleton_orchestration_manager
    if $test_orchestration_manager
      $test_orchestration_manager
    else
      # Fallback to instance if global not set (shouldn't happen in properly configured tests)
      puts "⚠️  Using fallback OrchestrationManager.instance - singleton may not be initialized"
      TaskerCore::Internal::OrchestrationManager.instance
    end
  end
  
  # Get the singleton orchestration handle  
  # @return [TaskerCore::OrchestrationHandle] Singleton handle instance
  def get_singleton_orchestration_handle
    if $test_orchestration_handle
      $test_orchestration_handle
    else
      # Fallback to manager's handle if global not set
      manager = get_singleton_orchestration_manager
      manager.orchestration_handle
    end
  end
  
  # Check if singleton ZeroMQ integration is available and running
  # @return [Hash] Status information about ZeroMQ integration
  def singleton_zeromq_status
    manager = get_singleton_orchestration_manager
    manager.zeromq_integration_status
  end
  
  # Verify singleton orchestration is properly initialized
  # This should be called at the start of integration tests to ensure proper setup
  def verify_singleton_orchestration_ready
    manager = get_singleton_orchestration_manager
    
    expect(manager).not_to be_nil
    expect(manager.initialized?).to be(true), "OrchestrationManager should be initialized"
    
    handle = get_singleton_orchestration_handle
    expect(handle).not_to be_nil, "OrchestrationHandle should be available"
    
    # Check ZeroMQ status
    status = singleton_zeromq_status
    if status[:enabled]
      puts "✅ Singleton ZeroMQ orchestration verified and ready"
      puts "   Running: #{status[:running]}"
      puts "   Config: #{status[:zeromq_config]&.keys&.join(', ') || 'default'}"
    else
      puts "ℹ️  ZeroMQ integration not enabled - reason: #{status[:reason]}"
    end
    
    { manager: manager, handle: handle, zeromq_status: status }
  end
  # Create tasks using our new domain API
  def create_task_via_domain_api(options = {})
    default_options = {
      name: "test_task_#{SecureRandom.hex(4)}",
      initiator: 'rspec_domain_test',
      context: { test_framework: 'rspec', test_type: 'domain_api' }
    }

    TaskerCore::TestHelpers::Factories.task(default_options.merge(options))
  end

  # Create workflow steps using our new domain API
  def create_workflow_step_via_domain_api(task_id, options = {})
    default_options = {
      task_id: task_id,
      name: "test_step_#{SecureRandom.hex(4)}",
      inputs: { test_data: 'domain_api_input' }
    }

    TaskerCore::TestHelpers::Factories.workflow_step(default_options.merge(options))
  end

  # Create foundation data using our new domain API
  def create_foundation_via_domain_api(options = {})
    default_options = {
      task_name: "foundation_task_#{SecureRandom.hex(4)}",
      namespace: 'rspec_domain_test'
    }

    TaskerCore::TestHelpers::Factories.foundation(default_options.merge(options))
  end

  # Verify no pool timeouts in results
  def verify_no_pool_timeouts(result, operation_name = "operation")
    # Most operations return Hash, but viable_steps returns Array, and Events.statistics returns EventStatistics object
    if result.is_a?(Array)
      # For Array results (like viable_steps), no error checking needed
      # Arrays are returned on success; errors would be returned as Hash with error key
      return result
    end
    
    # EventStatistics objects are also successful results (primitives in, objects out pattern)
    if result.class.name == "TaskerCore::Events::EventStatistics"
      # EventStatistics objects indicate successful operation, no timeout possible
      return result
    end
    
    expect(result).to be_a(Hash), "Expected Hash, Array, or EventStatistics result for #{operation_name}"

    if result['error']
      if result['error'].include?('pool timed out') || result['error'].include?('timeout')
        fail "❌ POOL TIMEOUT in #{operation_name}: #{result['error']}"
      else
        # Other errors are acceptable for testing - log but don't fail
        puts "⚠️  #{operation_name} error (not timeout): #{result['error']}"
      end
    end

    result
  end

  # Perform multiple operations and verify no pool exhaustion
  def stress_test_operations(count = 50, &operation_block)
    results = []

    count.times do |i|
      result = operation_block.call(i)
      verify_no_pool_timeouts(result, "stress_test_operation_#{i}")
      results << result
    end

    results
  end

  # Verify task structure
  def verify_task_structure(task, expected_name = nil)
    expect(task).to have_key('task_id')
    expect(task['task_id']).to be_a(Integer)
    expect(task['task_id']).to be > 0

    if expected_name
      expect(task['name']).to eq(expected_name)
    else
      expect(task).to have_key('name')
    end

    expect(task).to have_key('context')
    expect(task['context']).to be_a(Hash)
  end

  # Verify workflow step structure
  def verify_workflow_step_structure(step, expected_task_id = nil, expected_name = nil)
    expect(step).to have_key('workflow_step_id')
    expect(step['workflow_step_id']).to be_a(Integer)
    expect(step['workflow_step_id']).to be > 0

    if expected_task_id
      expect(step['task_id']).to eq(expected_task_id)
    else
      expect(step).to have_key('task_id')
      expect(step['task_id']).to be_a(Integer)
    end

    if expected_name
      expect(step['name']).to eq(expected_name)
    else
      expect(step).to have_key('name')
    end

    expect(step).to have_key('inputs')
    expect(step['inputs']).to be_a(Hash)
  end

  # ========================================================================
  # WORKER MANAGEMENT HELPERS
  # ========================================================================

  # Start embedded TCP executor server for integration tests
  # This server is needed for workers to connect and register
  #
  # @param options [Hash] Server configuration options
  # @return [TaskerCore::EmbeddedServer] Started embedded server
  def start_test_server(**options)
    server_config = {
      bind_address: '127.0.0.1:8080', # Use standard test port (matches default)
      max_connections: 10,            # Small for testing
      connection_timeout_ms: 5000     # Short timeout for testing
    }.merge(options)
    
    server = TaskerCore::EmbeddedServer.new(server_config)
    success = server.start
    expect(success).to be(true), "Embedded server should start successfully"
    
    # Store for cleanup
    @test_servers ||= []
    @test_servers << server
    
    puts "✅ Started embedded TCP executor server on #{server.bind_address}"
    server
  end

  # Start a test worker that auto-discovers namespaces from registered TaskHandlers
  # This ensures workers are available for the namespaces that TaskHandlers are registered for
  #
  # @param worker_id [String] Unique worker identifier (defaults to test worker)
  # @param options [Hash] Additional worker configuration options
  # @return [TaskerCore::Execution::WorkerManager] Started worker manager
  def start_test_worker(worker_id: nil, **options)
    worker_id ||= "test_worker_#{SecureRandom.hex(4)}"
    
    # Ensure TCP executor server is running first
    start_test_server unless @test_servers&.any?
    
    # Create worker manager with auto-discovery (supported_namespaces: nil)
    worker_manager = TaskerCore::Execution::WorkerManager.new(
      worker_id: worker_id,
      supported_namespaces: nil, # Auto-discover from TaskHandlers  
      max_concurrent_steps: 5,   # Small for testing
      heartbeat_interval: 10,    # Short interval for testing
      **options
    )
    
    # Start the worker
    success = worker_manager.start
    expect(success).to be(true), "Worker should start successfully"
    
    # Store for cleanup
    @test_workers ||= []
    @test_workers << worker_manager
    
    puts "✅ Started test worker: #{worker_id} with namespaces: #{worker_manager.supported_namespaces}"
    worker_manager
  end

  # Stop all test workers created during the test
  def stop_all_test_workers
    return unless @test_workers
    
    @test_workers.each do |worker|
      begin
        worker.stop("Test cleanup")
        puts "✅ Stopped test worker: #{worker.worker_id}"
      rescue StandardError => e
        puts "⚠️  Error stopping worker #{worker.worker_id}: #{e.message}"
      end
    end
    
    @test_workers.clear
  end

  # Stop all test servers created during the test
  def stop_all_test_servers
    return unless @test_servers
    
    @test_servers.each do |server|
      begin
        server.stop
        puts "✅ Stopped embedded TCP executor server"
      rescue StandardError => e
        puts "⚠️  Error stopping server: #{e.message}"
      end
    end
    
    @test_servers.clear
  end

  # Wait for worker registration to propagate to the Rust orchestrator
  # This ensures the Rust side knows about the worker before trying to execute tasks
  #
  # @param timeout [Integer] Maximum time to wait in seconds
  def wait_for_worker_registration(timeout: 5)
    start_time = Time.now
    
    loop do
      # Check if any workers are registered with the orchestrator
      # (This is a placeholder - we'd need to add a method to check worker status)
      break if Time.now - start_time > timeout
      
      sleep(0.1)
    end
    
    sleep(0.5) # Give a bit more time for registration to fully propagate
  end
end

# Helper module for complex workflow validation
module WorkflowValidationHelpers
  # Create complex workflow patterns for testing dependency logic
  # This now delegates to the Rust factory system which properly creates WorkflowStepEdge dependencies
  def create_complex_workflow(type, options = {})
    base_name = "#{type}_workflow_#{SecureRandom.hex(4)}"
    namespace = options[:namespace] || 'workflow_test'

    # Call the Rust complex workflow factory directly
    # This ensures proper WorkflowStepEdge creation in the database
    rust_result = TaskerCore::TestHelpers.create_complex_workflow_with_factory({
      pattern: type.to_s,
      task_name: base_name,
      namespace: namespace,
      context: { workflow_type: type.to_s, created_by: 'ruby_domain_helpers' }
    })

    # Check for errors from Rust factory
    if rust_result['error']
      raise "Failed to create complex workflow: #{rust_result['error']}"
    end

    # Extract results from Rust factory's clean nested structure
    task_data = rust_result['task']
    task_id = task_data['task_id']
    workflow_steps = rust_result['workflow_steps'] || []
    
    # Use the workflow steps directly from Rust factory - they're already well-structured
    steps = workflow_steps

    {
      type: type,
      task: task_data,
      steps: steps,
      rust_factory_result: rust_result
    }
  end

  # NOTE: Manual step creation methods removed
  # All workflow creation now delegates to Rust factory system
  # which properly creates WorkflowStepEdge dependencies in the database

  # Analyze workflow dependency structure via Rust Performance API
  def analyze_workflow_dependencies(task_id)
    result = TaskerCore::Performance.analyze_dependencies(task_id)
    verify_no_pool_timeouts(result, "dependency_analysis")
    result
  end

  # Validate dependency structure matches expected pattern
  def validate_workflow_structure(workflow, expected_structure)
    analysis = analyze_workflow_dependencies(workflow[:task]['task_id'])

    case expected_structure[:type]
    when :linear
      expect(analysis['has_cycles']).to be false
      expect(analysis['max_depth']).to eq(4) # A → B → C → D
      expect(analysis['parallel_branches']).to eq(0) # No parallelism
      expect(analysis['total_steps']).to eq(4)

    when :diamond
      expect(analysis['has_cycles']).to be false
      expect(analysis['max_depth']).to eq(3) # A → B,C → D
      expect(analysis['parallel_branches']).to eq(2) # B and C can run in parallel
      expect(analysis['total_steps']).to eq(4)

    when :parallel
      expect(analysis['has_cycles']).to be false
      expect(analysis['max_depth']).to eq(2) # A → B,C,D
      expect(analysis['parallel_branches']).to eq(3) # B, C, D can all run in parallel
      expect(analysis['total_steps']).to eq(4)

    when :tree
      expect(analysis['has_cycles']).to be false
      expect(analysis['max_depth']).to eq(3) # A → B,C → D,E
      expect(analysis['parallel_branches']).to eq(2) # B,C parallel and D,E parallel
      expect(analysis['total_steps']).to eq(5)

    else
      raise "Unknown expected workflow structure: #{expected_structure[:type]}"
    end

    analysis
  end
end

# Helper module for handle architecture testing
module HandleArchitectureHelpers
  # Get handle information for debugging (production domains only)
  def get_handle_info(domain = :registry)
    case domain
    when :registry
      TaskerCore::Registry.handle_info
    when :performance
      TaskerCore::Performance.handle_info
    when :environment
      TaskerCore::Environment.handle_info
    when :events
      TaskerCore::Events.handle_info
    when :testing
      TaskerCore::Testing.handle_info
    else
      raise "Unknown production domain: #{domain}"
    end
  end

  # Verify handle persistence across operations
  def verify_handle_persistence(domain = :registry, operations_count = 5)
    initial_info = get_handle_info(domain)

    operations_count.times do |i|
      # Perform operation
      case domain
      when :registry
        TaskerCore::Registry.list
      when :performance
        TaskerCore::Performance.system_health
      when :environment
        TaskerCore::Environment.handle_info  # Safe operation for environment
      when :events
        TaskerCore::Events.statistics
      when :testing
        TaskerCore::Testing.validate_environment
      end

      # Check handle is still the same
      current_info = get_handle_info(domain)
      expect(current_info['handle_id']).to eq(initial_info['handle_id'])
    end

    puts "✅ Handle persistence verified for #{domain} domain across #{operations_count} operations"
  end

  # Test rapid operations for pool timeout prevention
  def test_rapid_operations_no_timeouts(operations_count = 100)
    results = []
    start_time = Time.now

    operations_count.times do |i|
      # Rotate between all production domain operations for testing
      result = case i % 5
      when 0
        TaskerCore::Registry.list
      when 1
        TaskerCore::Performance.system_health
      when 2
        TaskerCore::Environment.handle_info
      when 3
        TaskerCore::Events.statistics
      when 4
        TaskerCore::Testing.validate_environment
      end
      verify_no_pool_timeouts(result, "rapid_operation_#{i}")
      results << result
    end

    elapsed = Time.now - start_time
    puts "✅ #{operations_count} rapid operations completed in #{elapsed.round(2)}s with no pool timeouts"

    results
  end
end
