# frozen_string_literal: true

require 'rspec'
require 'json'
require 'yaml'
require 'time'
require 'securerandom'
require 'dotenv'

def set_environment_variables
  # Load test-specific environment file
  Dotenv.load('.env.test')
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
  # Load core TaskerCore module
  require_relative '../lib/tasker_core'

  puts '✅ TaskerCore loaded successfully'
rescue LoadError => e
  puts "❌ CRITICAL: Could not load TaskerCore: #{e.message}"
  puts '   TaskerCore components are required for integration tests to run.'
  puts '   Check that the Ruby FFI extension is compiled and the paths are correct.'
  raise e # Fail fast - don't continue with broken state
end
