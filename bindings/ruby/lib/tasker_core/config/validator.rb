# frozen_string_literal: true

require_relative '../utils/path_resolver'

module TaskerCore
  module ConfigValidation
    # Configuration validator that catches setup issues early in the boot process
    #
    # This validator performs comprehensive checks on:
    # - Project structure and file locations
    # - Search path configuration and file discovery
    # - Database configuration
    # - Template file validity
    #
    # @example Basic validation
    #   validator = TaskerCore::Config::Validator.new
    #   validator.validate!
    #
    # @example Custom validation with specific checks
    #   validator = TaskerCore::Config::Validator.new
    #   validator.validate_project_structure
    #   validator.validate_search_paths
    #
    class Validator
      # Custom error for configuration validation failures
      class ValidationError < StandardError; end

      def initialize(config = nil)
        @config = config || TaskerCore::Config.instance
        @errors = []
        @warnings = []
      end

      # Run all validations and raise if any critical errors found
      # @raise [ValidationError] If critical configuration issues found
      def validate!
        validate_project_structure
        validate_search_paths
        validate_database_config
        validate_template_files

        if @errors.any?
          error_message = build_error_message
          raise ValidationError, error_message
        end

        log_validation_summary
      end

      # Validate project structure and file locations
      def validate_project_structure
        structure = Utils::PathResolver.project_structure_summary

        add_error("Project root directory does not exist: #{structure[:project_root]}") unless structure[:exists]

        unless structure[:cargo_toml]
          add_warning('Cargo.toml not found - may indicate incorrect project root detection')
        end

        add_warning('config/ directory not found at project root') unless structure[:config_dir]

        unless structure[:ruby_bindings]
          add_error("Ruby bindings directory not found: #{File.join(structure[:project_root], 'bindings', 'ruby')}")
        end

        # Check if we're running from an unexpected location
        return unless structure[:working_directory] != structure[:project_root]

        add_info("Running from subdirectory: #{structure[:relative_working_dir]}")
      end

      # Validate search paths configuration and file discovery
      def validate_search_paths
        paths = @config.task_template_search_paths

        if paths.empty?
          add_error("No TaskTemplate search paths configured for environment: #{@config.environment}")
          return
        end

        validation_results = Utils::PathResolver.validate_search_paths(paths)

        # Add warnings from path validation
        validation_results[:warnings].each { |warning| add_warning(warning) }

        if validation_results[:total_files].zero?
          add_warning("No TaskTemplate files found in any search path for environment: #{@config.environment}")
          add_info('Configured search paths:')
          paths.each { |path| add_info("  - #{path}") }
        end

        return unless validation_results[:valid_paths].zero?

        add_error('None of the configured search paths contain any files')
      end

      # Validate database configuration
      def validate_database_config
        unless @config.database_config_exists?
          add_error("No database configuration found for environment: #{@config.environment}")
          return
        end

        db_config = @config.database_config_for_env

        required_keys = %w[adapter host database username]
        missing_keys = required_keys.select do |key|
          (db_config[key] || db_config[key.to_sym]).nil? || (db_config[key] || db_config[key.to_sym]).to_s.empty?
        end

        add_error("Missing required database configuration keys: #{missing_keys.join(', ')}") if missing_keys.any?

        # Validate database URL if present
        return unless db_config['url'] && !valid_database_url?(db_config['url'])

        add_error("Invalid database URL format: #{db_config['url']}")
      end

      # Validate template files can be parsed
      def validate_template_files
        paths = @config.task_template_search_paths
        template_files = paths.flat_map { |pattern| Dir.glob(Utils::PathResolver.resolve_config_path(pattern)) }

        return if template_files.empty?

        invalid_files = []
        template_files.each do |file_path|
          YAML.load_file(file_path)
        rescue Psych::SyntaxError => e
          invalid_files << "#{file_path}: #{e.message}"
        rescue StandardError => e
          invalid_files << "#{file_path}: #{e.class} - #{e.message}"
        end

        return unless invalid_files.any?

        add_error('Invalid TaskTemplate YAML files found:')
        invalid_files.each { |error| add_error("  - #{error}") }
      end

      # Get validation summary
      # @return [Hash] Summary of validation results
      def validation_summary
        {
          errors: @errors.length,
          warnings: @warnings.length,
          error_messages: @errors,
          warning_messages: @warnings,
          project_structure: Utils::PathResolver.project_structure_summary,
          search_paths: @config.task_template_search_paths,
          environment: @config.environment
        }
      end

      private

      def add_error(message)
        @errors << message
        logger&.error "❌ CONFIG_VALIDATION: #{message}"
      end

      def add_warning(message)
        @warnings << message
        logger&.warn "⚠️  CONFIG_VALIDATION: #{message}"
      end

      def add_info(message)
        logger&.info "ℹ️  CONFIG_VALIDATION: #{message}"
      end

      def build_error_message
        message = "TaskerCore configuration validation failed:\n"
        @errors.each { |error| message += "  • #{error}\n" }

        if @warnings.any?
          message += "\nWarnings:\n"
          @warnings.each { |warning| message += "  • #{warning}\n" }
        end

        message += "\nFor troubleshooting help, run: bundle exec tasker-core diagnose"
        message
      end

      def log_validation_summary
        return unless logger

        if @errors.empty? && @warnings.empty?
          logger.info '✅ CONFIG_VALIDATION: All configuration checks passed'
        elsif @errors.empty?
          logger.info "✅ CONFIG_VALIDATION: Configuration valid (#{@warnings.length} warnings)"
        end

        return unless Utils::PathResolver.development_mode? && (@errors.any? || @warnings.any?)

        logger.info '🛠️  CONFIG_VALIDATION: Development mode - detailed summary:'
        summary = validation_summary
        logger.info "    Project root: #{summary[:project_structure][:project_root]}"
        logger.info "    Environment: #{summary[:environment]}"
        logger.info "    Search paths: #{summary[:search_paths].length}"
        summary[:search_paths].each { |path| logger.info "      - #{path}" }
      end

      def valid_database_url?(url)
        uri = URI.parse(url)
        uri.scheme && uri.host
      rescue URI::InvalidURIError
        false
      end

      def logger
        TaskerCore::Logging::Logger.instance
      end
    end
  end
end
