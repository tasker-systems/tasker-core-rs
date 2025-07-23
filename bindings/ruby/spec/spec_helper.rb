# frozen_string_literal: true

require 'rspec'
require 'json'
require 'yaml'
require 'time'
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

  config.define_derived_metadata(file_path: %r{/spec/handlers/integration/}) do |metadata|
    metadata[:type] = :handler_integration
  end
end

# Add support for Rails-like deep_merge and deep_symbolize_keys
class Hash
  def deep_merge(other_hash)
    dup.deep_merge!(other_hash)
  end

  def deep_merge!(other_hash)
    other_hash.each_pair do |k, v|
      tv = self[k]
      if tv.is_a?(Hash) && v.is_a?(Hash)
        self[k] = tv.deep_merge(v)
      else
        self[k] = v
      end
    end
    self
  end

  def deep_symbolize_keys
    transform_keys(&:to_sym).transform_values do |value|
      case value
      when Hash
        value.deep_symbolize_keys
      when Array
        value.map { |v| v.is_a?(Hash) ? v.deep_symbolize_keys : v }
      else
        value
      end
    end
  end
end

# Mock Rails logger for testing
module Rails
  class << self
    def logger
      @logger ||= Logger.new($stdout).tap do |log|
        log.level = Logger::INFO
      end
    end
  end
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

  # Load TaskerCore components that our tests need
  require_relative '../lib/tasker_core/types'
  require_relative '../lib/tasker_core/errors'
  require_relative '../lib/tasker_core/registry'
  require_relative '../lib/tasker_core/task_handler/base'
  require_relative '../lib/tasker_core/step_handler/base'

  puts "✅ TaskerCore loaded successfully"
rescue LoadError => e
  puts "❌ CRITICAL: Could not load TaskerCore: #{e.message}"
  puts "   TaskerCore components are required for integration tests to run."
  puts "   Check that the Ruby FFI extension is compiled and the paths are correct."
  raise e  # Fail fast - don't continue with broken state
end
