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
# 🎯 NEW: Utility infrastructure
require_relative 'tasker_core/utils/path_resolver' # Centralized path resolution
require_relative 'tasker_core/utils/template_loader' # Centralized template loading
require_relative 'tasker_core/config/validator' # Configuration validation

# 🎯 NEW: Internal infrastructure (hidden from public API)
require_relative 'tasker_core/internal/orchestration_manager' # Singleton orchestration manager

require_relative 'tasker_core/types'             # TaskerCore::Types - dry-struct types for validation
require_relative 'tasker_core/handlers'          # TaskerCore::Handlers domain
require_relative 'tasker_core/environment'       # TaskerCore::Environment domain
require_relative 'tasker_core/orchestration'     # TaskerCore::Orchestration domain
require_relative 'tasker_core/execution'         # TaskerCore::Execution - step sequence and processing

# 🎯 NEW: pgmq-based messaging and database access (replaces FFI performance and embedded server)
require_relative 'tasker_core/messaging'         # TaskerCore::Messaging - pgmq client and queue workers
require_relative 'tasker_core/database'          # TaskerCore::Database - SQL function access
require_relative 'tasker_core/state_machine'     # TaskerCore::StateMachine - state machine for task execution
require_relative 'tasker_core/registry'          # TaskerCore::Registry - database-backed step handler registry

# Boot sequence manager (loads after all dependencies)
require_relative 'tasker_core/boot' # Boot sequence manager

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

    # Get the project root directory
    # @return [String] Absolute path to project root
    def project_root
      Utils::PathResolver.project_root
    end

    # Get the gem root directory
    # @return [String] Absolute path to gem root
    def gem_root
      Utils::PathResolver.gem_root
    end

    # Get the gem library root directory
    # @return [String] Absolute path to gem library root
    def gem_lib_root
      Utils::PathResolver.gem_lib_root
    end

    # Validate configuration and raise if invalid
    # @raise [ConfigValidation::Validator::ValidationError] If configuration is invalid
    def validate_config!
      validator = ConfigValidation::Validator.new
      validator.validate!
    end

    # Run configuration diagnostics
    # @return [Hash] Diagnostic results
    def diagnose
      require_relative 'tasker_core/cli/diagnostics'
      CLI::Diagnostics.run_config_check
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
      puts 'Shutting down TaskerCore...'

      # Shutdown internal managers
      begin
        Internal::OrchestrationManager.instance.shutdown
      rescue StandardError
        nil
      end
      begin
        Internal::TestingManager.shutdown
      rescue StandardError
        nil
      end
      begin
        Internal::TestingFactoryManager.shutdown
      rescue StandardError
        nil
      end

      puts 'TaskerCore shutdown complete'
    end

    private

    def rust_extension_status
      {
        loaded: defined?(TaskerCore::BaseStepHandler) && defined?(TaskerCore::BaseTaskHandler),
        version: defined?(TaskerCore::RUST_VERSION) ? TaskerCore::RUST_VERSION : 'unknown',
        features: defined?(TaskerCore::FEATURES) ? TaskerCore::FEATURES : 'unknown'
      }
    end

    def domain_status
      %w[Factory Registry Performance Events Testing Handlers Environment Orchestration Execution].to_h do |domain|
        domain_class = const_get(domain)
        [
          domain.downcase.to_sym,
          {
            available: domain_class.respond_to?(:handle_info),
            methods: if domain_class.respond_to?(:handle_info)
                       domain_class.handle_info['available_methods'] || domain_class.handle_info[:available_methods] || []
                     else
                       []
                     end
          }
        ]
      end
    end

    def internal_status
      {
        orchestration_manager: defined?(Internal::OrchestrationManager),
        testing_manager: defined?(Internal::TestingManager),
        testing_factory_manager: defined?(Internal::TestingFactoryManager)
      }
    end

    def overall_health_status
      handle_status = self.handle_status
      all_healthy = handle_status.values.all? { |status| status[:status] != 'unavailable' }
      all_healthy ? 'healthy' : 'degraded'
    rescue StandardError
      'unhealthy'
    end

    def internal_health_status
      {
        orchestration_manager: begin
          Internal::OrchestrationManager.instance.handle_info[:status]
        rescue StandardError
          'unavailable'
        end,
        testing_manager: begin
          Internal::TestingManager.status
        rescue StandardError
          'unavailable'
        end,
        testing_factory_manager: begin
          Internal::TestingFactoryManager.status
        rescue StandardError
          'unavailable'
        end
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

# class Hash
#   def deep_merge(other_hash)
#     dup.deep_merge!(other_hash)
#   end

#   def deep_merge!(other_hash)
#     other_hash.each_pair do |k, v|
#       tv = self[k]
#       self[k] = if tv.is_a?(Hash) && v.is_a?(Hash)
#                   tv.deep_merge(v)
#                 else
#                   v
#                 end
#     end
#     self
#   end

#   def deep_symbolize_keys
#     transform_keys(&:to_sym).transform_values do |value|
#       case value
#       when Hash
#         value.deep_symbolize_keys
#       when Array
#         value.map { |v| v.is_a?(Hash) ? v.deep_symbolize_keys : v }
#       else
#         value
#       end
#     end
#   end
# end

# Automatically boot the system when TaskerCore is loaded
# This ensures proper initialization order for all components
# Skip auto-boot in test mode (controlled by spec_helper) or when explicitly disabled
TaskerCore::Boot.boot! unless defined?(Rails) || ENV['TASKER_SKIP_AUTO_BOOT'] || ENV['TASKER_ENV'] == 'test'
