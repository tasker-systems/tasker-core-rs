# frozen_string_literal: true

require 'bundler/setup'

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
    # Run migrations once before all tests
    puts "🗃️  Setting up test database with migrations..."
    begin
      # Use the same migration functions we expose to test helpers
      database_url = ENV['DATABASE_URL'] || 'postgresql://tasker:tasker@localhost/tasker_rust_test'
      result = TaskerCore::TestHelpers.run_migrations(database_url)
      
      if result['status'] == 'error'
        puts "❌ Migration failed: #{result['error']}"
        exit 1
      else
        puts "✅ Migrations completed successfully"
      end
    rescue => e
      puts "❌ Failed to setup test database: #{e.message}"
      exit 1
    end
  end

  config.after(:suite) do
    # Optional: Clean up after all tests
    puts "🧹 Test suite completed"
  end

  config.around(:each) do |example|
    # For tests that need isolated database state
    if example.metadata[:database]
      # Reset database for this test
      database_url = ENV['DATABASE_URL'] || 'postgresql://tasker:tasker@localhost/tasker_rust_test'
      TaskerCore::TestHelpers.drop_schema(database_url) 
      TaskerCore::TestHelpers.run_migrations(database_url)
    end
    
    example.run
  end
end
