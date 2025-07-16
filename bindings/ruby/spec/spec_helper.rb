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
  puts '🛡️  Environment safety: Set RAILS_ENV=test, APP_ENV=test, RACK_ENV=test'
else
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

  # Database migration hooks for test isolation
  config.before(:suite) do
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

    # Run migrations once before all tests
    puts '🗃️  Setting up test database with migrations...'
    begin
      # Use the same migration functions we expose to test helpers
      result = TaskerCore::TestHelpers.run_migrations(database_url)

      if result['status'] == 'error'
        puts "❌ Migration failed: #{result['error']}"
        exit 1
      else
        puts '✅ Migrations completed successfully'
        
        # Initialize orchestration system from current runtime context (for tests)
        puts '⚙️  Initializing orchestration system for tests...'
        begin
          init_result = TaskerCore.initialize_orchestration_system_from_current_runtime
          if init_result['status'] == 'initialized'
            puts '✅ Orchestration system initialized successfully'
          else
            puts "❌ Orchestration system initialization failed: #{init_result['error']}"
            exit 1
          end
        rescue StandardError => e
          puts "❌ Failed to initialize orchestration system: #{e.message}"
          exit 1
        end
      end
    rescue StandardError => e
      puts "❌ Failed to setup test database: #{e.message}"
      exit 1
    end
  end

  config.after(:suite) do
    # Optional: Clean up after all tests
    puts '🧹 Test suite completed'
  end

  config.around do |example|
    # For tests that need isolated database state
    if example.metadata[:database]
      # Environment safety check before destructive operations per test
      environment_variables = %w[RAILS_ENV APP_ENV RACK_ENV]
      current_env = environment_variables.find { |var| ENV.fetch(var, nil) }&.then { |var| ENV.fetch(var, nil) }

      unless current_env&.downcase == 'test'
        raise "Environment safety check failed! Current environment: #{current_env || 'unset'}. Expected: 'test'"
      end

      # Reset database for this test
      database_url = ENV['DATABASE_URL'] || 'postgresql://tasker:tasker@localhost/tasker_rust_test'
      TaskerCore::TestHelpers.drop_schema(database_url)
      TaskerCore::TestHelpers.run_migrations(database_url)
    end

    example.run
  end
end
