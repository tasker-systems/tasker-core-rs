# frozen_string_literal: true

# SimpleCov must be loaded before any other requires
# Activated by COVERAGE=true environment variable
if ENV['COVERAGE']
  require 'simplecov'
  require 'simplecov_json_formatter'

  SimpleCov.start do
    formatter SimpleCov::Formatter::MultiFormatter.new([
      SimpleCov::Formatter::HTMLFormatter,
      SimpleCov::Formatter::JSONFormatter
    ])

    coverage_dir 'coverage'
    track_files 'lib/**/*.rb'

    add_filter '/spec/'
    add_filter '/vendor/'
    add_filter '/ext/'

    add_group 'Core', 'lib/tasker'
    add_group 'Handlers', 'lib/handlers'

    # Skip threshold when COVERAGE_COLLECT_ONLY is set (data collection mode);
    # threshold enforcement happens via cargo make coverage-check
    unless ENV['COVERAGE_COLLECT_ONLY']
      minimum_coverage 70
      refuse_coverage_drop
    end

    enable_coverage :branch
  end
end

require 'rspec'
require 'json'
require 'yaml'
require 'time'
require 'securerandom'
require 'dotenv'

def set_environment_variables
  # Load test-specific environment file first
  # This must happen before any other initialization
  # .env sets TASKER_CONFIG_PATH to worker-test-embedded.toml (web API disabled)
  env_file = File.expand_path('../.env', __dir__)
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
