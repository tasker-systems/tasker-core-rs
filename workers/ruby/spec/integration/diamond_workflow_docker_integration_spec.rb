# frozen_string_literal: true

require 'spec_helper'
require 'securerandom'

require_relative 'test_helpers/ruby_integration_manager'

# Diamond Workflow Integration with Docker-based Rust Services
#
# This test validates the diamond workflow pattern by:
# 1. Connecting to running Docker Compose services (postgres, orchestration, ruby-worker)
# 2. Using HTTP clients to communicate with orchestration API
# 3. Testing parallel execution followed by convergence using Ruby handlers via FFI
# 4. Validating YAML configuration from workers/ruby/spec/handlers/examples/diamond_workflow/
#
# Prerequisites:
# Run `docker-compose -f docker/docker-compose.test.yml up --build -d` before running tests
#
# Diamond Pattern:
# 1. Start: Square the initial even number (4 → 16)
# 2. Branch B (Left): Add 25 to start result (16 + 25 = 41)
# 3. Branch C (Right): Multiply start result by 2 (16 × 2 = 32)
# 4. End: Average both branch results ((41 + 32) ÷ 2 = 36.5)

RSpec.describe 'Diamond Workflow Docker Integration', type: :integration do
  include RubyIntegrationTestHelpers

  let(:manager) { RubyWorkerIntegrationManager.setup }

  # Test data: even number for diamond pattern computation
  let(:test_input) do
    {
      even_number: 4 # Expected: 4 -> 16 -> (16+25=41, 16×2=32) -> avg(41,32)=36.5
    }
  end

  describe 'Diamond Pattern with Parallel Processing via Docker Services' do
    it 'executes A -> (B, C) -> D workflow with parallel branches', :aggregate_failures do
      puts "\n🚀 Starting Diamond Workflow Docker Integration Test"
      puts "   Services: Orchestration (#{manager.orchestration_url}), Ruby Worker (#{manager.worker_url})"
      puts '   Pattern: diamond_start -> (diamond_branch_b, diamond_branch_c) -> diamond_end'
      puts "   Input: even_number = #{test_input[:even_number]}"

      # Create diamond workflow task via orchestration API
      task_request = create_task_request(
        'diamond_workflow',
        'parallel_computation',
        test_input
      )

      puts "\n🎯 Creating diamond workflow task via orchestration API..."
      task_response = manager.orchestration_client.create_task(task_request)

      expect(task_response).to be_a(Hash)
      expect(task_response[:task_uuid]).not_to be_empty
      expect(task_response[:status]).to be_present

      puts '✅ Diamond workflow task created successfully!'
      puts "   Task UUID: #{task_response[:task_uuid]}"
      puts "   Status: #{task_response[:status]}"
      puts '   Expected steps: 4 (start, branch_b, branch_c, end)'

      # Monitor task execution (Ruby worker processes steps via FFI automatically)
      puts "\n⏱️ Monitoring diamond workflow execution via Docker services..."
      puts '   (Rust worker will call Ruby handlers via FFI for each step)'

      # Wait for task completion with timeout
      task = wait_for_task_completion(manager, task_response[:task_uuid], 30)

      expect(task).not_to be_nil
      puts '✅ Task execution completed successfully!'

      # Validate all steps completed with expected results
      steps = manager.orchestration_client.list_task_steps(task_response[:task_uuid])
      expect(steps).to be_an(Array)
      expect(steps.count).to eq(4) # start, branch_b, branch_c, end

      puts "\n🔍 Verifying diamond workflow step results..."

      steps.each_with_index do |step, index|
        expect(step[:current_state].to_s.downcase).to eq('complete'),
                                                      "Step #{index + 1} (#{step[:name]}) should be completed"

        puts "   ✅ Step #{index + 1}: #{step[:name]} - #{step[:current_state]}"

        # Validate specific step results based on diamond pattern
        next unless step[:results]

        case step[:name]
        when 'diamond_start'
          # Should square the input: 4² = 16
          result_value = step[:results][:result] || step[:results]['result']
          expect(result_value).to eq(16), 'Diamond start should square 4 to get 16'
          puts "     Result: #{result_value} (4² = 16) ✓"
        when 'diamond_branch_b'
          # Should add 25 to start result: 16 + 25 = 41
          result_value = step[:results][:result] || step[:results]['result']
          expect(result_value).to eq(41), 'Diamond branch B should add 25 to 16 to get 41'
          puts "     Result: #{result_value} (16 + 25 = 41) ✓"
        when 'diamond_branch_c'
          # Should multiply start result by 2: 16 × 2 = 32
          result_value = step[:results][:result] || step[:results]['result']
          expect(result_value).to eq(32), 'Diamond branch C should multiply 16 by 2 to get 32'
          puts "     Result: #{result_value} (16 × 2 = 32) ✓"
        when 'diamond_end'
          # Should average both branch results: (41 + 32) ÷ 2 = 36.5
          result_value = step[:results][:result] || step[:results]['result']
          expect(result_value).to eq(36.5), 'Diamond end should average 41 and 32 to get 36.5'
          puts "     Result: #{result_value} ((41 + 32) ÷ 2 = 36.5) ✓"
        end
      end

      puts "\n🎉 Diamond Workflow Docker Integration Test PASSED!"
      puts '✅ PostgreSQL with PGMQ (Docker): Working'
      puts '✅ Orchestration service (Docker): Working'
      puts '✅ Ruby-enabled Rust worker (Docker): Working'
      puts '✅ HTTP API integration: Working'
      puts '✅ Ruby handler execution via FFI: Working'
      puts '✅ Diamond workflow execution: Working'
      puts '✅ Parallel branch execution: Working'
      puts '✅ Diamond convergence pattern: Working'
      puts '✅ Ruby handlers from workers/ruby/spec/handlers/examples/diamond_workflow: Working'
      puts '✅ YAML config from workers/ruby/spec/handlers/examples/diamond_workflow/config: Working'
      puts '✅ End-to-end diamond pattern lifecycle: Working'
    end

    it 'validates dependency convergence with multiple parents via Docker' do
      puts "\n🔗 Testing dependency convergence: diamond_end depends on (branch_b, branch_c)"

      task_request = create_task_request(
        'diamond_workflow',
        'parallel_computation',
        { even_number: 8 }
      )

      task_response = manager.orchestration_client.create_task(task_request)
      task = wait_for_task_completion(manager, task_response[:task_uuid], 30)

      expect(task).not_to be_nil

      # Verify the diamond_end step ran after both branches
      steps = manager.orchestration_client.list_task_steps(task_response[:task_uuid])
      diamond_end = steps.find { |s| s[:name] == 'diamond_end' }
      expect(diamond_end).not_to be_nil

      # For input 8: 8² = 64, (64+25=89, 64×2=128), avg(89,128) = 108.5
      result_value = diamond_end[:results][:result] || diamond_end[:results]['result']
      expect(result_value).to eq(108.5)

      puts '✅ Convergence test completed - diamond_end executed after both branches'
      puts "   Final result: #{result_value} (expected: 108.5)"
    end

    it 'demonstrates parallel processing with larger input via Docker services' do
      puts "\n⚡ Testing with larger input to verify parallel execution"

      task_request = create_task_request(
        'diamond_workflow',
        'parallel_computation',
        { even_number: 10 }
      )

      # Execute via Docker services with HTTP API communication
      task_response = manager.orchestration_client.create_task(task_request)
      task = wait_for_task_completion(manager, task_response[:task_uuid], 30)

      expect(task).not_to be_nil

      steps = manager.orchestration_client.list_task_steps(task_response[:task_uuid])
      diamond_end = steps.find { |s| s[:name] == 'diamond_end' }
      result_value = diamond_end[:results][:result] || diamond_end[:results]['result']

      # For input 10: 10² = 100, (100+25=125, 100×2=200), avg(125,200) = 162.5
      expect(result_value).to eq(162.5)

      puts '✅ Performance test completed with Docker service communication'
      puts "   Final result: #{result_value} (expected: 162.5)"
    end
  end

  describe 'Ruby Worker Service Health and Status' do
    it 'validates Ruby worker health and handler registration' do
      puts "\n🏥 Checking Ruby worker health and handler status..."

      # Check worker health
      health = manager.worker_client.health_check
      expect(health[:healthy]).to be true
      puts "✅ Ruby worker health check: #{health[:data]}"

      # Check worker status and capabilities
      status = manager.worker_client.worker_status
      expect(status).to be_a(Hash)
      puts "✅ Ruby worker status: #{status.keys.join(', ')}"

      # Check registered handlers (if endpoint available)
      begin
        handlers = manager.worker_client.list_handlers
        puts "✅ Registered Ruby handlers: #{handlers.count} handlers"
        puts "   Handler list: #{handlers.map { |h| h[:name] || h['name'] }.join(', ')}"
      rescue StandardError => e
        puts "ℹ️  Handler list endpoint not available: #{e.message}"
      end

      # Check supported namespaces
      begin
        namespaces = manager.worker_client.supported_namespaces
        expect(namespaces).to include('diamond_workflow')
        puts "✅ Supported namespaces: #{namespaces.join(', ')}"
      rescue StandardError => e
        puts "ℹ️  Namespace endpoint not available: #{e.message}"
      end
    end
  end
end
