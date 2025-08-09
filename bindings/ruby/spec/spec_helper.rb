# frozen_string_literal: true

require 'rspec'
require 'json'
require 'yaml'
require 'time'
require 'securerandom'
require 'dotenv'
# require_relative 'domain_helpers'  # Not needed for pgmq architecture tests

def set_environment_variables
  Dotenv.load
  ENV['RUBY_ENV'] = 'test'
  ENV['RAILS_ENV'] = 'test'
  ENV['TASKER_ENV'] = 'test'
  ENV['TASKER_EMBEDDED_MODE'] = 'true'
end

def database_url
  ENV['DATABASE_URL'] || 'postgresql://tasker:tasker@localhost/tasker_rust_test'
end

def cleanup_workers
  # Clean up any queue workers created during the test
  return unless defined?(@test_workers) && @test_workers

  @test_workers.each do |worker|
    if worker.running?
      worker.stop
      # Give worker a moment to fully stop
      sleep 0.1
    end
  rescue TaskerCore::Errors::WorkerError, ArgumentError => e
    puts "⚠️ Failed to stop test worker: #{e.message}"
  end
  @test_workers.clear
end

def safely_shutdown_db_connections
  begin
    TaskerCore.teardown_test_database(database_url)
    # Check for connection issues without forcefully disconnecting
    if defined?(ActiveRecord) && ActiveRecord::Base.connected?
      # Just verify connection is still good, don't disconnect unless there's a problem
      ActiveRecord::Base.connection.active?
    end
  rescue ActiveRecord::ConnectionNotEstablished,
         ActiveRecord::ConnectionTimeoutError,
         PG::ConnectionBad,
         PG::UnableToSend => e
    puts "⚠️ Database connection issue detected, clearing pool: #{e.message}"
  end
  begin
    ActiveRecord::Base.connection_pool.disconnect! if defined?(ActiveRecord)
  rescue ActiveRecord::ConnectionNotEstablished, PG::ConnectionBad => disconnect_error
    puts "⚠️ Failed to disconnect ActiveRecord connections: #{disconnect_error.message}"
  end
end

# Configure RSpec for domain API testing
RSpec.configure do |config|
  # Use expect syntax only
  config.expect_with :rspec do |expectations|
    expectations.syntax = :expect
    expectations.include_chain_clauses_in_custom_matcher_descriptions = true
  end

  # Mock configuration
  config.mock_with :rspec do |mocks|
    mocks.verify_partial_doubles = true
  end

  # Test environment setup - pgmq architecture doesn't need FFI orchestration manager
  config.before(:suite) do
    set_environment_variables
    result = TaskerCore.setup_test_database(database_url)

    raise TaskerCore::Errors::OrchestrationError, "Test database setup failed: #{result['message']}" unless result['status'] == 'success'

    boot_result = TaskerCore::Boot.boot!(force_reload: true)

    raise TaskerCore::Errors::OrchestrationError, "TaskerCore boot failed: #{boot_result[:error]}" unless boot_result[:success]
  end

  # Per-test cleanup to prevent stale data issues
  config.before do |_example|
    # Clean up database records and queue messages to prevent
    # the orchestration system from processing stale data
    TaskerCore.cleanup_test_data
    TaskerCore.cleanup_test_queues
    TaskerCore::Utils::TemplateLoader.load_templates!
  end

  # Per-test cleanup to prevent resource leakage
  config.after do |_example|
    cleanup_workers
    TaskerCore.cleanup_test_data
    TaskerCore.cleanup_test_queues
  end

  config.after(:suite) do
    safely_shutdown_db_connections
    # Clean shutdown of orchestration system
    TaskerCore::Internal::OrchestrationManager.instance.reset!
  end

  # Configure test output
  config.order = :random
  config.shared_context_metadata_behavior = :apply_to_host_groups
  config.filter_run_when_matching :focus
  config.example_status_persistence_file_path = 'spec/examples.txt'
  config.disable_monkey_patching!
  config.warnings = true
end

# Load TaskerCore components - FAIL FAST if cannot load
begin
  # Load core TaskerCore module
  require_relative '../lib/tasker_core'

  puts '✅ TaskerCore loaded successfully'
rescue LoadError => e
  puts "❌ CRITICAL: Could not load TaskerCore: #{e.message}"
  puts '   TaskerCore components are required for integration tests to run.'
  puts '   Check that the Ruby FFI extension is compiled and the paths are correct.'
  raise e # Fail fast - don't continue with broken state
end
