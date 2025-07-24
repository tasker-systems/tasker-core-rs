# frozen_string_literal: true

require_relative 'tasker_core/version'
require 'json'
require 'faraday'
require 'dry-events'
require 'dry-struct'
require 'dry-types'
require 'dry-validation'

# Pre-define TaskerCore module for Magnus
module TaskerCore
end

begin
  # Load the compiled Rust extension first (provides base classes)
  require_relative 'tasker_core/tasker_core_rb'
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
require_relative 'tasker_core/logging/logger'            # Logging system

# 🎯 NEW: Internal infrastructure (hidden from public API)
require_relative 'tasker_core/internal/orchestration_manager'     # Singleton orchestration manager
require_relative 'tasker_core/internal/testing_manager'           # Singleton testing manager
require_relative 'tasker_core/internal/testing_factory_manager'   # Singleton testing factory manager

# 🎯 NEW: Clean Domain APIs with handle-based optimization
require_relative 'tasker_core/types'             # TaskerCore::Types - dry-struct types for validation
require_relative 'tasker_core/factory'           # TaskerCore::Factory domain
require_relative 'tasker_core/registry'          # TaskerCore::Registry domain
require_relative 'tasker_core/performance'       # TaskerCore::Performance domain
require_relative 'tasker_core/events'            # TaskerCore::Events domain (NEW - replaces complex events.rb)
require_relative 'tasker_core/testing'           # TaskerCore::Testing domain (NEW)
require_relative 'tasker_core/handlers'          # TaskerCore::Handlers domain (NEW)
require_relative 'tasker_core/environment'       # TaskerCore::Environment domain
require_relative 'tasker_core/execution'         # TaskerCore::Execution domain (ZeroMQ step execution)

# Legacy compatibility - these will be deprecated in favor of domain APIs
require_relative 'tasker_core/events_domain'     # TaskerCore::Events::Domain (legacy)
require_relative 'tasker_core/step_handler/base' # StepHandler::Base
require_relative 'tasker_core/step_handler/api'  # StepHandler::API
require_relative 'tasker_core/task_handler/base' # TaskHandler::Base
require_relative 'tasker_core/task_handler/results' # TaskHandler result classes and wrappers
require_relative 'tasker_core/test_helpers' # Test helpers for workflow testing
require_relative 'tasker_core/models' # Models for TaskerCore

module TaskerCore
  # Base error hierarchy
  class Error < StandardError; end
  class OrchestrationError < Error; end
  class DatabaseError < Error; end
  class StateTransitionError < Error; end
  class ValidationError < Error; end
  class TimeoutError < Error; end
  class FFIError < Error; end

  # Step handler error classification (mirrors Rails engine design)
  class ProceduralError < Error; end

  # Retryable errors - temporary failures that should be retried with backoff
  class RetryableError < ProceduralError
    attr_reader :retry_after, :context, :error_category

    def initialize(message, retry_after: nil, context: {}, error_category: nil)
      super(message)
      @retry_after = retry_after
      @context = context || {}
      @error_category = error_category
    end

    def skip_backoff?
      retry_after&.positive?
    end

    def effective_retry_delay(attempt_number = 1)
      return retry_after if retry_after&.positive?
      [2**attempt_number, 300].min
    end
  end

  # Permanent errors - failures that should NOT be retried
  class PermanentError < ProceduralError
    attr_reader :error_code, :context, :error_category

    def initialize(message, error_code: nil, context: {}, error_category: nil)
      super(message)
      @error_code = error_code
      @context = context || {}
      @error_category = error_category
    end

    def validation_error?
      error_category == 'validation' || error_code&.start_with?('VALIDATION_')
    end

    def authorization_error?
      error_category == 'authorization' || error_code&.start_with?('AUTH_')
    end

    def business_logic_error?
      error_category == 'business_logic' || error_code&.start_with?('BUSINESS_')
    end
  end

  # Main access point for system health and status
  class << self
    # Check if TaskerCore is ready for use
    # @return [Hash] System status and readiness info
    def status
      {
        rust_extension: rust_extension_status,
        domains: domain_status,
        internal_systems: internal_status,
        version: VERSION,
        checked_at: Time.now.utc.iso8601
      }
    end

    # Get comprehensive system health
    # @return [Hash] Detailed health information across all domains
    def health
      {
        overall: overall_health_status,
        domains: {
          factory: Factory.health,
          registry: Registry.health,
          performance: Performance.health,
          events: Events.health,
          testing: Testing.validate_environment,
          environment: Environment.status
        },
        internal: internal_health_status,
        timestamp: Time.now.utc.iso8601
      }
    end

    # Get handle information across all domains
    # @return [Hash] Handle status for all domain APIs
    def handle_status
      {
        factory: Factory.handle_info,
        registry: Registry.handle_info,
        performance: Performance.handle_info,
        events: Events.handle_info,
        testing: Testing.handle_info,
        checked_at: Time.now.utc.iso8601
      }
    end

    # Shutdown all systems gracefully
    def shutdown
      puts "Shutting down TaskerCore..."

      # Shutdown internal managers
      Internal::OrchestrationManager.instance.shutdown rescue nil
      Internal::TestingManager.shutdown rescue nil
      Internal::TestingFactoryManager.shutdown rescue nil

      puts "TaskerCore shutdown complete"
    end

    private

    def rust_extension_status
      {
        loaded: defined?(TaskerCore::BaseStepHandler) && defined?(TaskerCore::BaseTaskHandler),
        version: defined?(TaskerCore::RUST_VERSION) ? TaskerCore::RUST_VERSION : "unknown",
        features: defined?(TaskerCore::FEATURES) ? TaskerCore::FEATURES : "unknown"
      }
    end

    def domain_status
      %w[Factory Registry Performance Events Testing Handlers Environment].map do |domain|
        domain_class = const_get(domain)
        [
          domain.downcase.to_sym,
          {
            available: domain_class.respond_to?(:handle_info),
            methods: domain_class.respond_to?(:handle_info) ?
              domain_class.handle_info[:available_methods] : []
          }
        ]
      end.to_h
    end

    def internal_status
      {
        orchestration_manager: defined?(Internal::OrchestrationManager),
        testing_manager: defined?(Internal::TestingManager),
        testing_factory_manager: defined?(Internal::TestingFactoryManager)
      }
    end

    def overall_health_status
      begin
        handle_status = self.handle_status
        all_healthy = handle_status.values.all? { |status| status[:status] != "unavailable" }
        all_healthy ? "healthy" : "degraded"
      rescue
        "unhealthy"
      end
    end

    def internal_health_status
      {
        orchestration_manager: (Internal::OrchestrationManager.instance.handle_info[:status] rescue "unavailable"),
        testing_manager: (Internal::TestingManager.status rescue "unavailable"),
        testing_factory_manager: (Internal::TestingFactoryManager.status rescue "unavailable")
      }
    end
  end

  # Legacy compatibility aliases (to be deprecated)
  # These provide access to internal managers for backward compatibility
  autoload :OrchestrationManager, 'tasker_core/internal/orchestration_manager'
  autoload :TestingManager, 'tasker_core/internal/testing_manager'
  autoload :TestingFactoryManager, 'tasker_core/internal/testing_factory_manager'
end
