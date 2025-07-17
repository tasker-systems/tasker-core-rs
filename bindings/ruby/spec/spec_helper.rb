# frozen_string_literal: true

require 'bundler/setup'

# =============================================================================
# ENVIRONMENT SAFETY CHECKS
# =============================================================================
# Prevent running destructive database operations against non-test environments

# Check common Ruby framework environment variables
environment_variables = %w[RAILS_ENV APP_ENV RACK_ENV]
current_env = nil

environment_variables.each do |env_var|
  if ENV[env_var]
    current_env = ENV[env_var]
    break
  end
end

# If any environment variable is set and is NOT "test", refuse to run
if current_env && current_env.downcase != 'test'
  puts '❌ FATAL ERROR: Refusing to run destructive test operations!'
  puts "   Environment variable is set to: #{current_env}"
  puts "   Expected: 'test' or unset"
  puts '   This protects against accidentally running tests against production/development databases.'
  puts ''
  puts '   To fix: Set RAILS_ENV=test (or APP_ENV=test, RACK_ENV=test) or unset all environment variables'
  exit 1
end

# If no environment variables are set, explicitly set them to "test" for safety
if current_env.nil?
  ENV['RAILS_ENV'] = 'test'
  ENV['APP_ENV'] = 'test'
  ENV['RACK_ENV'] = 'test'
  ENV['TASKER_ENV'] = 'test'  # Set TASKER_ENV for configuration loading
  puts '🛡️  Environment safety: Set RAILS_ENV=test, APP_ENV=test, RACK_ENV=test, TASKER_ENV=test'
else
  ENV['TASKER_ENV'] = 'test'  # Ensure TASKER_ENV is set for configuration loading
  puts "🛡️  Environment safety: Confirmed running in test environment (#{current_env})"
end

# Load test environment configuration
require 'dotenv'
Dotenv.load('.env.test')

require 'dry-events'
require 'logger'
require 'time'
require 'json'

# Configure load path
$LOAD_PATH.unshift File.expand_path('../lib', __dir__)

# Load the main TaskerCore library (includes native extension)
require 'tasker_core'
require 'test_helpers'

RSpec.configure do |config|
  config.expect_with :rspec do |expectations|
    expectations.include_chain_clauses_in_custom_matcher_descriptions = true
  end

  config.mock_with :rspec do |mocks|
    mocks.verify_partial_doubles = true
  end

  config.shared_context_metadata_behavior = :apply_to_host_groups
  config.filter_run_when_matching :focus
  config.example_status_persistence_file_path = 'spec/examples.txt'
  config.disable_monkey_patching!
  config.warnings = true

  config.default_formatter = 'doc' if config.files_to_run.one?

  config.profile_examples = 10
  config.order = :random
  Kernel.srand config.seed

  def ensure_safe_environment!
    # Double-check environment safety before destructive operations
    environment_variables = %w[RAILS_ENV APP_ENV RACK_ENV]
    current_env = environment_variables.find { |var| ENV.fetch(var, nil) }&.then { |var| ENV.fetch(var, nil) }

    unless current_env&.downcase == 'test'
      puts '❌ FATAL ERROR: Environment safety check failed in test suite!'
      puts "   Current environment: #{current_env || 'unset'}"
      puts '   Refusing to run destructive database operations.'
      exit 1
    end

    # Ensure DATABASE_URL points to test database
    database_url = ENV['DATABASE_URL'] || 'postgresql://tasker:tasker@localhost/tasker_rust_test'
    unless database_url.include?('test') || database_url.include?('_test')
      if ENV['FORCE_ACCEPT_DB_URL']
        puts '⚠️  OVERRIDE: Accepting non-test-like DATABASE_URL due to FORCE_ACCEPT_DB_URL=true'
        puts "   DATABASE_URL: #{database_url}"
      else
        puts '❌ FATAL ERROR: DATABASE_URL does not appear to be a test database!'
        puts "   DATABASE_URL: #{database_url}"
        puts "   Expected: URL containing 'test' or '_test'"
        puts '   This protects against accidentally running tests against production/development databases.'
        puts ''
        puts "   To override: Set FORCE_ACCEPT_DB_URL=true if you're certain this is a test database"
        puts '   Example: FORCE_ACCEPT_DB_URL=true bundle exec rspec'
        exit 1
      end
    end
  end

  def initialize_orchestration_system
    puts '🧪 Setting up test environment...'
    puts '💡 Note: Database migrations should be run separately with: bundle exec rake test:setup'

    begin
      # 🎯 UNIFIED ENTRY POINT: Initialize unified orchestration system via OrchestrationManager
      puts '⚙️  Initializing unified orchestration system for tests...'
      begin
        orchestration_manager = TaskerCore::OrchestrationManager.instance
        init_result = orchestration_manager.initialize_orchestration_system!
        
        if init_result['status'] == 'initialized'
          puts '✅ Unified orchestration system initialized successfully via OrchestrationManager'
          puts "   Architecture: #{init_result['architecture']}"
          puts "   Pool source: #{init_result['pool_source']}"
          puts "   Manager status: #{orchestration_manager.status}"
        else
          puts "❌ Unified orchestration system initialization failed: #{init_result['error']}"
          exit 1
        end
      rescue StandardError => e
        puts "❌ Failed to initialize unified orchestration system: #{e.message}"
        exit 1
      end

      # Use TestingManager singleton for unified test environment setup
      puts '🧪 Running lightweight test environment setup via TestingManager...'
      testing_manager = TaskerCore::TestingManager.instance
      result = testing_manager.setup_test_environment

      if result['status'] == 'error'
        puts "❌ TestingManager setup failed: #{result['error']}"
        puts "💡 Try running: bundle exec rake test:setup"
        exit 1
      else
        puts '✅ TestingManager setup completed successfully'
        puts "   Steps: #{result['steps_completed']&.join(', ')}"
        puts "   Pool connections: #{result['pool_connections']}"
        puts "   Manager status: #{testing_manager.status}"
      end
    rescue StandardError => e
      puts "❌ Failed to setup test environment: #{e.message}"
      puts "💡 Try running: bundle exec rake test:setup"
      exit 1
    end
  end

  # Test environment setup (migrations handled by rake task)
  config.before(:suite) do
    ensure_safe_environment!
    initialize_orchestration_system
  end

  config.after(:suite) do
    # Clean up after all tests using TestingFramework
    ensure_safe_environment!

    puts '🧹 Cleaning up test environment with TestingManager...'
    testing_manager = TaskerCore::TestingManager.instance
    result = testing_manager.cleanup_test_environment

    if result['status'] == 'error'
      puts "⚠️  TestingManager cleanup failed: #{result['error']}"
    else
      puts '✅ TestingManager cleanup completed successfully'
    end

    puts '🧹 Test suite completed'
  end
end
