# frozen_string_literal: true

require 'rspec'
require 'json'
require 'yaml'
require 'time'
require 'securerandom'
require 'dotenv'

def set_environment_variables
  # Load test-specific environment file first
  # This must happen before any other initialization
  # .env.test sets TASKER_CONFIG_PATH to ruby-rspec.toml (web API disabled)
  env_file = File.expand_path('../.env.test', __dir__)
  Dotenv.load(env_file) if File.exist?(env_file)

  ENV['TASKER_ENV'] = 'test'
  ENV['TASKER_DISABLE_AUTO_BOOT'] = 'true'
end

# Set environment variables at load time
set_environment_variables

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
end

# Load TaskerCore components - FAIL FAST if cannot load
begin
  # Load core TaskerCore module (test environment loading happens automatically)
  require_relative '../lib/tasker_core'

  puts '✅ TaskerCore loaded successfully'

  # Verify test environment was loaded properly
  if TaskerCore::TestEnvironment.loaded?
    test_info = TaskerCore::TestEnvironment.info
    puts "🧪 Test environment loaded: #{test_info[:handler_count]} example handlers available"
    puts "📁 Template path: #{test_info[:template_path] || 'Not set'}"
    puts "📄 Template files: #{test_info[:template_files] || 0}"

    # Show loaded handlers for debugging
    handler_names = TaskerCore::TestEnvironment.handler_names
    unless handler_names.empty?
      puts '🎯 Example handlers loaded:'
      handler_names.first(5).each { |name| puts "   - #{name}" }
      puts "   ... and #{handler_names.size - 5} more" if handler_names.size > 5
    end
  else
    puts "⚠️  Test environment was not loaded (this is expected if TASKER_ENV != 'test')"
  end
rescue LoadError => e
  puts "❌ CRITICAL: Could not load TaskerCore: #{e.message}"
  puts '   TaskerCore components are required for integration tests to run.'
  puts '   Check that the Ruby FFI extension is compiled and the paths are correct.'
  raise e # Fail fast - don't continue with broken state
rescue StandardError => e
  puts "❌ CRITICAL: Unexpected error during TaskerCore setup: #{e.class} - #{e.message}"
  puts e.backtrace.first(10).join("\n   ")
  raise e
end

# TAS-65: Load domain events test helpers
require_relative 'support/domain_events'
puts '🧪 Domain event test helpers loaded'
