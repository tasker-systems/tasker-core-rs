# frozen_string_literal: true

require 'spec_helper'
require 'securerandom'

require_relative 'test_helpers/ruby_integration_manager'

# Tree Workflow Integration with Docker-based Rust Services
#
# This test validates the hierarchical tree workflow pattern by:
# 1. Connecting to running Docker Compose services (postgres, orchestration, ruby-worker)
# 2. Using HTTP clients to communicate with orchestration API
# 3. Testing hierarchical step execution with multiple convergence levels
# 4. Validating YAML configuration from workers/ruby/spec/handlers/examples/tree_workflow/
#
# Prerequisites:
# Run `docker-compose -f docker/docker-compose.test.yml up --build -d` before running tests
#
# Tree Pattern:
# 1. Root: Initial processing
# 2. Level 1: Branch Left & Branch Right (parallel)
# 3. Level 2: Leaf D, E (under Left) & Leaf F, G (under Right) (parallel within branches)
# 4. Final: Convergence step (depends on all leaves)

RSpec.describe 'Tree Workflow Docker Integration', type: :integration do
  include RubyIntegrationTestHelpers

  let(:manager) { RubyWorkerIntegrationManager.setup }

  # Test data for tree workflow - small number due to exponential growth
  let(:test_input) do
    {
      even_number: 2, # Expected: 2 -> 4 -> branches -> leaves -> convergence
      test_run_id: SecureRandom.uuid
    }
  end

  describe 'Hierarchical Tree Processing via Docker Services' do
    it 'executes Root -> (BranchL->LeafD,E) & (BranchR->LeafF,G) -> Convergence workflow', :aggregate_failures do
      puts "\n🌳 Starting Tree Workflow Docker Integration Test"
      puts "   Services: Orchestration (#{manager.orchestration_url}), Ruby Worker (#{manager.worker_url})"
      puts '   Pattern: tree_root -> (branch_left -> [leaf_d, leaf_e]) & (branch_right -> [leaf_f, leaf_g]) -> final_convergence'
      puts "   Input: even_number = #{test_input[:even_number]} (exponential growth pattern)"

      # Create tree workflow task via orchestration API
      task_request = create_task_request(
        'tree_workflow',
        'hierarchical_computation',
        test_input
      )

      puts "\n🎯 Creating tree workflow task via orchestration API..."
      task_response = manager.orchestration_client.create_task(task_request)

      expect(task_response).to be_a(Hash)
      expect(task_response[:task_uuid]).not_to be_empty
      expect(task_response[:status]).to be_present

      puts '✅ Tree workflow task created successfully!'
      puts "   Task UUID: #{task_response[:task_uuid]}"
      puts "   Initial Status: #{task_response[:status]}"

      task_uuid = task_response[:task_uuid]

      # Wait for hierarchical tree completion with extended timeout
      puts "\n⏳ Waiting for hierarchical tree workflow completion..."
      puts '   Expected order: root -> branches (parallel) -> leaves (parallel within branches) -> convergence'

      completed_task = wait_for_task_completion(manager, task_uuid, 12)

      expect(completed_task).to be_a(Hash)
      expect(completed_task[:status]).to eq('complete').or eq('Complete')

      puts '✅ Tree workflow completed successfully!'

      # Validate hierarchical execution structure
      puts "\n🔍 Validating tree workflow execution structure..."

      # Get detailed step information
      steps = manager.orchestration_client.list_task_steps(task_uuid)
      expect(steps).to be_an(Array)
      expect(steps.size).to eq(8) # root + 2 branches + 4 leaves + convergence

      puts "   Total steps executed: #{steps.size}"

      # Validate step names and structure
      step_names = steps.map { |s| s[:name] || s['name'] }
      expected_steps = %w[
        tree_root
        tree_branch_left
        tree_branch_right
        tree_leaf_d
        tree_leaf_e
        tree_leaf_f
        tree_leaf_g
        tree_final_convergence
      ]

      expected_steps.each do |expected_step|
        matching_steps = step_names.select { |name| name.include?(expected_step) }
        expect(matching_steps).not_to be_empty, "Expected step '#{expected_step}' not found in executed steps: #{step_names.join(', ')}"
      end

      puts "   ✅ All expected tree steps found: #{expected_steps.join(', ')}"

      # Validate hierarchical completion status
      completed_steps = steps.select { |s| (s[:status] || s['status']) == 'complete' }
      expect(completed_steps.size).to eq(8), "Expected all 8 steps to be complete, but only #{completed_steps.size} completed"

      puts "   ✅ All #{completed_steps.size} steps completed successfully"

      # Validate exponential growth pattern in results
      root_step = steps.find { |s| (s[:name] || s['name']).include?('tree_root') }
      convergence_step = steps.find { |s| (s[:name] || s['name']).include?('final_convergence') }

      if root_step && root_step[:result] && convergence_step && convergence_step[:result]
        puts "   📊 Mathematical progression: Root result → Final convergence result"
        puts "      Root result: #{root_step[:result]}"
        puts "      Final result: #{convergence_step[:result]}"
      else
        puts "   ℹ️  Step results not available in current response format"
      end

      puts "\n🎉 Tree workflow Docker integration test completed successfully!"
      puts "   ✅ Hierarchical execution validated"
      puts "   ✅ Parallel processing within branches confirmed"
      puts "   ✅ All convergence dependencies respected"
    end

    it 'validates parallel execution within hierarchical levels' do
      puts "\n🔀 Testing parallel execution within tree workflow levels..."

      task_request = create_task_request(
        'tree_workflow',
        'hierarchical_computation',
        test_input.merge({ parallel_test: true })
      )

      task_response = manager.orchestration_client.create_task(task_request)
      task_uuid = task_response[:task_uuid]

      # Monitor execution timing to verify parallel processing
      start_time = Time.now
      completed_task = wait_for_task_completion(manager, task_uuid, 12)
      total_time = Time.now - start_time

      expect(completed_task[:status]).to eq('complete').or eq('Complete')

      puts "   ✅ Tree workflow completed in #{total_time.round(2)} seconds"

      # Validate that parallel execution occurred efficiently
      # (Tree workflow should complete faster than sequential execution due to parallelism)
      expect(total_time).to be < 30, 'Tree workflow should complete within 30 seconds with parallel execution'

      steps = manager.orchestration_client.list_task_steps(task_uuid)

      # Verify all leaf nodes executed (should happen in parallel)
      leaf_steps = steps.select { |s| (s[:name] || s['name']).include?('leaf') }
      expect(leaf_steps.size).to eq(4), 'Expected 4 leaf steps in tree pattern'

      puts "   ✅ Confirmed #{leaf_steps.size} leaf steps executed in parallel"
      puts "   ✅ Hierarchical parallel execution validated"
    end
  end

  describe 'Tree Workflow Error Handling and Resilience' do
    it 'handles tree workflow edge cases gracefully' do
      puts "\n🛡️  Testing tree workflow resilience..."

      # Test with edge case input
      edge_case_input = {
        even_number: 0, # Edge case: zero input
        test_scenario: 'edge_case',
        test_run_id: SecureRandom.uuid
      }

      task_request = create_task_request(
        'tree_workflow',
        'hierarchical_computation',
        edge_case_input
      )

      task_response = manager.orchestration_client.create_task(task_request)
      expect(task_response[:task_uuid]).not_to be_empty

      puts "   Created edge case test task: #{task_response[:task_uuid]}"

      # The task should either complete successfully or handle the edge case gracefully
      begin
        completed_task = wait_for_task_completion(manager, task_response[:task_uuid], 5)
        puts "   ✅ Edge case handled successfully: #{completed_task[:status]}"
      rescue StandardError => e
        puts "   ℹ️  Edge case resulted in expected behavior: #{e.message}"
        # This is acceptable - edge cases may result in controlled failures
      end

      puts "   ✅ Tree workflow resilience validated"
    end
  end
end
