# frozen_string_literal: true

require 'singleton'
require 'pathname'
require 'tasker_core/errors'
require 'tasker_core/types/config'

module TaskerCore
  # Simplified configuration manager that delegates to Rust's UnifiedConfig::Manager
  #
  # This class provides a thin Ruby wrapper around the Rust configuration system with
  # dry-types validation. All validation and loading is handled by the Rust side,
  # then converted to type-safe Ruby structures.
  #
  # Follows fail-fast principle: any configuration error results in immediate failure.
  class Config
    include Singleton

    attr_reader :environment, :config_manager, :config, :raw_config

    def initialize
      @environment = detect_environment
      load_unified_config!
    end

    # Get database configuration as typed structure
    def database_config
      @config.database
    end

    # Get PGMQ configuration as typed structure
    def pgmq_config
      @config.pgmq
    end

    # Get executor pools configuration as typed structure
    def executor_pools_config
      @config.executor_pools_config
    end

    # Get orchestration configuration as typed structure
    def orchestration_config
      @config.orchestration
    end

    # Get telemetry configuration as typed structure
    def telemetry_config
      @config.telemetry
    end

    # Get circuit breakers configuration as typed structure
    def circuit_breakers_config
      @config.circuit_breakers
    end

    # Get system configuration as typed structure
    def system_config
      @config.system
    end

    # Get engine configuration as typed structure
    def engine_config
      @config.engine
    end

    # Get auth configuration as typed structure
    def auth_config
      @config.auth
    end

    # Get health configuration as typed structure
    def health_config
      @config.health
    end

    # Get dependency graph configuration as typed structure
    def dependency_graph_config
      @config.dependency_graph
    end

    # Get backoff configuration as typed structure
    def backoff_config
      @config.backoff
    end

    # Get execution configuration as typed structure
    def execution_config
      @config.execution
    end

    # Get reenqueue configuration as typed structure
    def reenqueue_config
      @config.reenqueue
    end

    # Get events configuration as typed structure
    def events_config
      @config.events
    end

    # Get cache configuration as typed structure
    def cache_config
      @config.cache
    end

    # Get query cache configuration as typed structure
    def query_cache_config
      @config.query_cache
    end

    # Get task templates configuration as typed structure
    def task_templates_config
      @config.task_templates
    end

    # Backward compatibility alias
    alias task_template_config task_templates_config

    # Get task template search paths for backward compatibility
    def task_template_search_paths
      search_paths = @config.task_templates.search_paths

      # Convert relative paths to absolute paths
      search_paths.map do |path|
        if path.start_with?('/')
          path
        else
          # Resolve relative to config root
          File.join(Utils::PathResolver.project_root, path)
        end
      end
    end

    # Type-safe configuration access methods using dry-types

    # Get database URL for current environment
    def database_url
      @config.database_url
    end

    # Environment checks using typed config
    def test_environment?
      @config.test_environment?
    end

    def development_environment?
      @config.development_environment?
    end

    def production_environment?
      @config.production_environment?
    end

    # System configuration accessors
    def max_dependency_depth
      @config.max_dependency_depth
    end

    def max_workflow_steps
      @config.max_workflow_steps
    end

    def system_version
      @config.system_version
    end

    def connection_timeout
      @config.connection_timeout
    end

    def max_retries
      @config.max_retries
    end

    def max_recursion_depth
      @config.max_recursion_depth
    end

    def pgmq_shutdown_timeout
      @config.pgmq_shutdown_timeout
    end

    def pgmq_max_batch_size
      @config.pgmq_max_batch_size
    end

    # Get configuration summary for debugging
    def summary
      @config_manager.summary
    end

    # Get available component names
    def component_names
      @config_manager.component_names
    end

    # Get configuration root directory
    def config_root
      @config_manager.config_root
    end

    # Get specific component configuration
    def component(name)
      @config_manager.component(name)
    end

    # Reload configuration (creates new manager instance)
    def reload!
      load_unified_config!
    end

    # Class method to configure with block (for compatibility)
    def self.configure
      yield instance if block_given?
      instance
    end

    # Get the singleton instance
    def self.current
      instance
    end

    private

    def detect_environment
      ENV['TASKER_ENV'] || ENV['RAILS_ENV'] || ENV['RACK_ENV'] || ENV['APP_ENV'] || 'development'
    end

    def load_unified_config!
      # Use UnifiedConfig::Manager with fail-fast behavior
      # This will raise an exception if configuration is invalid or missing
      @config_manager = TaskerCore::UnifiedConfig::Manager.new_with_environment(@environment)

      # Get the complete configuration as Ruby Hash from Rust (via serde_magnus)
      # All validation has already been done on the Rust side
      raw_hash = @config_manager.config

      # Debug: Check what type of object we got back
      unless raw_hash.is_a?(Hash)
        raise TaskerCore::Errors::ConfigurationError,
              "Expected Hash from config_manager.config, got #{raw_hash.class}"
      end

      # Convert to symbolized keys for dry-types compatibility
      symbolized_hash = raw_hash.deep_symbolize_keys

      # Create type-safe configuration using dry-types validation
      @config = TaskerCore::Types::Config::TaskerConfig.new(symbolized_hash)

      # Store raw config for deprecated methods (frozen)
      @raw_config = symbolized_hash.freeze

      @config
    rescue StandardError => e
      # Re-raise with more context but don't hide the original error
      raise TaskerCore::Errors::ConfigurationError,
            "Failed to load unified TOML configuration for environment '#{@environment}': #{e.message}"
    end
  end
end
