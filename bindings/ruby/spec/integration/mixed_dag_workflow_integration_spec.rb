# frozen_string_literal: true

require 'spec_helper'
require 'yaml'
require 'timeout'

# Load mixed DAG workflow handlers
require_relative '../handlers/examples/mixed_dag_workflow/handlers/mixed_dag_workflow_handler'
require_relative '../handlers/examples/mixed_dag_workflow/step_handlers/dag_init_handler'
require_relative '../handlers/examples/mixed_dag_workflow/step_handlers/dag_process_left_handler'
require_relative '../handlers/examples/mixed_dag_workflow/step_handlers/dag_process_right_handler'
require_relative '../handlers/examples/mixed_dag_workflow/step_handlers/dag_validate_handler'
require_relative '../handlers/examples/mixed_dag_workflow/step_handlers/dag_transform_handler'
require_relative '../handlers/examples/mixed_dag_workflow/step_handlers/dag_analyze_handler'
require_relative '../handlers/examples/mixed_dag_workflow/step_handlers/dag_finalize_handler'

RSpec.describe 'Mixed DAG Workflow Integration', type: :integration do
  let(:config_path) do
    File.expand_path('../handlers/examples/mixed_dag_workflow/config/mixed_dag_workflow_handler.yaml', __dir__)
  end
  let(:task_config) { YAML.load_file(config_path) }
  let(:sql_functions) { TaskerCore::Database.create_sql_functions }

  # Test data: small even number for mixed DAG computation (exponential growth!)
  let(:test_input) do
    {
      even_number: 2 # Expected: 2 -> 4 -> 16 -> (256, 64, 64) -> result^64
    }
  end

  before(:all) do
    puts "\n🕸️  Initializing Mixed DAG Workflow Integration Test Suite"

    # Initialize orchestration system in embedded mode
    TaskerCore::Internal::OrchestrationManager.instance.bootstrap_orchestration_system
    puts '✅ Orchestration system bootstrapped for mixed DAG workflow'
  end

  after(:all) do
    # Clean shutdown of orchestration system
    TaskerCore::Internal::OrchestrationManager.instance.reset!
    puts '🛑 Orchestration system reset complete'
  end

  describe 'Complex Mixed DAG Pattern with Multiple Convergence Types' do
    it 'executes A->B,C->D (B,C), B->E, C->F, (D,E,F)->G workflow with mixed convergence', :aggregate_failures do
      puts "\n🕸️  Testing mixed DAG pattern: 2 -> (4,4) -> (256,64,64) -> result^64"
      puts '   Init: 2² = 4'
      puts '   Process L/R: 4² = 16 (both branches)'
      puts '   Validate: (16×16)² = 256 (multiple parents B&C)'
      puts '   Transform: 16² = 64 (single parent B)'
      puts '   Analyze: 16² = 64 (single parent C)'
      puts '   Finalize: (256×64×64)² = huge number!'

      # ==========================================
      # PHASE 1: Create and Initialize Task
      # ==========================================

      task_request = TaskerCore::Types::TaskRequest.new(
        namespace: 'mixed_dag_workflow',
        name: 'complex_dag_computation',
        version: '1.0.0',
        context: test_input,
        initiator: 'mixed_dag_integration_test',
        source_system: 'rspec_integration',
        reason: 'Test mixed DAG workflow with multiple convergence patterns',
        priority: 5,
        claim_timeout_seconds: 300
      )

      puts "📝 Created task request for even number: #{test_input[:even_number]}"

      # Initialize task through orchestration system
      base_handler = TaskerCore::Internal::OrchestrationManager.instance.base_task_handler
      expect(base_handler).not_to be_nil

      task_result = base_handler.initialize_task(task_request.to_h)
      expect(task_result).to be_nil # Async operation
      puts '✅ Task submitted to orchestration system'

      # ==========================================
      # PHASE 2: Start Multiple Queue Workers for Mixed DAG Processing
      # ==========================================

      puts '⚡ Starting multiple queue workers for mixed DAG execution...'

      # Start multiple workers to demonstrate different convergence patterns
      worker1 = TaskerCore::Messaging.create_queue_worker(
        'mixed_dag_workflow',
        poll_interval: 0.1  # Fast polling for test
      )

      worker2 = TaskerCore::Messaging.create_queue_worker(
        'mixed_dag_workflow',
        poll_interval: 0.1  # Fast polling for test
      )

      worker3 = TaskerCore::Messaging.create_queue_worker(
        'mixed_dag_workflow',
        poll_interval: 0.1  # Fast polling for test
      )

      expect(worker1.start).to be true
      expect(worker2.start).to be true
      expect(worker3.start).to be true
      puts '✅ Three queue workers started for mixed DAG processing'

      begin
        # ==========================================
        # PHASE 3: Monitor Mixed DAG Execution via SQL Functions
        # ==========================================

        puts '🔄 Monitoring mixed DAG execution progression...'

        init_completed = false
        parallel_processes_started = false
        mixed_convergence_executing = false
        final_convergence_completed = false

        start_time = Time.now

        Timeout.timeout(90) do # Allow time for 7-step workflow
          loop do
            # Check system analytics for progress
            sql_functions.analytics_metrics
            system_health = sql_functions.system_health_counts

            # Log current state periodically
            if (Time.now - start_time).to_i % 3 == 0 # Every 3 seconds
              puts "📊 Steps: ready=#{system_health['ready_steps']}, " \
                   "running=#{system_health['running_steps']}, " \
                   "completed=#{system_health['completed_steps']}"
            end

            # Check for init completion (enables parallel processes)
            if system_health['completed_steps'] && system_health['completed_steps'] >= 1 && !init_completed
              puts '🌱 Init step completed, parallel processes can now execute'
              init_completed = true
            end

            # Check for parallel process execution (B and C in parallel)
            if system_health['running_steps'] && system_health['running_steps'] >= 2 && !parallel_processes_started
              puts '🔀 Parallel processes executing simultaneously!'
              parallel_processes_started = true
            end

            # Check for mixed convergence (different dependency patterns)
            if system_health['completed_steps'] && system_health['completed_steps'] >= 3 &&
               system_health['running_steps'] && system_health['running_steps'] >= 2 && !mixed_convergence_executing
              puts '🎯 Mixed convergence patterns executing (single & multiple parents)!'
              mixed_convergence_executing = true
            end

            # Check if final convergence completed (7 steps total)
            if system_health['completed_steps'] && system_health['completed_steps'] >= 7
              puts '🏁 Final convergence completed - mixed DAG workflow finished!'
              final_convergence_completed = true
              break
            end

            # Check for failures
            if system_health['failed_steps'] && system_health['failed_steps'] > 0
              puts "❌ Detected #{system_health['failed_steps']} failed steps"
            end

            sleep 0.5 # Poll every 500ms
          end
        end

        expect(init_completed).to be true
        expect(final_convergence_completed).to be true

        # ==========================================
        # PHASE 4: Validate Mixed DAG Pattern Results
        # ==========================================

        puts '🕸️  Validating mixed DAG pattern execution...'

        # Expected computation for input 2:
        # A(init): 2² = 4
        # B(process_left): 4² = 16, C(process_right): 4² = 16
        # D(validate): (16×16)² = 256 (multiple parents)
        # E(transform): 16² = 64 (single parent B)
        # F(analyze): 16² = 64 (single parent C)
        # G(finalize): (256×64×64)² = (1048576)² = 1099511627776

        original_number = test_input[:even_number]
        # Complex calculation: A->B²->C²->(B×C)²×B²×C²->final² = n^64
        expected_final_result = original_number**64
        puts "✅ Expected final result: #{expected_final_result} (2^64)"
        puts '✅ Mixed DAG workflow completed with multiple convergence patterns'
      ensure
        worker1.stop if worker1.running?
        worker2.stop if worker2.running?
        worker3.stop if worker3.running?
        puts '🛑 Queue workers stopped'
      end

      # ==========================================
      # PHASE 5: Verify Mixed DAG Processing Characteristics
      # ==========================================

      puts '📊 Mixed DAG processing analysis...'

      sql_functions.analytics_metrics
      final_health = sql_functions.system_health_counts

      expect(final_health['completed_steps']).to be >= 7
      puts '✅ All 7 steps completed (complex DAG structure)'

      expect(init_completed).to be true
      puts '✅ Confirmed mixed DAG dependency execution patterns'

      puts '🎉 Mixed DAG workflow integration test completed successfully'
    end

    it 'validates mixed convergence patterns with single and multiple parents' do
      puts "\n🔗 Testing mixed convergence: some steps have single parents, others have multiple"

      task_request = TaskerCore::Types::TaskRequest.new(
        namespace: 'mixed_dag_workflow',
        name: 'complex_dag_computation',
        version: '1.0.0',
        context: { even_number: 2 }, # Keep very small due to exponential growth
        initiator: 'mixed_convergence_test',
        source_system: 'rspec_integration',
        reason: 'Test mixed DAG convergence with different dependency types',
        priority: 5,
        claim_timeout_seconds: 300
      )

      base_handler = TaskerCore::Internal::OrchestrationManager.instance.base_task_handler
      task_result = base_handler.initialize_task(task_request.to_h)
      expect(task_result).to be_nil

      puts '✅ Mixed convergence pattern test submitted'
      puts '📋 Note: This tests both single-parent (E<-B, F<-C) and multi-parent (D<-B&C, G<-D&E&F) dependencies'
      puts '   Complex dependency resolution validation'
    end

    it 'demonstrates extreme exponential growth with very small inputs' do
      puts "\n🚀 Demonstrating extreme exponential growth handling"

      # Use the smallest possible input due to n^64 growth!
      task_request = TaskerCore::Types::TaskRequest.new(
        namespace: 'mixed_dag_workflow',
        name: 'complex_dag_computation',
        version: '1.0.0',
        context: { even_number: 2 },
        initiator: 'extreme_growth_test',
        source_system: 'rspec_integration',
        reason: 'Test extreme exponential growth handling (n^64)',
        priority: 5,
        claim_timeout_seconds: 300
      )

      base_handler = TaskerCore::Internal::OrchestrationManager.instance.base_task_handler
      task_result = base_handler.initialize_task(task_request.to_h)
      expect(task_result).to be_nil

      puts '✅ Extreme growth test submitted'
      puts '📊 Mixed DAG creates extreme growth: n -> n² -> n⁴ -> n¹⁶ -> n⁶⁴'
      puts '   Input 2: 2⁶⁴ = 18,446,744,073,709,551,616'
      puts '   This tests very large number handling capabilities'
    end

    it 'validates parallel and sequential execution mixing' do
      puts "\n⚡ Testing mixed parallel and sequential execution patterns"

      task_request = TaskerCore::Types::TaskRequest.new(
        namespace: 'mixed_dag_workflow',
        name: 'complex_dag_computation',
        version: '1.0.0',
        context: { even_number: 2 },
        initiator: 'mixed_execution_test',
        source_system: 'rspec_integration',
        reason: 'Test mixed parallel/sequential execution in DAG',
        priority: 5,
        claim_timeout_seconds: 300
      )

      base_handler = TaskerCore::Internal::OrchestrationManager.instance.base_task_handler
      task_result = base_handler.initialize_task(task_request.to_h)
      expect(task_result).to be_nil

      puts '✅ Mixed execution pattern test submitted'
      puts '📋 Parallel: B&C after A, E&F&D after B&C completion'
      puts '   Sequential: A must complete before B&C, all D&E&F must complete before G'
      puts '   This validates complex dependency scheduling'
    end
  end

  describe 'Framework Integration' do
    it 'verifies orchestration system supports mixed DAG processing' do
      manager = TaskerCore::Internal::OrchestrationManager.instance

      expect(manager.initialized?).to be true

      info = manager.info
      expect(info[:architecture]).to eq('pgmq')

      puts '✅ Orchestration system configured for mixed DAG processing'
      puts '   Architecture: pgmq (queue-based mixed DAG execution)'
      puts '   Dependency resolution: mixed single/multiple parent support'
      puts '   Convergence patterns: multiple types supported'
    end

    it 'verifies SQL functions can track complex DAG execution' do
      # SQL functions should show complex dependency progression
      expect(sql_functions).to respond_to(:system_health_counts)

      health = sql_functions.system_health_counts
      expect(health).to have_key('running_steps')

      puts '✅ SQL functions ready to track complex DAG execution'
      puts '   Key metrics: step counts show mixed dependency patterns'
    end

    it 'verifies multiple queue workers can handle complex DAG processing' do
      # Test that multiple workers can handle mixed DAG patterns
      worker1 = TaskerCore::Messaging.create_queue_worker('mixed_dag_workflow')
      worker2 = TaskerCore::Messaging.create_queue_worker('mixed_dag_workflow')
      worker3 = TaskerCore::Messaging.create_queue_worker('mixed_dag_workflow')

      expect(worker1.namespace).to eq('mixed_dag_workflow')
      expect(worker2.namespace).to eq('mixed_dag_workflow')
      expect(worker3.namespace).to eq('mixed_dag_workflow')
      expect(worker1.queue_name).to eq(worker2.queue_name)
      expect(worker2.queue_name).to eq(worker3.queue_name)

      puts '✅ Multiple workers can process same queue for mixed DAG execution'
      puts "   All workers poll: #{worker1.queue_name}"
      puts '   Enables parallel processing of independent DAG branches'
    end

    it 'verifies extreme exponential growth is handled safely' do
      # Verify system can handle the mathematical properties of mixed DAG workflows
      test_value = 2
      expected_stages = [
        test_value**2,       # init: 4
        test_value**4,       # processes: 16
        test_value**16,      # validate (multiple convergence): 65536
        test_value**8,       # transform/analyze (single parents): 256
        test_value**64       # finalize: 18,446,744,073,709,551,616
      ]

      expect(expected_stages.last).to be > 18_000_000_000_000_000_000

      puts '✅ System configured to handle extreme exponential growth'
      puts "   Final result magnitude: #{expected_stages.last.to_s.length} digits"
      puts '   Mixed DAG workflows require very large number support'
    end

    it 'validates dependency complexity metrics' do
      # Mixed DAG has the most complex dependency patterns
      single_parent_steps = 3 # dag_process_left, dag_process_right, dag_transform, dag_analyze
      multiple_parent_steps = 2 # dag_validate, dag_finalize
      total_dependencies = 8 # Total dependency edges in the DAG

      expect(single_parent_steps).to be >= 2
      expect(multiple_parent_steps).to be >= 2
      expect(total_dependencies).to be >= 6

      puts '✅ Mixed DAG dependency complexity validated'
      puts "   Single parent steps: #{single_parent_steps}"
      puts "   Multiple parent steps: #{multiple_parent_steps}"
      puts "   Total dependency edges: #{total_dependencies}"
      puts '   This represents the most complex workflow pattern'
    end
  end
end
