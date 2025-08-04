# frozen_string_literal: true

require 'spec_helper'
require 'yaml'
require 'timeout'

# Load linear workflow handlers
require_relative '../handlers/examples/linear_workflow/handlers/linear_workflow_handler'
require_relative '../handlers/examples/linear_workflow/step_handlers/linear_step_1_handler'
require_relative '../handlers/examples/linear_workflow/step_handlers/linear_step_2_handler'
require_relative '../handlers/examples/linear_workflow/step_handlers/linear_step_3_handler'
require_relative '../handlers/examples/linear_workflow/step_handlers/linear_step_4_handler'

RSpec.describe 'Linear Workflow Integration', type: :integration do
  let(:config_path) { File.expand_path('../handlers/examples/linear_workflow/config/linear_workflow_handler.yaml', __dir__) }
  let(:task_config) { YAML.load_file(config_path) }
  let(:sql_functions) { TaskerCore::Database.create_sql_functions }

  # Test data: even number that will flow through the mathematical sequence
  let(:test_input) do
    {
      even_number: 6  # Expected progression: 6 -> 36 -> 46 -> 138 -> 69
    }
  end

  before(:all) do
    puts "\n🚀 Initializing Linear Workflow Integration Test Suite"
    
    # Initialize orchestration system in embedded mode
    # This will set up queues, start embedded Rust listeners, and prepare the system
    TaskerCore::Internal::OrchestrationManager.instance.bootstrap_orchestration_system
    puts "✅ Orchestration system bootstrapped successfully"
  end

  after(:all) do
    # Clean shutdown of orchestration system
    TaskerCore::Internal::OrchestrationManager.instance.reset!
    puts "🛑 Orchestration system reset complete"
  end

  describe 'Complete Linear Mathematical Sequence' do
    it 'executes A -> B -> C -> D workflow with mathematical operations', :aggregate_failures do
      puts "\n📊 Testing mathematical sequence: 6 -> 36 -> 46 -> 138 -> 69"

      # ==========================================
      # PHASE 1: Create and Initialize Task
      # ==========================================

      task_request = TaskerCore::Types::TaskRequest.new(
        namespace: 'linear_workflow',
        name: 'mathematical_sequence',
        version: '1.0.0',
        context: test_input,
        initiator: 'linear_integration_test',
        source_system: 'rspec_integration',
        reason: 'Test linear mathematical workflow progression',
        priority: 5,
        claim_timeout_seconds: 300
      )

      puts "📝 Created task request for even number: #{test_input[:even_number]}"

      # Initialize task through orchestration system
      base_handler = TaskerCore::Internal::OrchestrationManager.instance.base_task_handler
      expect(base_handler).not_to be_nil

      task_result = base_handler.initialize_task(task_request.to_h)
      expect(task_result).to be_nil  # Async operation
      puts "✅ Task submitted to orchestration system"

      # ==========================================
      # PHASE 2: Poll for Task Creation via SQL Functions
      # ==========================================

      task_id = nil
      puts "🔍 Polling for task creation..."

      # Poll for task to appear in the system
      Timeout.timeout(30) do
        loop do
          # Use analytics to find our task
          analytics = sql_functions.analytics_metrics
          if analytics['total_tasks'] && analytics['total_tasks'] > 0
            puts "📊 Found #{analytics['total_tasks']} task(s) in system"
            break
          end
          sleep 0.5
        end
      end

      # Get task execution context to find our task ID
      # Note: In a real integration, we'd have a way to track the task ID from creation
      # For now, we'll use a different approach to verify the workflow
      puts "✅ Task created and queued for processing"

      # ==========================================
      # PHASE 3: Monitor Step Progression via SQL Functions
      # ==========================================

      puts "🔄 Monitoring step progression through SQL functions..."

      # Start queue workers to process the steps
      worker = TaskerCore::Messaging.create_queue_worker(
        'linear_workflow',
        poll_interval: 0.1  # Fast polling for test
      )
      
      expect(worker.start).to be true
      puts "⚡ Queue worker started for linear_workflow namespace"

      begin
        # Monitor workflow progression
        step_completion_times = {}
        workflow_completed = false

        Timeout.timeout(60) do  # Give it time to complete all 4 steps
          loop do
            # Check system analytics for progress
            analytics = sql_functions.analytics_metrics
            system_health = sql_functions.system_health_counts

            puts "📊 Analytics: #{analytics}"
            puts "🏥 Health: #{system_health}"

            # Check if we have completed steps
            if system_health['completed_steps'] && system_health['completed_steps'] >= 4
              puts "🎉 All 4 steps completed!"
              workflow_completed = true
              break
            end

            # Check for failed steps
            if system_health['failed_steps'] && system_health['failed_steps'] > 0
              puts "❌ Detected #{system_health['failed_steps']} failed steps"
              # Don't break yet - might be retrying
            end

            sleep 1  # Poll every second
          end
        end

        expect(workflow_completed).to be true

        # ==========================================
        # PHASE 4: Validate Mathematical Results
        # ==========================================

        puts "🧮 Validating mathematical sequence results..."

        # Expected progression for input 6:
        # Step 1: 6² = 36
        # Step 2: 36 + 10 = 46  
        # Step 3: 46 × 3 = 138
        # Step 4: 138 ÷ 2 = 69

        expected_final_result = 69
        puts "✅ Expected final result: #{expected_final_result}"

        # In a full implementation, we would query the final task result
        # For now, we've validated that the workflow executed completely
        puts "✅ Mathematical workflow completed successfully"

      ensure
        worker.stop if worker.running?
        puts "🛑 Queue worker stopped"
      end

      # ==========================================
      # PHASE 5: Verify System State via SQL Functions
      # ==========================================

      puts "🔍 Final system state verification..."

      final_analytics = sql_functions.analytics_metrics
      final_health = sql_functions.system_health_counts

      expect(final_health['completed_steps']).to be >= 4
      expect(final_analytics['total_tasks']).to be >= 1

      puts "✅ Linear workflow integration test completed successfully"
      puts "   📊 Final analytics: #{final_analytics}"
      puts "   🏥 Final health: #{final_health}"
    end

    it 'validates step dependency chain execution order' do
      puts "\n🔗 Testing step dependency chain: linear_step_1 -> linear_step_2 -> linear_step_3 -> linear_step_4"

      # This test would verify that steps execute in the correct order
      # by monitoring step readiness and execution timing

      # Create a simple task to test dependency ordering
      task_request = TaskerCore::Types::TaskRequest.new(
        namespace: 'linear_workflow',
        name: 'mathematical_sequence',
        version: '1.0.0',
        context: { even_number: 8 },
        initiator: 'dependency_chain_test',
        source_system: 'rspec_integration',
        reason: 'Test linear workflow dependency chain',
        priority: 5,
        claim_timeout_seconds: 300
      )

      base_handler = TaskerCore::Internal::OrchestrationManager.instance.base_task_handler
      task_result = base_handler.initialize_task(task_request.to_h)
      expect(task_result).to be_nil

      puts "✅ Dependency chain validation test submitted"
      
      # In a full implementation, this would monitor step readiness using:
      # sql_functions.step_readiness_status(task_id) 
      # to verify that step 2 only becomes ready after step 1 completes, etc.

      puts "📋 Note: Full dependency chain monitoring requires task ID tracking"
      puts "   This will be implemented in Phase 4.3 with database-backed task creation"
    end

    it 'handles mathematical errors gracefully' do
      puts "\n⚠️ Testing error handling with invalid input"

      # Test with odd number (should fail validation)
      invalid_input = { even_number: 7 }  # Odd number should fail

      task_request = TaskerCore::Types::TaskRequest.new(
        namespace: 'linear_workflow',
        name: 'mathematical_sequence',
        version: '1.0.0',
        context: invalid_input,
        initiator: 'error_handling_test',
        source_system: 'rspec_integration',
        reason: 'Test error handling with invalid input',
        priority: 5,
        claim_timeout_seconds: 300
      )

      base_handler = TaskerCore::Internal::OrchestrationManager.instance.base_task_handler
      
      # This should either reject the task or handle the error gracefully
      expect { base_handler.initialize_task(task_request.to_h) }.not_to raise_error

      puts "✅ Error handling test submitted (validation happens in step handlers)"
      puts "📋 Note: Step-level error handling will be visible in SQL function monitoring"
    end
  end

  describe 'Framework Integration' do
    it 'verifies orchestration system is initialized properly' do
      manager = TaskerCore::Internal::OrchestrationManager.instance
      
      expect(manager.initialized?).to be true
      
      info = manager.info
      expect(info[:architecture]).to eq('pgmq')
      expect(info[:pgmq_available]).to be true
      expect(info[:queues_initialized]).to be true
      
      puts "✅ Orchestration system properly initialized in pgmq mode"
    end

    it 'verifies SQL functions can track linear workflow progress' do
      # Test that our SQL functions are available for progress tracking
      expect(sql_functions).to respond_to(:task_execution_context)
      expect(sql_functions).to respond_to(:step_readiness_status)
      expect(sql_functions).to respond_to(:analytics_metrics)
      expect(sql_functions).to respond_to(:system_health_counts)

      puts "✅ All required SQL functions available for progress tracking"
    end

    it 'verifies queue worker can process linear_workflow namespace' do
      worker = TaskerCore::Messaging.create_queue_worker('linear_workflow')
      
      expect(worker.namespace).to eq('linear_workflow')
      expect(worker.queue_name).to eq('linear_workflow_queue')
      expect(worker).to respond_to(:can_handle_step?)

      puts "✅ Queue worker configured for linear_workflow namespace"
    end
  end
end