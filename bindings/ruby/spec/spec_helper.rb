# frozen_string_literal: true

require 'rspec'
require 'json'
require 'securerandom'
require_relative 'domain_helpers'

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

  # Test environment setup using new domain APIs
  config.before(:suite) do
    puts "🚀 Setting up test environment using TaskerCore::Environment"

    # Setup test environment through our new domain API
    result = TaskerCore::Environment.setup_test

    if result&.dig('status') == 'error'
      puts "⚠️  Test environment setup warning: #{result['error']}"
      puts "   Tests will continue but database operations may timeout"
    else
      puts "✅ Test environment setup successful"
    end
  end

  config.after(:suite) do
    puts "🧹 Cleaning up test environment using TaskerCore::Environment"

    # Cleanup test environment through our new domain API
    result = TaskerCore::Environment.cleanup_test

    if result&.dig('status') == 'error'
      puts "⚠️  Test environment cleanup warning: #{result['error']}"
    else
      puts "✅ Test environment cleanup successful"
    end
  end

  # Include domain test helpers instead of legacy TestHelpers
  config.include DomainTestHelpers
  config.include WorkflowValidationHelpers
  config.include HandleArchitectureHelpers

  # Configure test output
  config.order = :random
  config.shared_context_metadata_behavior = :apply_to_host_groups
  config.filter_run_when_matching :focus
  config.example_status_persistence_file_path = "spec/examples.txt"
  config.disable_monkey_patching!
  config.warnings = true

  # Default to running tests that require database unless explicitly excluded
  config.filter_run_excluding :skip_database unless ENV['INCLUDE_DATABASE_TESTS'] == 'true'

  # Add custom metadata for test categorization
  config.define_derived_metadata(file_path: %r{/spec/domain/}) do |metadata|
    metadata[:type] = :domain_api
  end

  config.define_derived_metadata(file_path: %r{/spec/architecture/}) do |metadata|
    metadata[:type] = :handle_architecture
  end

  config.define_derived_metadata(file_path: %r{/spec/integration/}) do |metadata|
    metadata[:type] = :integration
  end

  config.define_derived_metadata(file_path: %r{/spec/legacy/}) do |metadata|
    metadata[:type] = :legacy_regression
  end
end
