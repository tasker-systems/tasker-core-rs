# frozen_string_literal: true

require 'yaml'
require 'pathname'
require 'singleton'
require 'dotenv'
require 'erb'

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
    DEFAULT_CONFIG_DIR = File.expand_path('../../../../config', __dir__).freeze

    attr_accessor :config_directory, :environment
    attr_reader :loaded_config, :database_config

    def initialize
      Dotenv.load
      @config_directory = DEFAULT_CONFIG_DIR
      @environment = detect_environment
      @loaded_config = nil
      @database_config = nil
      load_config!
      load_database_config!
    end

    # Detect current environment from common environment variables
    def detect_environment
      ENV['RAILS_ENV'] || ENV['RACK_ENV'] || ENV['APP_ENV'] || ENV['TASKER_ENV'] || 'development'
    end

    # Load and parse the configuration file
    def load_config!
      config_file = find_config_file

      unless config_file && File.exist?(config_file)
        raise Errors::ConfigurationError, "Configuration file not found. Looked for: #{config_file}"
      end

      @loaded_config = YAML.load_file(config_file)

      # Validate configuration structure
      validate_config!

      @loaded_config
    rescue Psych::SyntaxError => e
      raise Errors::ConfigurationError, "Invalid YAML in configuration file #{config_file}: #{e.message}"
    rescue StandardError => e
      raise TaskerCore::Errors::ConfigurationError, "Failed to load configuration from #{config_file}: #{e.message}"
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

    # Find the configuration file - only use the unified config file
    def find_config_file
      File.join(@config_directory, 'tasker-config.yaml')
    end

    # Load and parse the database configuration from unified config
    def load_database_config!
      load_config! unless @loaded_config

      # Extract database configuration from unified config
      effective = effective_config
      base_db_config = effective['database']

      raise Errors::ConfigurationError, 'Database configuration not found in unified config file' unless base_db_config

      # Build database configuration with environment-specific structure
      @database_config = {
        'default' => base_db_config,
        @environment => base_db_config
      }

      # Validate database configuration structure
      validate_database_config!

      @database_config
    rescue StandardError => e
      raise Errors::ConfigurationError, "Failed to load database configuration from unified config: #{e.message}"
    end

    # Get the current database configuration for the environment
    def database_config_for_env(env = nil)
      env ||= @environment
      load_database_config! unless @database_config

      env_config = @database_config[env] || @database_config['default']
      raise Errors::ConfigurationError, "Database configuration not found for environment: #{env}" unless env_config

      # Make a copy to avoid modifying the original
      config = env_config.dup

      # Interpolate environment variables in database config values
      config = interpolate_env_vars_in_hash(config)

      # Clean up config for ActiveRecord compatibility
      config = clean_database_config_for_activerecord(config)

      # Convert string keys to symbols for ActiveRecord compatibility
      symbolize_keys(config)
    end

    # Get the current database configuration for ActiveRecord
    def activerecord_database_config(env = nil)
      database_config_for_env(env)
    end

    # Get database URL for the current environment (useful for external tools)
    def database_url(env = nil)
      config = database_config_for_env(env)
      "postgresql://#{config[:username]}:#{config[:password]}@#{config[:host]}/#{config[:database]}"
    end

    # Get TaskTemplate search paths for the current environment
    # @return [Array<String>] Array of file glob patterns to search for TaskTemplate YAML files
    def task_template_search_paths
      effective = effective_config
      task_template_config = effective['task_templates']

      if task_template_config && task_template_config['search_paths']
        # Convert relative paths to absolute paths from project root
        task_template_config['search_paths'].map do |path|
          Utils::PathResolver.resolve_config_path(path)
        end
      else
        # Fallback to safe default paths if not configured
        [
          Utils::PathResolver.resolve_config_path('config/task_templates/*.{yml,yaml}'),
          Utils::PathResolver.resolve_config_path('config/tasks/*.{yml,yaml}')
        ]
      end
    end

    # Add validation method that uses the new validator
    def validate!
      require_relative 'config/validator'
      validator = ConfigValidation::Validator.new(self)
      validator.validate!
    end

    # Check if database configuration exists for environment
    def database_config_exists?(env = nil)
      env ||= @environment
      load_database_config! unless @database_config
      @database_config.key?(env) || @database_config.key?('default')
    end

    # Get all available database environments
    def available_database_environments
      load_database_config! unless @database_config
      @database_config.keys.reject { |key| key == 'default' }
    end

    # Validate the loaded database configuration has required structure
    def validate_database_config!
      return unless @database_config

      # Check if we have at least one valid configuration
      valid_configs = @database_config.select do |_key, config|
        config.is_a?(Hash) && config['adapter']
      end

      if valid_configs.empty?
        raise Errors::ConfigurationError,
              "No valid database configurations found. Each configuration must have 'adapter' key."
      end

      # Validate each configuration
      @database_config.each do |env, config|
        next unless config.is_a?(Hash)

        unless config['adapter']
          raise Errors::ConfigurationError,
                "Missing required 'adapter' in database configuration for environment '#{env}'"
        end
      end
    end

    # Validate the loaded configuration has required structure
    def validate_config!
      nil unless @loaded_config
    end

    # Deep merge two hashes (for environment overrides)
    def deep_merge(base, override)
      base.merge(override) do |_key, base_val, override_val|
        if base_val.is_a?(Hash) && override_val.is_a?(Hash)
          deep_merge(base_val, override_val)
        else
          override_val
        end
      end
    end

    # Convert string keys to symbols recursively
    def symbolize_keys(hash)
      return hash unless hash.is_a?(Hash)

      hash.each_with_object({}) do |(key, value), result|
        new_key = key.is_a?(String) ? key.to_sym : key
        new_value = value.is_a?(Hash) ? symbolize_keys(value) : value
        result[new_key] = new_value
      end
    end

    # Interpolate environment variables in hash values
    def interpolate_env_vars_in_hash(hash)
      return hash unless hash.is_a?(Hash)

      hash.transform_values do |value|
        if value.is_a?(Hash)
          interpolate_env_vars_in_hash(value)
        elsif value.is_a?(String)
          # Handle ${VAR:-default} syntax
          value.gsub(/\$\{([^}]+)\}/) do |match|
            var_expr = ::Regexp.last_match(1)
            if var_expr.include?(':-')
              var_name, default_value = var_expr.split(':-', 2)
              ENV[var_name] || default_value
            else
              ENV[var_expr] || match
            end
          end
        else
          value
        end
      end
    end

    # Clean up database config for ActiveRecord compatibility
    def clean_database_config_for_activerecord(config)
      cleaned = config.dup

      # Remove Rust-specific configurations that ActiveRecord doesn't understand
      cleaned.delete('enable_secondary_database')

      # Ensure pool is an integer for ActiveRecord
      if cleaned['pool'].is_a?(String)
        cleaned['pool'] = cleaned['pool'].to_i
      elsif cleaned['pool'].is_a?(Hash)
        # If pool is a hash (Rust config), extract max_connections for ActiveRecord
        cleaned['pool'] = cleaned['pool']['max_connections'] || 25
      end

      cleaned
    end
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
