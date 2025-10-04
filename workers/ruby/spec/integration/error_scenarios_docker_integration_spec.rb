# frozen_string_literal: true

require 'spec_helper'
require_relative 'test_helpers/ruby_integration_manager'

RSpec.describe 'Error Scenarios - Docker Integration Tests', :docker_integration do
  include RubyIntegrationTestHelpers

  let(:manager) { RubyWorkerIntegrationManager.setup }

  before(:all) do
    puts "\n" + "=" * 80
    puts "ERROR SCENARIO INTEGRATION TESTS - Simplified Core Patterns"
    puts "=" * 80
    puts "Testing 3 core error handling patterns:"
    puts "  1. Happy path (success_step) - immediate success"
    puts "  2. Permanent failure (permanent_error_step) - fail fast, no retries"
    puts "  3. Retryable failure (retryable_error_step) - retry with backoff, then fail"
    puts "=" * 80 + "\n"
  end

  describe 'Happy Path - Success Scenario' do
    it 'executes successfully on first attempt' do
      # Create task with only success step
      task_request = create_task_request(
        'test_errors',
        'error_testing',
        scenario_type: 'success',
        bypass_steps: ['permanent_error_step', 'retryable_error_step']
      )

      response = manager.orchestration_client.create_task(task_request)
      task_uuid = response[:task_uuid]

      expect(task_uuid).to be_present

      # Wait for completion - should be fast (< 2 seconds)
      completed_task = wait_for_task_completion(manager, task_uuid, 3)

      # Verify success
      expect(completed_task[:status]).to eq('complete')
      expect(completed_task[:execution_status]).to eq('complete')
      expect(completed_task[:total_steps]).to eq(1)
      expect(completed_task[:completed_steps]).to eq(1)
      expect(completed_task[:failed_steps]).to eq(0)

      puts "✅ Success scenario completed in < 3 seconds"
    end
  end

  describe 'Permanent Failure Scenario' do
    it 'fails immediately without retries' do
      # Create task with only permanent error step
      task_request = create_task_request(
        'test_errors',
        'error_testing',
        scenario_type: 'permanent_failure',
        bypass_steps: ['success_step', 'retryable_error_step']
      )

      response = manager.orchestration_client.create_task(task_request)
      task_uuid = response[:task_uuid]

      expect(task_uuid).to be_present

      # Wait for failure - should be fast since no retries
      start_time = Time.now
      failed_task = wait_for_task_failure(manager, task_uuid, 3)
      elapsed = Time.now - start_time

      # Verify it failed quickly (no retry delays)
      expect(elapsed).to be < 2.0

      # Verify failure state
      expect(failed_task[:execution_status]).to match(/error|blocked/)
      expect(failed_task[:total_steps]).to eq(1)
      expect(failed_task[:completed_steps]).to eq(0)
      expect(failed_task[:failed_steps]).to eq(1)

      # Verify step is in error state (not waiting for retry)
      error_step = failed_task[:steps].find { |s| s[:step_name] == 'permanent_error_step' }
      expect(error_step[:state]).to eq('error')
      expect(error_step[:is_retryable]).to be_falsey

      puts "✅ Permanent failure completed in #{elapsed.round(2)}s (no retries)"
    end
  end

  describe 'Retryable Failure Scenario' do
    it 'retries with backoff then fails after exhausting retries' do
      # Create task with only retryable error step
      task_request = create_task_request(
        'test_errors',
        'error_testing',
        scenario_type: 'retryable_failure',
        bypass_steps: ['success_step', 'permanent_error_step']
      )

      response = manager.orchestration_client.create_task(task_request)
      task_uuid = response[:task_uuid]

      expect(task_uuid).to be_present

      # Wait for failure - will take longer due to retries
      # With 2 retries at 50ms base: ~150-200ms total
      start_time = Time.now
      failed_task = wait_for_task_failure(manager, task_uuid, 5)
      elapsed = Time.now - start_time

      # Verify it took time for retries but not too long
      expect(elapsed).to be < 3.0
      expect(elapsed).to be > 0.1 # At least some retry delay

      # Verify failure state after retry exhaustion
      expect(failed_task[:execution_status]).to match(/error|blocked/)
      expect(failed_task[:total_steps]).to eq(1)
      expect(failed_task[:completed_steps]).to eq(0)
      expect(failed_task[:failed_steps]).to eq(1)

      # Verify step exhausted retries
      error_step = failed_task[:steps].find { |s| s[:step_name] == 'retryable_error_step' }
      expect(error_step[:state]).to eq('error')
      expect(error_step[:is_retryable]).to be_truthy
      expect(error_step[:retry_count]).to be >= 2 # Should have retried at least twice

      puts "✅ Retryable failure completed in #{elapsed.round(2)}s (with #{error_step[:retry_count]} retries)"
    end
  end

  describe 'Mixed Workflow Validation' do
    it 'can handle workflow with both success and failure steps' do
      # This validates the orchestration can handle mixed outcomes
      # Note: Due to no dependencies, all steps run in parallel

      task_request = create_task_request(
        'test_errors',
        'error_testing',
        scenario_type: 'mixed',
        bypass_steps: [] # Run all steps
      )

      response = manager.orchestration_client.create_task(task_request)
      task_uuid = response[:task_uuid]

      expect(task_uuid).to be_present

      # Wait for completion or failure (some steps will succeed, others fail)
      # This should fail overall because error steps will fail
      failed_task = wait_for_task_failure(manager, task_uuid, 5)

      # Verify mixed results
      expect(failed_task[:total_steps]).to eq(3)
      expect(failed_task[:completed_steps]).to be >= 1 # At least success_step
      expect(failed_task[:failed_steps]).to be >= 1 # At least one error step

      puts "✅ Mixed workflow handled correctly: #{failed_task[:completed_steps]} succeeded, #{failed_task[:failed_steps]} failed"
    end
  end
end
