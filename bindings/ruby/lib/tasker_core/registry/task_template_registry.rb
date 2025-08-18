# frozen_string_literal: true

require_relative '../logging/logger'
require_relative '../types/task_template'
require_relative '../types/task_types'
require 'yaml'
require 'singleton'

module TaskerCore
  module Registry
    # Task Template Registry - Database operations for task templates
    #
    # Responsibilities:
    # - Load named task configurations from database via ActiveRecord
    # - Store task templates to database via ActiveRecord
    # - Manage task template YAML file loading and registration
    # - Validate task template compatibility with Rust
    #
    # Does NOT handle:
    # - Handler class resolution or instantiation
    # - Handler callable registration
    # - Step-level handler operations
    class TaskTemplateRegistry
      include Singleton
      attr_reader :logger

      def initialize
        @logger = TaskerCore::Logging::Logger.instance
        logger.info 'TaskTemplateRegistry initialized for database operations'
      end

      # Get task template configuration from database using ActiveRecord
      # @param task [TaskerCore::Database::Models::Task] Task ActiveRecord model
      # @return [TaskerCore::Types::TaskTemplate, nil] TaskTemplate or nil if not found
      def get_task_template(task)
        named_task = task.named_task
        config_hash = named_task.configuration

        return nil unless config_hash

        # Convert to TaskTemplate dry-struct
        symbolized_config = prepare_config_for_task_template(config_hash)
        TaskerCore::Types::TaskTemplate.new(symbolized_config)
      rescue StandardError => e
        logger.error "💥 TASK_TEMPLATE_REGISTRY: Error loading task template for task #{task.task_uuid}: #{e.message}"
        nil
      end

      # Get task template by namespace, name, and version
      # @param namespace [String] Namespace name
      # @param name [String] Task name
      # @param version [String] Task version
      # @return [TaskerCore::Types::TaskTemplate, nil] TaskTemplate or nil if not found
      def get_task_template_by_key(namespace, name, version = '1.0.0')
        named_task = TaskerCore::Database::Models::NamedTask
                     .joins(:task_namespace)
                     .find_by(
                       task_namespace: { name: namespace },
                       name: name,
                       version: version
                     )

        return nil unless named_task

        config_hash = named_task.configuration
        return nil unless config_hash

        symbolized_config = prepare_config_for_task_template(config_hash)
        TaskerCore::Types::TaskTemplate.new(symbolized_config)
      rescue StandardError => e
        template_key = "#{namespace}/#{name}:#{version}"
        logger.error "💥 TASK_TEMPLATE_REGISTRY: Error loading task template #{template_key}: #{e.message}"
        nil
      end

      # Register a TaskTemplate programmatically using dry-struct validation
      # @param template_data [Hash] TaskTemplate data structure
      # @return [Hash] Registration result with status and details
      def register_task_template(template_data)
        template = TaskerCore::Types::TaskTemplate.new(template_data)

        unless template.valid_for_registration?
          error_msg = "TaskTemplate failed registration validation: #{template.template_key}"
          logger.warn "⚠️ #{error_msg}"
          return { success: false, error: error_msg }
        end

        validate_rust_compatibility!(template)

        logger.info "📝 Registering TaskTemplate: #{template.template_key}"

        if register_task_template_in_database(template)
          {
            success: true,
            template_key: template.template_key,
            registered_at: Time.now.utc.iso8601,
            message: 'TaskTemplate registered in database'
          }
        else
          { success: false, error: 'Database registration failed' }
        end
      rescue Dry::Struct::Error => e
        error_msg = "TaskTemplate validation failed: #{e.message}"
        logger.error "❌ #{error_msg}"
        { success: false, error: error_msg }
      rescue ArgumentError => e
        error_msg = "TaskTemplate compatibility check failed: #{e.message}"
        logger.error "❌ #{error_msg}"
        { success: false, error: error_msg }
      rescue StandardError => e
        error_msg = "Registration failed: #{e.message}"
        logger.error error_msg
        { success: false, error: error_msg }
      end

      # Register a TaskTemplate from YAML file
      # @param file_path [String] Path to the YAML TaskTemplate file
      # @return [Hash] Registration result with status and details
      def register_task_template_from_yaml(file_path)
        logger.info "📝 Registering TaskTemplate from YAML: #{file_path}"

        unless File.exist?(file_path)
          error_msg = "TaskTemplate YAML file not found: #{file_path}"
          logger.error error_msg
          return { success: false, error: error_msg }
        end

        template = load_task_template_from_file(file_path)
        return { success: false, error: 'Failed to load valid TaskTemplate from file' } unless template

        if register_task_template_in_database(template)
          {
            success: true,
            template_key: template.template_key,
            registered_at: Time.now.utc.iso8601,
            message: 'TaskTemplate registered from YAML file'
          }
        else
          { success: false, error: 'Database registration failed' }
        end
      rescue StandardError => e
        error_msg = "Failed to register TaskTemplate from #{file_path}: #{e.message}"
        logger.error error_msg
        { success: false, error: error_msg }
      end

      # Register multiple TaskTemplates from a directory
      # @param directory_path [String] Path to directory containing YAML files
      # @return [Hash] Batch registration results
      def register_task_templates_from_directory(directory_path)
        logger.info "📁 Batch registering TaskTemplates from directory: #{directory_path}"

        unless Dir.exist?(directory_path)
          error_msg = "Directory not found: #{directory_path}"
          logger.error error_msg
          return { success: false, error: error_msg }
        end

        yaml_files = Dir.glob(File.join(directory_path, '**', '*.{yml,yaml}'))

        if yaml_files.empty?
          return {
            success: true,
            message: "No YAML files found in #{directory_path}",
            registered_count: 0,
            failed_count: 0
          }
        end

        results = {
          success: true,
          registered_templates: [],
          failed_templates: [],
          registered_count: 0,
          failed_count: 0
        }

        yaml_files.each do |file_path|
          result = register_task_template_from_yaml(file_path)

          if result[:success]
            results[:registered_templates] << file_path
            results[:registered_count] += 1
          else
            results[:failed_templates] << { file: file_path, error: result[:error] }
            results[:failed_count] += 1
          end
        end

        logger.info "📊 Batch registration complete: #{results[:registered_count]} successful, #{results[:failed_count]} failed"
        results
      end

      # List registered TaskTemplates from database
      # @return [Hash] List of registered templates with metadata
      def list_registered_task_templates
        logger.debug '📋 Listing registered TaskTemplates'

        templates = load_task_templates_from_database
        {
          success: true,
          templates: templates.map(&:template_key),
          total_count: templates.size,
          message: 'TaskTemplates loaded from database'
        }
      rescue StandardError => e
        error_msg = "Failed to list registered templates: #{e.message}"
        logger.error error_msg
        { success: false, error: error_msg }
      end

      # Check if a specific TaskTemplate is registered in database
      # @param namespace [String] Namespace name
      # @param name [String] Task name
      # @param version [String] Task version
      # @return [Hash] Registration status and details
      def task_template_registered?(namespace, name, version = '1.0.0')
        template_key = "#{namespace}/#{name}:#{version}"
        logger.debug "🔍 Checking registration status: #{template_key}"

        registered = TaskerCore::Database::Models::NamedTask
                     .joins(:task_namespace)
                     .exists?(
                       task_namespace: { name: namespace },
                       name: name,
                       version: version
                     )

        {
          success: true,
          registered: registered,
          template_key: template_key,
          message: registered ? 'TaskTemplate found in database' : 'TaskTemplate not found in database'
        }
      rescue ActiveRecord::StatementInvalid => e
        error_msg = "Database query failed while checking template status: #{e.message}"
        logger.error error_msg
        { success: false, error: error_msg }
      rescue StandardError => e
        error_msg = "Failed to check template status: #{e.message}"
        logger.error error_msg
        { success: false, error: error_msg }
      end

      # Load TaskTemplates from database
      # @return [Array<TaskerCore::Types::TaskTemplate>] Array of TaskTemplate instances from database
      def load_task_templates_from_database
        logger.debug '🔍 Loading TaskTemplates from database'

        named_tasks = TaskerCore::Database::Models::NamedTask.includes(:task_namespace).all

        named_tasks.filter_map do |named_task|
          config = named_task.configuration
          next unless config

          # Use configuration directly from JSONB database column
          # PostgreSQL JSONB is automatically parsed into a Ruby hash by ActiveRecord
          symbolized_config = prepare_config_for_task_template(config)
          TaskerCore::Types::TaskTemplate.new(symbolized_config)
        rescue StandardError => e
          logger.warn "⚠️ Failed to load TaskTemplate for #{named_task.name}: #{e.message}"
          nil
        end
      end

      private

      # Prepare configuration hash for TaskTemplate dry-struct conversion
      # @param config_hash [Hash] Raw configuration from database
      # @return [Hash] Symbolized and properly structured configuration
      def prepare_config_for_task_template(config_hash)
        # Use ActiveSupport's deep symbolization for all nested structures
        # This is required for dry-types validation to work correctly
        symbolized = config_hash.deep_symbolize_keys
        
        # Special handling for environments: keep environment names as strings but symbolize nested keys
        if symbolized[:environments].is_a?(Hash)
          symbolized[:environments] = symbolized[:environments].transform_keys(&:to_s)
        end
        
        symbolized
      end

      # Load TaskTemplate from YAML file
      # @param file_path [String] Path to YAML file
      # @return [TaskerCore::Types::TaskTemplate, nil] TaskTemplate instance or nil
      def load_task_template_from_file(file_path)
        logger.debug "📖 Loading TaskTemplate from file: #{file_path}"

        yaml_content = YAML.safe_load_file(file_path, permitted_classes: [Symbol])
        return nil unless yaml_content.is_a?(Hash)

        # Convert to TaskTemplate-compatible structure
        template_data = normalize_template_data(yaml_content)

        TaskerCore::Types::TaskTemplate.new(template_data)
      rescue Psych::SyntaxError => e
        logger.error "❌ YAML parsing failed for #{file_path}: #{e.message}"
        nil
      rescue Dry::Struct::Error => e
        logger.error "❌ TaskTemplate validation failed for #{file_path}: #{e.message}"
        nil
      rescue StandardError => e
        logger.error "❌ Error loading TaskTemplate from #{file_path}: #{e.message}"
        nil
      end

      # Register TaskTemplate in database using ActiveRecord
      # @param template [TaskerCore::Types::TaskTemplate] TaskTemplate instance
      # @return [Boolean] Success indicator
      def register_task_template_in_database(template)
        logger.debug "📝 Database registration: #{template.template_key}"

        configuration = build_database_configuration(template)

        TaskerCore::Database::Models::NamedTask.transaction do
          namespace = TaskerCore::Database::Models::TaskNamespace.find_or_create_by!(
            name: template.namespace_name
          )

          named_task = TaskerCore::Database::Models::NamedTask.find_or_initialize_by(
            task_namespace: namespace,
            name: template.name,
            version: template.version
          )

          named_task.description = template.description
          named_task.configuration = configuration
          named_task.save!
        end

        logger.info "✅ TaskTemplate registered in database: #{template.template_key}"
        true
      rescue ActiveRecord::RecordInvalid, ActiveRecord::RecordNotSaved => e
        logger.error "Database validation failed for #{template.template_key}: #{e.message}"
        false
      rescue StandardError => e
        logger.error "Database registration failed for #{template.template_key}: #{e.message}"
        false
      end

      # Validate that TaskTemplate has all required fields for Rust deserialization
      # @param template [TaskerCore::Types::TaskTemplate] TaskTemplate to validate
      # @raise [ArgumentError] if required fields are missing
      def validate_rust_compatibility!(template)
        required_fields = {
          name: template.name,
          namespace_name: template.namespace_name,
          version: template.version
        }

        missing_fields = []
        required_fields.each do |field, value|
          missing_fields << field if value.nil? || (value.respond_to?(:empty?) && value.empty?)
        end

        unless missing_fields.empty?
          raise ArgumentError,
                "TaskTemplate missing required fields for Rust compatibility: #{missing_fields.join(', ')}"
        end

        # Validate steps have required fields for new self-describing structure
        template.steps.each_with_index do |step, index|
          if step.name.nil? || step.name.empty?
            raise ArgumentError, "Step at index #{index} missing required 'name' field"
          end
          if step.handler.callable.nil? || step.handler.callable.empty?
            raise ArgumentError, "Step '#{step.name}' missing required callable in handler definition"
          end
        end

        logger.debug "✅ TaskTemplate passed Rust compatibility validation: #{template.template_key}"
      end

      # Build database configuration structure from TaskTemplate
      # @param template [TaskerCore::Types::TaskTemplate] TaskTemplate instance
      # @return [Hash] Hash configuration for database JSONB storage (compatible with Rust serde_json::from_value)
      def build_database_configuration(template)
        # Convert TaskTemplate to hash for JSONB storage
        # PostgreSQL will handle the JSON conversion, and Rust will receive it as serde_json::Value
        # The Rust side uses serde_json::from_value to deserialize this
        template.to_h
      end

      # Normalize template data from YAML for TaskTemplate creation
      # @param yaml_data [Hash] Raw YAML data
      # @return [Hash] Normalized template data
      def normalize_template_data(yaml_data)
        # Convert keys to symbols with special handling for environments
        normalized = deep_symbolize_keys(yaml_data)

        # Special handling for environments: keep environment names as strings but symbolize nested keys
        if normalized[:environments].is_a?(Hash)
          normalized[:environments] = normalized[:environments].transform_keys(&:to_s)
        end

        normalized
      end

      # Recursively symbolize keys in nested hashes and arrays
      # @param obj [Object] The object to process
      # @return [Object] The object with symbolized keys
      def deep_symbolize_keys(obj)
        case obj
        when Hash
          obj.transform_keys(&:to_sym).transform_values { |value| deep_symbolize_keys(value) }
        when Array
          obj.map { |item| deep_symbolize_keys(item) }
        else
          obj
        end
      end


      # Serialize environments for database storage
      # @param environments [Hash] Environment configurations
      # @return [Hash] Serialized environment data
      def serialize_environments(environments)
        return {} unless environments

        environments.transform_values do |env|
          if env.respond_to?(:to_h)
            env.to_h
          else
            env
          end
        end
      end

      # Get current environment name
      # @return [String] Current environment name
      def current_environment
        ENV['TASKER_ENV'] || ENV['RAILS_ENV'] || ENV['APP_ENV'] || ENV['RACK_ENV'] || 'development'
      end

      # Apply environment-specific overrides to template data
      # @param template_data [Hash] Base template data
      # @param environment_name [String] Environment name
      # @return [Hash] Template data with environment overrides applied
      def apply_environment_overrides(template_data, environment_name)
        environment_config = template_data[:environments][environment_name]
        return template_data unless environment_config&.dig(:step_overrides)

        # Apply step overrides
        step_overrides = environment_config[:step_overrides]
        template_data[:step_templates] = template_data[:step_templates].map do |step|
          step_name = step[:name]
          if step_overrides[step_name]
            step.merge(step_overrides[step_name])
          else
            step
          end
        end

        template_data
      end
    end
  end
end
