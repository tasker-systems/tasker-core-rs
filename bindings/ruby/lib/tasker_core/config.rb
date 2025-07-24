# frozen_string_literal: true

require 'yaml'
require 'pathname'
require 'singleton'

module TaskerCore
  # Configuration management system for TaskerCore Ruby bindings
  #
  # Provides a flexible configuration system that allows user applications
  # to override default paths and settings while maintaining consistency
  # between Ruby and Rust sides of the boundary.
  #
  # @example Basic usage with defaults
  #   config = TaskerCore::Config.instance
  #   zmq_config = config.zeromq
  #   puts zmq_config.step_sub_endpoint
  #
  # @example Override config directory 
  #   TaskerCore::Config.config_directory = "/path/to/my/app/config"
  #   config = TaskerCore::Config.instance
  #
  # @example Override in user application
  #   TaskerCore.configure do |config|
  #     config.config_directory = Rails.root.join("config", "tasker")
  #     config.environment = Rails.env
  #   end
  #
  class Config
    include Singleton

    # Default configuration directory - can be overridden by user applications
    DEFAULT_CONFIG_DIR = File.expand_path("../../../../config", __dir__).freeze
    
    attr_accessor :config_directory, :environment
    attr_reader :loaded_config

    def initialize
      @config_directory = DEFAULT_CONFIG_DIR
      @environment = detect_environment
      @loaded_config = nil
      @zeromq_config = nil
    end

    # Detect current environment from common environment variables
    def detect_environment
      ENV['RAILS_ENV'] || ENV['RACK_ENV'] || ENV['APP_ENV'] || ENV['TASKER_ENV'] || 'development'
    end

    # Load and parse the ZeroMQ configuration file
    def load_config!
      config_file = find_config_file
      
      unless config_file && File.exist?(config_file)
        raise ConfigurationError, "Configuration file not found. Looked for: #{config_file}"
      end

      @loaded_config = YAML.load_file(config_file)
      
      # Validate configuration structure
      validate_config!
      
      # Initialize ZeroMQ config from loaded data
      @zeromq_config = nil # Reset to force reload
      
      @loaded_config
    rescue Psych::SyntaxError => e
      raise ConfigurationError, "Invalid YAML in configuration file #{config_file}: #{e.message}"
    rescue StandardError => e
      raise ConfigurationError, "Failed to load configuration from #{config_file}: #{e.message}"
    end

    # Get ZeroMQ configuration with environment-specific overrides
    def zeromq
      @zeromq_config ||= ZeroMQConfig.new(self)
    end

    # Get the current effective configuration (loaded + environment overrides)
    def effective_config
      load_config! unless @loaded_config
      
      base_config = @loaded_config.dup
      env_config = @loaded_config[@environment]
      
      if env_config
        deep_merge(base_config, env_config)
      else
        base_config
      end
    end

    # Find the configuration file, checking multiple possible locations
    def find_config_file
      possible_files = [
        File.join(@config_directory, 'zeromq.yaml'),
        File.join(@config_directory, 'zeromq.yml'), 
        File.join(@config_directory, 'tasker_zeromq.yaml'),
        File.join(@config_directory, 'tasker_zeromq.yml'),
        File.join(@config_directory, 'tasker.yaml'),
        File.join(@config_directory, 'tasker.yml')
      ]
      
      possible_files.find { |file| File.exist?(file) }
    end

    # Validate the loaded configuration has required structure
    def validate_config!
      return unless @loaded_config
      
      unless @loaded_config.key?('step_execution') || @loaded_config[@environment]&.key?('step_execution')
        raise ConfigurationError, "Configuration must contain 'step_execution' section"
      end
    end

    # Deep merge two hashes (for environment overrides)
    def deep_merge(base, override)
      base.merge(override) do |key, base_val, override_val|
        if base_val.is_a?(Hash) && override_val.is_a?(Hash)
          deep_merge(base_val, override_val)
        else
          override_val
        end
      end
    end

    # ZeroMQ-specific configuration wrapper
    class ZeroMQConfig
      attr_reader :config_instance

      def initialize(config_instance)
        @config_instance = config_instance
      end

      # Get step subscriber endpoint (Ruby receives step batches from Rust)
      def step_sub_endpoint
        step_config.fetch('handler_step_subscriber', tcp_defaults[:step_sub_endpoint])
      end

      # Get result publisher endpoint (Ruby publishes results back to Rust)  
      def result_pub_endpoint
        step_config.fetch('handler_result_publisher', tcp_defaults[:result_pub_endpoint])
      end

      # Get maximum number of concurrent workers
      def max_workers
        step_config.fetch('max_workers', 10)
      end

      # Get batch size for step processing
      def batch_size
        step_config.fetch('batch_size', 10)
      end

      # Get result timeout in milliseconds
      def result_timeout_ms
        step_config.fetch('result_timeout_ms', 300_000)
      end

      # Get polling interval in milliseconds
      def poll_interval_ms
        step_config.fetch('handler_poll_interval_ms', 1)
      end

      # Get high-water mark settings
      def step_queue_hwm
        step_config.fetch('step_queue_hwm', 1000)
      end

      def result_queue_hwm
        step_config.fetch('result_queue_hwm', 1000)
      end

      # Convert to hash for FFI compatibility
      def to_h
        {
          step_sub_endpoint: step_sub_endpoint,
          result_pub_endpoint: result_pub_endpoint,
          max_workers: max_workers,
          batch_size: batch_size,
          result_timeout_ms: result_timeout_ms,
          poll_interval_ms: poll_interval_ms,
          step_queue_hwm: step_queue_hwm,
          result_queue_hwm: result_queue_hwm
        }
      end

      # Export configuration for debugging
      def to_debug_info
        {
          step_sub_endpoint: step_sub_endpoint,
          result_pub_endpoint: result_pub_endpoint,
          max_workers: max_workers,
          environment: @config_instance.environment,
          config_directory: @config_instance.config_directory,
          config_file: @config_instance.find_config_file
        }
      end

      private

      # Get the step_execution configuration section
      def step_config
        effective = @config_instance.effective_config
        effective.dig('step_execution') || {}
      end

      # Default TCP endpoints for cross-language communication
      def tcp_defaults
        {
          step_sub_endpoint: 'tcp://127.0.0.1:5555',
          result_pub_endpoint: 'tcp://127.0.0.1:5556'
        }
      end
    end

    # Note: ConfigurationError is defined in the main TaskerCore module
  end

  # Convenience method for user applications to configure TaskerCore
  #
  # @example Configure in Rails application
  #   TaskerCore.configure do |config|
  #     config.config_directory = Rails.root.join("config", "tasker")
  #     config.environment = Rails.env
  #   end
  #
  def self.configure
    yield Config.instance if block_given?
  end
end