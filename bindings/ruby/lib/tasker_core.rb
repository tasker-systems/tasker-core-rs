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
require_relative 'tasker_core/config'                    # Configuration management system

# 🎯 NEW: Internal infrastructure (hidden from public API)
require_relative 'tasker_core/internal/orchestration_manager'     # Singleton orchestration manager

# 🎯 Clean Domain APIs - Ruby-idiomatic interfaces with handle-based optimization
#
# Domain Pattern: Each domain provides clean public API while internally using
# the shared OrchestrationManager singleton for optimal FFI performance
#
# Primary Domains:
#   - Factory: Test data creation (TaskerCore::Factory.task, .workflow_step, .foundation)
#   - Registry: Handler management (TaskerCore::Registry.register, .find, .include?)
#   - Performance: System monitoring (TaskerCore::Performance.system_health, .analytics)
#   - Events: Event publishing (TaskerCore::Events.publish, .publish_orchestration)
#   - Testing: Test utilities (TaskerCore::Testing.create_task, .setup_environment)
#   - Handlers: Step/task handlers (TaskerCore::Handlers::Steps, TaskerCore::Handlers::Tasks)
#   - Environment: Environment setup (TaskerCore::Environment.setup_test, .cleanup_test)
#   - Orchestration: Workflow execution (TaskerCore::Orchestration.execute_workflow)
#   - Execution: Ruby-Rust command integration (TaskerCore::Execution.start_worker)
#

require_relative 'tasker_core/monkeypatch'
require_relative 'tasker_core/types'             # TaskerCore::Types - dry-struct types for validation
require_relative 'tasker_core/handlers'          # TaskerCore::Handlers domain
require_relative 'tasker_core/environment'       # TaskerCore::Environment domain
require_relative 'tasker_core/orchestration'     # TaskerCore::Orchestration domain

# 🎯 NEW: pgmq-based messaging and database access (replaces FFI performance and embedded server)
require_relative 'tasker_core/messaging'         # TaskerCore::Messaging - pgmq client and queue workers
require_relative 'tasker_core/database'          # TaskerCore::Database - SQL function access

# Core systems - required for domain APIs to function
require_relative 'tasker_core/step_handler/base' # StepHandler::Base (used by Handlers domain)
require_relative 'tasker_core/step_handler/api'  # StepHandler::API (used by Handlers domain)
require_relative 'tasker_core/task_handler/base' # TaskHandler::Base (used by Handlers domain)
require_relative 'tasker_core/errors' # Errors for TaskerCore
require_relative 'tasker_core/embedded_orchestrator' # Embedded orchestration for testing


module TaskerCore
  # Main access point for system health and status
  class << self
    # Create a new orchestration handle for task processing
    # @return [TaskerCore::OrchestrationHandle] Handle for orchestration operations
    def create_orchestration_handle
      Internal::OrchestrationManager.instance.orchestration_handle
    end

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
          registry: Registry.handle_info,
          performance: Performance.handle_info,
          events: Events.health,
          testing: Testing.validate_environment,
          orchestration: Orchestration.health,
          environment: Environment.handle_info,
          execution: Execution.version_info
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
        orchestration: Orchestration.handle_info,
        environment: Environment.handle_info,
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
      %w[Factory Registry Performance Events Testing Handlers Environment Orchestration Execution].map do |domain|
        domain_class = const_get(domain)
        [
          domain.downcase.to_sym,
          {
            available: domain_class.respond_to?(:handle_info),
            methods: domain_class.respond_to?(:handle_info) ?
              (domain_class.handle_info['available_methods'] || domain_class.handle_info[:available_methods] || []) : []
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

  # Direct access to internal components - maintained for transition period
  # New development should use domain APIs instead (e.g., TaskerCore::Orchestration, TaskerCore::Testing)
  module Internal
    autoload :OrchestrationManager, 'tasker_core/internal/orchestration_manager'
  end

  # Legacy direct aliases - deprecated, use TaskerCore::Internal instead
  # These use autoload to match the Internal module pattern
  autoload :OrchestrationManager, 'tasker_core/internal/orchestration_manager'
end
