# frozen_string_literal: true

require_relative 'tasker_core/version'
require 'json'
require 'faraday'
require 'dry-events'
require 'dry-struct'
require 'dry-types'
require 'dry-validation'
require 'concurrent-ruby'
require 'timeout'
require 'dotenv'
require 'active_support'
require 'active_support/core_ext'

# Pre-define TaskerCore module for Magnus
module TaskerCore
end

begin
  Dotenv.load
  # Load the compiled Rust extension first (provides base classes)
  require_relative 'tasker_core/tasker_worker_rb'
rescue LoadError => e
  raise LoadError, <<~MSG

    ❌ Failed to load tasker-worker-rb native extension!

    This usually means the Rust extension hasn't been compiled yet.

    To compile the extension:
      cd #{File.dirname(__FILE__)}/../..
      rake compile

    Or if you're using this gem in a Rails application:
      bundle exec rake tasker_core:compile

    Original error: #{e.message}

  MSG
end

# Load Ruby modules after Rust extension (they depend on Rust base classes)
require_relative 'tasker_core/errors'
require_relative 'tasker_core/logger'
require_relative 'tasker_core/internal'
require_relative 'tasker_core/template_discovery'
require_relative 'tasker_core/test_environment'
require_relative 'tasker_core/types'
require_relative 'tasker_core/models'
require_relative 'tasker_core/handlers'
require_relative 'tasker_core/registry'
require_relative 'tasker_core/subscriber'
require_relative 'tasker_core/event_bridge'
require_relative 'tasker_core/worker/event_poller'
require_relative 'tasker_core/bootstrap'

module TaskerCore
  module Worker
  end
end

# Load test environment components if appropriate (before auto-boot)
TaskerCore::TestEnvironment.load_if_test!
