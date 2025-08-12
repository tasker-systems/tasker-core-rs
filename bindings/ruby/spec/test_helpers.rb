# frozen_string_literal: true

require 'json'

module TaskerCore
  # Test helpers for creating test data using Rust factory patterns
  #
  # Since TaskerCore is a standalone gem without Rails transactional specs,
  # these helpers provide:
  # 1. Access to Rust factory patterns for creating test data
  # 2. Database cleanup utilities
  # 3. Convenient Ruby APIs for common test scenarios
  # 4. Environment safety checks to prevent destructive operations in non-test environments
  #
  # ## Usage
  #
  # ```ruby
  # include TaskerCore::TestHelpers
  #
  # describe 'my workflow' do
  #
  #   it 'processes tasks' do
  #     task = create_test_task(name: 'test_task', context: { test: true })
  #     step = create_test_workflow_step(task_uuid: task.id, name: 'test_step')
  #
  #     # Your test logic here
  #   end
  # end
  # ```
  module TestHelpers
    # ==========================================================================
    # ENVIRONMENT SAFETY VALIDATION
    # ==========================================================================

    # Validate that we're running in a test environment before any destructive operations
    def self.validate_test_environment!
      environment_variables = %w[RAILS_ENV APP_ENV RACK_ENV TASKER_ENV]
      current_env = environment_variables.find { |var| ENV.fetch(var, nil) }&.then { |var| ENV.fetch(var, nil) }

      if current_env && current_env.downcase != 'test'
        raise "❌ FATAL ERROR: Refusing to run destructive database operations!\n   " \
              "Current environment: #{current_env}\n   " \
              "Expected: 'test'\n   " \
              "This protects against accidentally running operations against production/development databases.\n   " \
              'To fix: Set RAILS_ENV=test (or APP_ENV=test, RACK_ENV=test, TASKER_ENV=test)'
      end

      ENV['TASKER_ENV'] ||= 'test'
      true
    end
  end
end
