# frozen_string_literal: true

require 'rspec'
require 'json'
require 'yaml'
require 'time'
require 'securerandom'
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
    # pgmq architecture is database-driven, no FFI setup needed
    puts "🔧 Setting up pgmq architecture tests..."
  end

  # Configure test output
  config.order = :random
  config.shared_context_metadata_behavior = :apply_to_host_groups
  config.filter_run_when_matching :focus
  config.example_status_persistence_file_path = "spec/examples.txt"
  config.disable_monkey_patching!
  config.warnings = true
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

  puts "✅ TaskerCore loaded successfully"
rescue LoadError => e
  puts "❌ CRITICAL: Could not load TaskerCore: #{e.message}"
  puts "   TaskerCore components are required for integration tests to run."
  puts "   Check that the Ruby FFI extension is compiled and the paths are correct."
  raise e  # Fail fast - don't continue with broken state
end
