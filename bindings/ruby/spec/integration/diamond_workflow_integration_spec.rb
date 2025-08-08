# frozen_string_literal: true

require 'spec_helper'
require 'yaml'
require 'timeout'

require_relative 'test_helpers/shared_test_loop'

# Load diamond workflow handlers
require_relative '../handlers/examples/diamond_workflow/handlers/diamond_workflow_handler'
require_relative '../handlers/examples/diamond_workflow/step_handlers/diamond_start_handler'
require_relative '../handlers/examples/diamond_workflow/step_handlers/diamond_branch_b_handler'
require_relative '../handlers/examples/diamond_workflow/step_handlers/diamond_branch_c_handler'
require_relative '../handlers/examples/diamond_workflow/step_handlers/diamond_end_handler'

RSpec.describe 'Diamond Workflow Integration', type: :integration do
  let(:config_path) do
    File.expand_path('../handlers/examples/diamond_workflow/config/diamond_workflow_handler.yaml', __dir__)
  end
  let(:task_config) { YAML.load_file(config_path) }
  let(:sql_functions) { TaskerCore::Database.create_sql_functions }

  # Test data: even number for diamond pattern computation
  let(:test_input) do
    {
      even_number: 4 # Expected: 4 -> 16 -> (16+25=41, 16×2=32) -> avg(41,32)=36.5
    }
  end

  let(:namespace) { 'diamond_workflow' }
  let(:shared_loop) { SharedTestLoop.new }

  before do
    shared_loop.start
  end

  after do
    shared_loop.stop
  end

  describe 'Diamond Pattern with Parallel Processing' do
    it 'executes A -> (B, C) -> D workflow with parallel branches', :aggregate_failures do
      task_request = TaskerCore::Types::TaskRequest.new(
        namespace: 'diamond_workflow',
        name: 'parallel_computation',
        version: '1.0.0',
        context: test_input.merge({ random_uuid: SecureRandom.uuid }),
        initiator: 'diamond_integration_test',
        source_system: 'rspec_integration',
        reason: 'Test diamond workflow with parallel branches',
        priority: 5,
        claim_timeout_seconds: 300
      )

      # Use SharedTestLoop with 2 workers to demonstrate parallel processing
      task = shared_loop.run(task_request: task_request, num_workers: 2, namespace: namespace)
      expect(task).not_to be_nil

      # Validate all steps completed with expected results
      expect(task.workflow_steps.count).to eq(4) # start, branch_b, branch_c, end

      task.workflow_steps.each do |step|
        results = JSON.parse(step.results)
        expect(results).to be_a(Hash)
        expect(results.keys).to include('result')

        # Validate specific step results
        case step.name
        when 'diamond_start'
          expect(results['result']).to eq(16) # 4² = 16
        when 'diamond_branch_b'
          expect(results['result']).to eq(256)
        when 'diamond_branch_c'
          expect(results['result']).to eq(256)
        when 'diamond_end'
          expect(results['result']).to eq(4_294_967_296)
        end
      end
    end

    it 'validates dependency convergence with multiple parents' do
      puts "\n🔗 Testing dependency convergence: diamond_end depends on (branch_b, branch_c)"

      task_request = TaskerCore::Types::TaskRequest.new(
        namespace: 'diamond_workflow',
        name: 'parallel_computation',
        version: '1.0.0',
        context: { even_number: 8, random_uuid: SecureRandom.uuid },
        initiator: 'convergence_test',
        source_system: 'rspec_integration',
        reason: 'Test diamond convergence with multiple dependencies',
        priority: 5,
        claim_timeout_seconds: 300
      )

      task = shared_loop.run(task_request: task_request, num_workers: 2, namespace: namespace)
      expect(task).not_to be_nil

      # Verify the diamond_end step ran after both branches
      diamond_end = task.workflow_steps.find { |s| s.name == 'diamond_end' }
      expect(diamond_end).not_to be_nil

      results = JSON.parse(diamond_end.results)
      expect(results['result']).to eq(281_474_976_710_656)

      puts '✅ Convergence test completed - diamond_end executed after both branches'
    end

    it 'demonstrates parallel processing with larger input' do
      puts "\n⚡ Testing with larger input to verify parallel execution"

      task_request = TaskerCore::Types::TaskRequest.new(
        namespace: 'diamond_workflow',
        name: 'parallel_computation',
        version: '1.0.0',
        context: { even_number: 10, random_uuid: SecureRandom.uuid },
        initiator: 'performance_test',
        source_system: 'rspec_integration',
        reason: 'Measure parallel execution performance',
        priority: 10, # High priority
        claim_timeout_seconds: 300
      )

      # Use 3 workers to maximize parallelism
      task = shared_loop.run(task_request: task_request, num_workers: 3, namespace: namespace)
      expect(task).not_to be_nil

      diamond_end = task.workflow_steps.find { |s| s.name == 'diamond_end' }
      results = JSON.parse(diamond_end.results)
      expect(results['result']).to eq(10_000_000_000_000_000)

      puts '✅ Performance test completed with high priority and multiple workers'
    end
  end
end
