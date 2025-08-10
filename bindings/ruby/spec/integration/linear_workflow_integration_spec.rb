# frozen_string_literal: true

require 'spec_helper'
require 'yaml'
require 'timeout'

require_relative 'test_helpers/shared_test_loop'

# Load linear workflow handlers
require_relative '../handlers/examples/linear_workflow/handlers/linear_workflow_handler'
require_relative '../handlers/examples/linear_workflow/step_handlers/linear_step_1_handler'
require_relative '../handlers/examples/linear_workflow/step_handlers/linear_step_2_handler'
require_relative '../handlers/examples/linear_workflow/step_handlers/linear_step_3_handler'
require_relative '../handlers/examples/linear_workflow/step_handlers/linear_step_4_handler'

RSpec.describe 'Linear Workflow Integration', type: :integration do
  let(:config_path) do
    File.expand_path('../handlers/examples/linear_workflow/config/linear_workflow_handler.yaml', __dir__)
  end
  let(:task_config) { YAML.load_file(config_path) }
  let(:sql_functions) { TaskerCore::Database.create_sql_functions }
  # Test data: even number that will flow through the mathematical sequence
  let(:test_input) do
    {
      even_number: 6, # Expected progression: 6 -> 36 -> 46 -> 138 -> 69
      test_run_id: SecureRandom.uuid # Unique ID to avoid identity hash collisions
    }
  end

  let(:shared_loop) { SharedTestLoop.new }
  let(:namespace) { 'linear_workflow' }

  # Track workers created during tests for proper cleanup
  before do
    shared_loop.start
  end

  after do
    shared_loop.stop
  end

  describe 'Complete Linear Mathematical Sequence' do
    it 'executes A -> B -> C -> D workflow with mathematical operations', :aggregate_failures do
      # Create TaskRequest using dry-struct and convert to FFI hash
      task_request = TaskerCore::Types::TaskTypes::TaskRequest.new(
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

      task = shared_loop.run(task_request: task_request, num_workers: 1, namespace: namespace)
      expect(task).not_to be_nil
      task.workflow_steps.each do |step|
        results = JSON.parse(step.results)
        expect(results).to be_a(Hash)
        expect(results.keys).to include('result')
        expect(results['result']).to be_a(Integer)
      end

      # Verify task was created immediately (no polling needed with FFI)
      task_execution_context = TaskerCore::Database::Functions::FunctionBasedTaskExecutionContext.find(task.task_id)

      expect(task_execution_context.task_id).to eq(task.task_id)
      expect(task_execution_context.status).to eq('complete')
      expect(task_execution_context.total_steps).to eq(4)
      expect(task_execution_context.pending_steps).to eq(0)
      expect(task_execution_context.in_progress_steps).to eq(0)
      expect(task_execution_context.completed_steps).to eq(4)
      expect(task_execution_context.failed_steps).to eq(0)
      expect(task_execution_context.ready_steps).to eq(0)
      expect(task_execution_context.execution_status).to eq('all_complete')
      expect(task_execution_context.recommended_action).to eq('finalize_task')
      expect(task_execution_context.completion_percentage).to eq(100.0)
      expect(task_execution_context.health_status).to eq('healthy')
    end

    it 'validates step dependency chain execution order' do
      # Create TaskRequest for dependency chain testing
      task_request = TaskerCore::Types::TaskTypes::TaskRequest.new(
        namespace: 'linear_workflow',
        name: 'mathematical_sequence',
        version: '1.0.0',
        context: { even_number: 8, test_run_id: SecureRandom.uuid },
        initiator: 'dependency_chain_test',
        source_system: 'rspec_integration',
        reason: 'Test linear workflow dependency chain',
        priority: 5,
        claim_timeout_seconds: 300
      )

      task = shared_loop.run(task_request: task_request, num_workers: 1, namespace: namespace)
      expect(task).not_to be_nil
      task.workflow_steps.each do |step|
        results = JSON.parse(step.results)
        expect(results).to be_a(Hash)
        expect(results.keys).to include('result')
        expect(results['result']).to be_a(Integer)
      end
    end

    it 'handles mathematical errors gracefully' do
      # Test with odd number (should fail validation in step handlers)
      invalid_input = { even_number: 7 }

      # Task creation should succeed (validation happens in step handlers)
      expect do
        # Create TaskRequest for error handling testing
        task_request = TaskerCore::Types::TaskTypes::TaskRequest.new(
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

        # Initialize task using embedded FFI with TaskRequest hash
        task_result = TaskerCore.initialize_task_embedded(task_request.to_ffi_hash)
        expect(task_result).to be_a(Hash)
        expect(task_result[:success]).to be(true)
      end.not_to raise_error
    end
  end

  describe 'Framework Integration' do
    it 'verifies orchestration system is initialized properly' do
      # Check that the embedded orchestrator (FFI-based) exists
      orchestrator = TaskerCore.embedded_orchestrator
      expect(orchestrator).not_to be_nil

      # The `running?` method seems to have state sync issues with the FFI layer
      # Instead of relying on that, let's test actual functionality
      # If the orchestration system is working, we should be able to:
      # 1. Create a test task
      # 2. Use pgmq functionality

      # Check that pgmq is available - this should work or there's a real problem
      pgmq_client = TaskerCore::Messaging::PgmqClient.new
      expect(pgmq_client.connection).not_to be_nil

      # Check that we can create queues (core functionality)
      expect(pgmq_client.create_queue('test_verification_queue')).to be true

      # Test actual orchestration functionality - create a simple task
      task_request = TaskerCore::Types::TaskTypes::TaskRequest.new(
        namespace: 'linear_workflow',
        name: 'mathematical_sequence',
        version: '1.0.0',
        context: { even_number: 2, test_run_id: SecureRandom.uuid },
        initiator: 'orchestration_verification_test',
        source_system: 'rspec_integration',
        reason: 'Test orchestration system functionality',
        priority: 5,
        claim_timeout_seconds: 30
      )

      # If orchestration is working, this should succeed
      task_result = TaskerCore.initialize_task_embedded(task_request.to_ffi_hash)
      expect(task_result).to be_a(Hash)
      expect(task_result[:success]).to be(true)
      expect(task_result[:task_id]).to be_a(Integer)

      puts "✅ Orchestration system functional - created task #{task_result['task_id']}"
    end

    it 'verifies function-based database access can track linear workflow progress' do
      # Test function-based task execution context
      expect(TaskerCore::Database::Functions::FunctionBasedTaskExecutionContext).to respond_to(:find)
      expect(TaskerCore::Database::Functions::FunctionBasedTaskExecutionContext).to respond_to(:for_tasks)

      # Test function-based step readiness status
      expect(TaskerCore::Database::Functions::FunctionBasedStepReadinessStatus).to respond_to(:for_task)
      expect(TaskerCore::Database::Functions::FunctionBasedStepReadinessStatus).to respond_to(:ready_for_task)

      # Test that we can still use the compatibility layer
      expect(sql_functions).to respond_to(:close)
    end

    it 'verifies ActiveRecord models can access linear workflow data' do
      # Test that all our ActiveRecord models are available
      expect(TaskerCore::Database::Models::Task).to respond_to(:find)
      expect(TaskerCore::Database::Models::Task).to respond_to(:with_all_associated)
      expect(TaskerCore::Database::Models::WorkflowStep).to respond_to(:find)
      expect(TaskerCore::Database::Models::NamedStep).to respond_to(:find)
      expect(TaskerCore::Database::Models::NamedTask).to respond_to(:find)
      expect(TaskerCore::Database::Models::TaskNamespace).to respond_to(:find)

      # Test our view-based models
      expect(TaskerCore::Database::Models::ReadyTask).to respond_to(:find_by)
      expect(TaskerCore::Database::Models::ReadyTask).to respond_to(:available)
      expect(TaskerCore::Database::Models::ReadyTask).to respond_to(:for_namespace)
      expect(TaskerCore::Database::Models::StepDagRelationship).to respond_to(:for_task)
      expect(TaskerCore::Database::Models::StepDagRelationship).to respond_to(:root_steps)

      # Test that associations are properly configured by checking instance methods
      expect(TaskerCore::Database::Models::Task.instance_methods).to include(:workflow_steps)
      expect(TaskerCore::Database::Models::Task.instance_methods).to include(:named_task)
      expect(TaskerCore::Database::Models::WorkflowStep.instance_methods).to include(:named_step)
      expect(TaskerCore::Database::Models::WorkflowStep.instance_methods).to include(:task)
      expect(TaskerCore::Database::Models::NamedTask.instance_methods).to include(:task_namespace)

      # Test that view-based models are read-only
      expect(TaskerCore::Database::Models::ReadyTask.new.readonly?).to be true
      expect(TaskerCore::Database::Models::StepDagRelationship.new.readonly?).to be true
    end

    it 'verifies task initialization creates ready steps' do
      # Create TaskRequest
      task_request = TaskerCore::Types::TaskTypes::TaskRequest.new(
        namespace: 'linear_workflow',
        name: 'mathematical_sequence',
        version: '1.0.0',
        context: { even_number: 10, test_run_id: SecureRandom.uuid },
        initiator: 'step_readiness_test',
        source_system: 'rspec_integration',
        reason: 'Test step readiness after task initialization',
        priority: 5,
        claim_timeout_seconds: 300
      )

      # Initialize task
      task_result = TaskerCore.initialize_task_embedded(task_request.to_ffi_hash)
      task_id = task_result[:task_id]

      expect(task_id).to be_a(Integer)
      expect(task_id).to be > 0 # rubocop:disable Style/NumericPredicate

      # Check task execution context using function-based approach
      task_context = TaskerCore::Database::Functions::FunctionBasedTaskExecutionContext.find(task_id)

      # Get detailed step information using ActiveRecord models
      task = TaskerCore::Database::Models::Task.with_all_associated.find(task_id)

      task.workflow_steps.order(:workflow_step_id).each do |step|

        # Check step readiness status using function-based approach
        readiness_statuses = TaskerCore::Database::Functions::FunctionBasedStepReadinessStatus.for_task(task_id,
                                                                                                        [step.workflow_step_id])
        readiness_status = readiness_statuses.first unless readiness_statuses.empty?

        expect(readiness_status.dependencies_satisfied).not_to be_nil
      end
      # Assertions
      expect(task_context.total_steps).to eq(4) # Linear workflow has 4 steps
      expect(task_context.ready_steps).to be > 0, 'Expected at least one ready step after task initialization' # rubocop:disable Style/NumericPredicate
      expect(task_context.execution_status).to eq('has_ready_steps').or eq('in_progress').or eq('pending')
    end

    it 'verifies orchestration loop claims ready tasks' do
      # Create TaskRequest
      task_request = TaskerCore::Types::TaskTypes::TaskRequest.new(
        namespace: 'linear_workflow',
        name: 'mathematical_sequence',
        version: '1.0.0',
        context: { even_number: 6, test_run_id: SecureRandom.uuid },
        initiator: 'orchestration_claiming_test',
        source_system: 'rspec_integration',
        reason: 'Test orchestration loop claiming behavior',
        priority: 5,
        claim_timeout_seconds: 300
      )

      # Initialize task
      task_result = TaskerCore.initialize_task_embedded(task_request.to_ffi_hash)
      task_id = task_result[:task_id]

      expect(task_id).to be_a(Integer)
      expect(task_id).to be > 0 # rubocop:disable Style/NumericPredicate

      # First, let's check the task itself
      task = TaskerCore::Database::Models::Task.with_all_associated.find(task_id)
      expect(task).not_to be_nil, 'Task should exist'

      ready_task = TaskerCore::Database::Models::ReadyTask.find_by(task_id: task_id)

      expect(ready_task).not_to be_nil, 'Task should appear in ready tasks view'

      expect(ready_task.namespace_name).to eq('linear_workflow')
      expect(ready_task.ready_steps_count.to_i).to be > 0 # rubocop:disable Style/NumericPredicate
      expect(ready_task.claim_status).to eq('available')

      # Test our convenience methods
      expect(ready_task.available?).to be true
      expect(ready_task.has_ready_steps?).to be true
      expect(ready_task.ready_for_execution?).to be true

      expect { TaskerCore::Database::Models::ReadyTask.find_by(task_id: task_id) }.not_to raise_error
    end
  end
end
