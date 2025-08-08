# frozen_string_literal: true

require 'spec_helper'
require 'yaml'
require 'timeout'

require_relative 'test_helpers/shared_test_loop'

# Load tree workflow handlers
require_relative '../handlers/examples/tree_workflow/handlers/tree_workflow_handler'
require_relative '../handlers/examples/tree_workflow/step_handlers/tree_root_handler'
require_relative '../handlers/examples/tree_workflow/step_handlers/tree_branch_left_handler'
require_relative '../handlers/examples/tree_workflow/step_handlers/tree_branch_right_handler'
require_relative '../handlers/examples/tree_workflow/step_handlers/tree_leaf_d_handler'
require_relative '../handlers/examples/tree_workflow/step_handlers/tree_leaf_e_handler'
require_relative '../handlers/examples/tree_workflow/step_handlers/tree_leaf_f_handler'
require_relative '../handlers/examples/tree_workflow/step_handlers/tree_leaf_g_handler'
require_relative '../handlers/examples/tree_workflow/step_handlers/tree_final_convergence_handler'

RSpec.describe 'Tree Workflow Integration', type: :integration do
  let(:config_path) do
    File.expand_path('../handlers/examples/tree_workflow/config/tree_workflow_handler.yaml', __dir__)
  end
  let(:task_config) { YAML.load_file(config_path) }
  let(:sql_functions) { TaskerCore::Database.create_sql_functions }

  # Test data: small even number for tree pattern computation (exponential growth!)
  let(:test_input) do
    {
      even_number: 2 # Expected: 2 -> 4 -> 16 -> 256 (×4) -> 256² = 65536
    }
  end

  let(:namespace) { 'tree_workflow' }
  let(:shared_loop) { SharedTestLoop.new }

  before do
    shared_loop.start
  end

  after do
    shared_loop.stop
  end

  describe 'Hierarchical Tree Pattern with Multiple Convergence Levels' do
    it 'executes A -> (B -> (D, E), C -> (F, G)) -> H workflow with hierarchical processing', :aggregate_failures do
      task_request = TaskerCore::Types::TaskRequest.new(
        namespace: 'tree_workflow',
        name: 'hierarchical_computation',
        version: '1.0.0',
        context: test_input.merge({ random_uuid: SecureRandom.uuid }),
        initiator: 'tree_integration_test',
        source_system: 'rspec_integration',
        reason: 'Test tree workflow with hierarchical branching',
        priority: 5,
        claim_timeout_seconds: 300
      )

      # Use SharedTestLoop with 3 workers for parallel leaf processing
      task = shared_loop.run(task_request: task_request, num_workers: 3, namespace: namespace, timeout: 15)
      expect(task).not_to be_nil

      # Validate all 8 steps completed
      expect(task.workflow_steps.count).to eq(8) # root + 2 branches + 4 leaves + convergence

      task.workflow_steps.each do |step|
        results = JSON.parse(step.results)
        expect(results).to be_a(Hash)
        expect(results.keys).to include('result')
        expect(results['result']).to be_a(Integer)

        # For input 2, results are powers of 2: 4, 16, 256, etc.
        # Final result: 2^32 = 4294967296
      end

      # Verify the final convergence step has the expected result
      final_step = task.workflow_steps.find { |s| s.name == 'tree_final_convergence' }
      expect(final_step).not_to be_nil
      final_results = JSON.parse(final_step.results)
      expect(final_results['result']).to eq(18_446_744_073_709_551_616)
    end

    it 'validates hierarchical dependency convergence with multiple levels' do
      puts "\n🔗 Testing hierarchical convergence: leaves depend on branches, convergence depends on all leaves"

      task_request = TaskerCore::Types::TaskRequest.new(
        namespace: 'tree_workflow',
        name: 'hierarchical_computation',
        version: '1.0.0',
        context: { even_number: 2, random_uuid: SecureRandom.uuid }, # Keep small!
        initiator: 'hierarchical_test',
        source_system: 'rspec_integration',
        reason: 'Test tree hierarchical convergence dependencies',
        priority: 5,
        claim_timeout_seconds: 300
      )

      task = shared_loop.run(task_request: task_request, num_workers: 2, namespace: namespace, timeout: 15)
      expect(task).not_to be_nil

      # Verify the final step executed after all leaves
      final_step = task.workflow_steps.find { |s| s.name == 'tree_final_convergence' }
      expect(final_step).not_to be_nil
      expect(final_step.results).to be_a(String)
      expect(JSON.parse(final_step.results)['result']).to eq(18_446_744_073_709_551_616)

      puts '✅ Hierarchical convergence test completed - final step executed after all leaves'
    end

    it 'demonstrates exponential growth handling' do
      puts "\n📈 Demonstrating exponential growth with small inputs"

      # Use very small input due to exponential nature
      task_request = TaskerCore::Types::TaskRequest.new(
        namespace: 'tree_workflow',
        name: 'hierarchical_computation',
        version: '1.0.0',
        context: { even_number: 2, random_uuid: SecureRandom.uuid },
        initiator: 'exponential_test',
        source_system: 'rspec_integration',
        reason: 'Test handling of exponential mathematical growth',
        priority: 5,
        claim_timeout_seconds: 300
      )

      task = shared_loop.run(task_request: task_request, num_workers: 4, namespace: namespace, timeout: 15)
      expect(task).not_to be_nil

      # Verify exponential growth handling
      final_step = task.workflow_steps.find { |s| s.name == 'tree_final_convergence' }
      final_results = JSON.parse(final_step.results)

      puts '✅ Exponential growth test completed'
      puts '📊 Tree pattern creates exponential growth: n -> n² -> n⁴ -> n⁸ -> n³²'
      puts "   Input 2: 2³² = #{final_results['result']}"
      expect(final_results['result']).to eq(18_446_744_073_709_551_616)
    end
  end
end
