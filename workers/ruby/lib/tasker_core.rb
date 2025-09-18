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
require 'statesman'

# Pre-define TaskerCore module for Magnus
module TaskerCore
end

begin
  Dotenv.load
  # Load the compiled Rust extension first (provides base classes)
  require_relative 'tasker_core/tasker_worker_rb'
rescue LoadError => e
  raise LoadError, <<~MSG

    ❌ Failed to load tasker-core-rb native extension!

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
require_relative 'tasker_core/types'
require_relative 'tasker_core/models'
require_relative 'tasker_core/handlers'
require_relative 'tasker_core/registry'
require_relative 'tasker_core/subscriber'
require_relative 'tasker_core/event_bridge'
require_relative 'tasker_core/bootstrap'

module TaskerCore
  module Worker
  end
end
# Automatically boot the system when TaskerCore is loaded
# This ensures proper initialization order for all components
# Skip auto-boot in test mode (controlled by spec_helper) or when explicitly disabled
TaskerCore::Worker::Bootstrap.start! unless ENV['TASKER_ENV'] == 'test' || ENV['TASKER_DISABLE_AUTO_BOOT'] == 'true'
