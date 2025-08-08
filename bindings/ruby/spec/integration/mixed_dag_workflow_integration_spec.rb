# frozen_string_literal: true

require 'spec_helper'
require 'yaml'
require 'timeout'

require_relative 'test_helpers/shared_test_loop'

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

  let(:namespace) { 'mixed_dag_workflow' }
  let(:shared_loop) { SharedTestLoop.new }

  before do
    shared_loop.start
  end

  after do
    shared_loop.stop
  end

  describe 'Complex Mixed DAG Pattern with Multiple Convergence Types' do
    it 'executes A->B,C->D (B,C), B->E, C->F, (D,E,F)->G workflow with mixed convergence', :aggregate_failures do
      task_request = TaskerCore::Types::TaskRequest.new(
        namespace: 'mixed_dag_workflow',
        name: 'complex_dag_computation',
        version: '1.0.0',
        context: test_input.merge({ random_uuid: SecureRandom.uuid }),
        initiator: 'mixed_dag_integration_test',
        source_system: 'rspec_integration',
        reason: 'Test mixed DAG workflow with multiple convergence patterns',
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

    it 'validates mixed convergence patterns with single and multiple parents' do
      puts "\n🔗 Testing mixed convergence: some steps have single parents, others have multiple"

      task_request = TaskerCore::Types::TaskRequest.new(
        namespace: 'mixed_dag_workflow',
        name: 'complex_dag_computation',
        version: '1.0.0',
        context: { even_number: 2, random_uuid: SecureRandom.uuid },
        initiator: 'mixed_convergence_test',
        source_system: 'rspec_integration',
        reason: 'Test mixed DAG convergence with different dependency types',
        priority: 5,
        claim_timeout_seconds: 300
      )

      task = shared_loop.run(task_request: task_request, num_workers: 2, namespace: namespace)
      expect(task).not_to be_nil
      task.workflow_steps.each do |step|
        results = JSON.parse(step.results)
        expect(results).to be_a(Hash)
        expect(results.keys).to include('result')
        expect(results['result']).to be_a(Integer)
      end
    end

    it 'demonstrates extreme exponential growth with very small inputs' do
      # Use the smallest possible input due to n^64 growth!
      task_request = TaskerCore::Types::TaskRequest.new(
        namespace: 'mixed_dag_workflow',
        name: 'complex_dag_computation',
        version: '1.0.0',
        context: { even_number: 4, random_uuid: SecureRandom.uuid },
        initiator: 'extreme_growth_test',
        source_system: 'rspec_integration',
        reason: 'Test extreme exponential growth handling (n^64)',
        priority: 5,
        claim_timeout_seconds: 300
      )

      task = shared_loop.run(task_request: task_request, num_workers: 3, namespace: namespace)
      expect(task).not_to be_nil
      task.workflow_steps.each do |step|
        results = JSON.parse(step.results)
        expect(results).to be_a(Hash)
        expect(results.keys).to include('result')
        expect(results['result']).to be_a(Integer)
      end
    end
  end
end
