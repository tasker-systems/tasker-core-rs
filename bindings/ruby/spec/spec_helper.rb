# frozen_string_literal: true

require 'rspec'
require 'json'
require 'yaml'
require 'time'
require 'securerandom'
require 'dotenv'
# require_relative 'domain_helpers'  # Not needed for pgmq architecture tests

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
    Dotenv.load

    # CRITICAL: Set environment to 'test' for proper TaskTemplate discovery
    # The DistributedHandlerRegistry uses current_environment to determine search patterns
    # Without this, it defaults to 'development' and looks in wrong directories
    ENV['RUBY_ENV'] = 'test'

    # Setup test database FIRST using new comprehensive management system
    database_url = ENV['DATABASE_URL'] || 'postgresql://tasker:tasker@localhost/tasker_rust_test'

    begin
      puts "🧪 Setting up test database: #{database_url}"
      result = TaskerCore.setup_test_database(database_url)

      if result['status'] == 'success'
        puts '✅ Test database setup completed successfully'
        puts "   - Environment: #{result['environment']}"
        puts "   - Operations completed: #{result['operations'].keys.join(', ')}" if result['operations']
      else
        puts "❌ Test database setup failed: #{result['message']}"
        raise "Test database setup failed: #{result['message']}"
      end
    rescue StandardError => e
      puts "💥 Failed to setup test database: #{e.message}"
      raise "Test database setup failed: #{e.message}"
    end

    # CRITICAL: Set environment variables for proper system behavior
    ENV['RUBY_ENV'] = 'test'
    ENV['RAILS_ENV'] = 'test'
    ENV['TASKER_ENV'] = 'test'
    ENV['TASKER_EMBEDDED_MODE'] = 'true'

    # Use the new boot sequence to properly initialize all components
    begin
      puts '🚀 Booting TaskerCore system with proper initialization order...'
      boot_result = TaskerCore::Boot.boot!(force_reload: true)
      
      if boot_result[:success]
        puts "✅ TaskerCore boot completed in #{boot_result[:boot_time].round(3)}s"
        puts "   - Environment: #{boot_result[:environment]}"
        puts "   - Embedded mode: #{boot_result[:embedded_mode]}"
        puts "   - TaskTemplates loaded: #{boot_result[:task_templates_loaded]}"
        puts "   - Orchestrator started: #{boot_result[:orchestrator_started]}"
      else
        puts "⚠️ TaskerCore boot failed: #{boot_result[:error]}"
        puts '   Continuing with tests - individual tests may fail if they need specific functionality'
      end
    rescue StandardError => e
      puts "⚠️ TaskerCore boot error: #{e.message}"
      puts '   Continuing with tests - individual tests may fail if they need specific functionality'
    end
  end

  # Per-test cleanup to prevent resource leakage
  config.after(:each) do |example|
    # Clean up any queue workers created during the test
    if defined?(@test_workers) && @test_workers
      @test_workers.each do |worker|
        begin
          if worker.running?
            worker.stop
            # Give worker a moment to fully stop
            sleep 0.1
          end
        rescue TaskerCore::Errors::WorkerError, ArgumentError => e
          puts "⚠️ Failed to stop test worker: #{e.message}"
        end
      end
      @test_workers.clear
    end

    # Don't disconnect connections aggressively - this interferes with the embedded orchestrator
    # Only clear connection pool if there are connection errors or warnings
    begin
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
      begin
        ActiveRecord::Base.connection_pool.disconnect! if defined?(ActiveRecord)
      rescue ActiveRecord::ConnectionNotEstablished, PG::ConnectionBad => disconnect_error
        puts "⚠️ Failed to disconnect ActiveRecord connections: #{disconnect_error.message}"
      end
    end
  end

  config.after(:suite) do
    # Teardown test database using comprehensive management system
    database_url = ENV['DATABASE_URL'] || 'postgresql://tasker:tasker@localhost/tasker_rust_test'

    begin
      puts "🧹 Tearing down test database: #{database_url}"
      result = TaskerCore.teardown_test_database(database_url)

      if result['status'] == 'success'
        puts '✅ Test database teardown completed successfully'
        puts "   - Environment: #{result['environment']}"
        puts "   - Operations completed: #{result['operations'].keys.join(', ')}" if result['operations']
      else
        puts "⚠️ Test database teardown had issues: #{result['message']}"
      end
    rescue TaskerCore::Errors::DatabaseError, 
           ActiveRecord::ConnectionNotEstablished => e
      puts "⚠️ Test database teardown failed (non-fatal): #{e.message}"
    end

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

# Mock Time.current for Rails compatibility
class Time
  def self.current
    now
  end
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
