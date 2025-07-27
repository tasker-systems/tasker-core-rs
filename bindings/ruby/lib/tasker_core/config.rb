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
    end

    # Detect current environment from common environment variables
    def detect_environment
      ENV['RAILS_ENV'] || ENV['RACK_ENV'] || ENV['APP_ENV'] || ENV['TASKER_ENV'] || 'development'
    end

    # Load and parse the configuration file
    def load_config!
      config_file = find_config_file

      unless config_file && File.exist?(config_file)
        raise ConfigurationError, "Configuration file not found. Looked for: #{config_file}"
      end

      @loaded_config = YAML.load_file(config_file)

      # Validate configuration structure
      validate_config!

      @loaded_config
    rescue Psych::SyntaxError => e
      raise ConfigurationError, "Invalid YAML in configuration file #{config_file}: #{e.message}"
    rescue StandardError => e
      raise ConfigurationError, "Failed to load configuration from #{config_file}: #{e.message}"
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
      default_config_file = File.join(@config_directory, 'tasker-config.yaml')
      environment_config_file = File.join(@config_directory, "tasker-config-#{@environment}.yaml")
      [default_config_file, environment_config_file].find { |file| File.exist?(file) }
    end

    # Validate the loaded configuration has required structure
    def validate_config!
      return unless @loaded_config
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
