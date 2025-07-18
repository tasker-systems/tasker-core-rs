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
  #     step = create_test_workflow_step(task_id: task.id, name: 'test_step')
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

      # Validate DATABASE_URL looks like a test database
      database_url = ENV['DATABASE_URL'] || 'postgresql://tasker:tasker@localhost/tasker_rust_test'
      unless database_url.include?('test') || database_url.include?('_test')
        if ENV['FORCE_ACCEPT_DB_URL']
          warn '⚠️  OVERRIDE: Accepting non-test-like DATABASE_URL due to FORCE_ACCEPT_DB_URL=true'
          warn "   DATABASE_URL: #{database_url}"
        else
          raise "❌ FATAL ERROR: DATABASE_URL does not appear to be a test database!\n   " \
                "DATABASE_URL: #{database_url}\n   " \
                "Expected: URL containing 'test' or '_test'\n   " \
                "This protects against accidentally running tests against production/development databases.\n   " \
                "To override: Set FORCE_ACCEPT_DB_URL=true if you're certain this is a test database\n   " \
                'Example: FORCE_ACCEPT_DB_URL=true bundle exec rspec'
        end
      end

      true
    end

    # ==========================================================================
    # DATABASE CONNECTION AND TRANSACTION MANAGEMENT
    # ==========================================================================

    def setup_test_database
      # Environment safety check before destructive operations
      TaskerCore::TestHelpers.validate_test_environment!

      # Run migrations to ensure schema is up to date
      run_migrations
      # Initialize foundation data if needed
      create_test_foundation
    end

    def run_migrations
      # Environment safety check before destructive operations
      TaskerCore::TestHelpers.validate_test_environment!

      # Run all migrations using Rust DatabaseMigrations
      database_url = ENV['DATABASE_URL'] || 'postgresql://tasker:tasker@localhost/tasker_rust_test'
      result = TaskerCore::TestHelpers.run_migrations(database_url)

      raise "Migration failed: #{result['error']}" if result.is_a?(Hash) && result['status'] == 'error'

      result.is_a?(Hash) ? result : {}
    end

    def drop_schema
      # Environment safety check before destructive operations
      TaskerCore::TestHelpers.validate_test_environment!

      # Drop and recreate schema using Rust DatabaseMigrations
      database_url = ENV['DATABASE_URL'] || 'postgresql://tasker:tasker@localhost/tasker_rust_test'
      result = TaskerCore::TestHelpers.drop_schema(database_url)

      raise "Schema drop failed: #{result['error']}" if result.is_a?(Hash) && result['status'] == 'error'

      result.is_a?(Hash) ? result : {}
    end

    def reset_test_database
      # Environment safety check before destructive operations
      TaskerCore::TestHelpers.validate_test_environment!

      # Complete database reset: drop schema + run migrations
      drop_schema
      run_migrations
    end
  end
end
