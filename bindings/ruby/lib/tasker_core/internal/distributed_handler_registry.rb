# frozen_string_literal: true

require_relative '../logging/logger'
require_relative '../types/task_template'
require_relative '../types/task_types'
require_relative '../messaging/pgmq_client'
require 'yaml'
require 'singleton'

module TaskerCore
  module Orchestration
    # Distributed handler registry supporting flexible callable interfaces
    #
    # This registry extends the base TaskHandlerRegistry to support the revolutionary
    # .call(task, sequence, step) interface, enabling registration of Procs, Lambdas,
    # classes with call methods, and any callable object for distributed worker environments.
    #
    # @example Register a Proc
    #   registry = DistributedHandlerRegistry.new
    #   registry.register_proc('OrderProcessor') do |task, sequence, step|
    #     { status: 'completed', output: process_order(task.context) }
    #   end
    #
    # @example Register a Lambda
    #   order_validator = ->(task, sequence, step) do
    #     validate_order_data(task.context, step.handler_config)
    #   end
    #   registry.register_callable('OrderValidator', order_validator)
    #
    # @example Register a class-based callable
    #   class PaymentProcessor
    #     def call(task, sequence, step)
    #       process_payment(task.context[:payment_info])
    #     end
    #   end
    #   registry.register_callable('PaymentProcessor', PaymentProcessor.new)
    class DistributedHandlerRegistry
      include Singleton
      attr_reader :logger

      def initialize
        @callables = {}
        @validation_enabled = true
        @logger = TaskerCore::Logging::Logger.instance

        logger.info "DistributedHandlerRegistry initialized with callable support"
      end

      # Register a callable object directly
      # @param handler_class [String] The handler class name/identifier
      # @param callable [Object] Any object that responds to .call(task, sequence, step)
      # @return [void]
      def register_callable(handler_class, callable)
        validate_callable_interface!(callable) if @validation_enabled
        @callables[handler_class.to_s] = callable

        logger.debug "Registered callable for #{handler_class}: #{callable.class.name}"
      end

      # Register a Proc/Lambda for step processing
      # @param handler_class [String] The handler class name/identifier
      # @param block [Proc] Block that will be called with (task, sequence, step)
      # @return [void]
      def register_proc(handler_class, &block)
        raise ArgumentError, "Block required for register_proc" unless block_given?
        register_callable(handler_class, block)
      end

      # Register a class that has a call method
      # @param handler_class [String] The handler class name/identifier
      # @param klass [Class] Class that responds to .call or can be instantiated to create callable
      # @return [void]
      def register_class(handler_class, klass)
        if klass.respond_to?(:call)
          # Class itself is callable (has class method .call)
          register_callable(handler_class, klass)
        else
          # Instantiate the class and register the instance
          instance = klass.new
          register_callable(handler_class, instance)
        end
      end

      # Get callable for step execution using priority resolution order
      # @param handler_class [String] The handler class name/identifier
      # @return [Object, nil] Callable object or nil if none found
      def get_callable_for_class(handler_class)
        handler_key = handler_class.to_s

        # Priority 1: Direct callable registration (Procs, Lambdas, etc.)
        return @callables[handler_key] if @callables.key?(handler_key)

        # Priority 2: Class with .call method
        begin
          klass = handler_key.constantize
          return klass if klass.respond_to?(:call)
        rescue NameError
          # Class doesn't exist, continue to next priority
        end

        # Priority 3: Instance with .call method
        instance = get_handler_instance(handler_key)
        return instance if instance&.respond_to?(:call)

        nil
      end

      # Get handler instance using traditional resolution
      # @param handler_class [String] The handler class name/identifier
      # @return [Object, nil] Handler instance or nil if not found
      def get_handler_instance(handler_class)
        # Direct class instantiation
        begin
          klass = handler_class.to_s.constantize
          klass.new
        rescue NameError => e
          logger.warn "Could not instantiate handler class #{handler_class}: #{e.message}"
          nil
        rescue => e
          logger.error "Error instantiating #{handler_class}: #{e.message}"
          nil
        end
      end

      # Remove a registered callable
      # @param handler_class [String] The handler class name/identifier
      # @return [Object, nil] The removed callable or nil if not found
      def unregister_callable(handler_class)
        @callables.delete(handler_class.to_s)
      end

      # List all registered callables
      # @return [Hash] Hash of handler_class => callable
      def list_callables
        @callables.dup
      end

      # Check if a callable is registered for a handler class
      # @param handler_class [String] The handler class name/identifier
      # @return [Boolean] True if callable is registered
      def callable_registered?(handler_class)
        @callables.key?(handler_class.to_s)
      end

      # Clear all registered callables
      # @return [void]
      def clear_callables!
        @callables.clear
        logger.info "Cleared all registered callables"
      end

      # Enable or disable callable validation
      # @param enabled [Boolean] Whether to validate callable interfaces
      # @return [void]
      def validation_enabled=(enabled)
        @validation_enabled = !!enabled
        logger&.debug "Callable validation #{enabled ? 'enabled' : 'disabled'}"
      end

      # Get statistics about registered callables
      # @return [Hash] Statistics about the registry
      def stats
        {
          total_callables: @callables.size,
          callable_types: @callables.values.group_by(&:class).transform_values(&:size),
          validation_enabled: @validation_enabled,
          handler_classes: @callables.keys
        }
      end

      # TaskTemplate Registration API
      # These methods provide a unified API for registering TaskTemplates
      # from YAML files, programmatically, or in batches.

      # Register a TaskTemplate programmatically using dry-struct validation
      # @param template_data [Hash] TaskTemplate data structure
      # @return [Hash] Registration result with status and details
      def register_task_template(template_data)
        begin
          # Create TaskTemplate using dry-struct (handles validation automatically)
          template = TaskerCore::Types::TaskTemplate.new(template_data)

          unless template.valid_for_registration?
            return { success: false, error: "TaskTemplate failed registration validation" }
          end

          logger.info "📝 Registering TaskTemplate: #{template.template_key}"

          # Register in database
          if register_task_template_in_database(template)
            {
              success: true,
              template_key: template.template_key,
              registered_at: Time.now.utc.iso8601,
              message: "TaskTemplate registered in database-backed registry"
            }
          else
            { success: false, error: "Database registration failed" }
          end
        rescue Dry::Struct::Error => e
          error_msg = "TaskTemplate validation failed: #{e.message}"
          logger.error error_msg
          { success: false, error: error_msg }
        rescue StandardError => e
          error_msg = "Registration failed: #{e.message}"
          logger.error error_msg
          { success: false, error: error_msg }
        end
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

        begin
          template = load_task_template_from_file(file_path)
          return { success: false, error: "Failed to load valid TaskTemplate from file" } unless template

          if register_task_template_in_database(template)
            {
              success: true,
              template_key: template.template_key,
              registered_at: Time.now.utc.iso8601,
              message: "TaskTemplate registered from YAML file"
            }
          else
            { success: false, error: "Database registration failed" }
          end
        rescue StandardError => e
          error_msg = "Failed to register TaskTemplate from #{file_path}: #{e.message}"
          logger.error error_msg
          { success: false, error: error_msg }
        end
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
        logger.debug "📋 Listing registered TaskTemplates"

        begin
          templates = load_task_templates_from_database
          {
            success: true,
            templates: templates.map(&:template_key),
            total_count: templates.size,
            message: "TaskTemplates loaded from database-backed registry"
          }
        rescue StandardError => e
          error_msg = "Failed to list registered templates: #{e.message}"
          logger.error error_msg
          { success: false, error: error_msg }
        end
      end

      # Check if a specific TaskTemplate is registered in database
      # @param namespace [String] Namespace name
      # @param name [String] Task name
      # @param version [String] Task version
      # @return [Hash] Registration status and details
      def task_template_registered?(namespace, name, version = '1.0.0')
        template_key = "#{namespace}/#{name}:#{version}"
        logger.debug "🔍 Checking registration status: #{template_key}"

        begin
          return { success: false, registered: false, template_key: template_key, message: "Database not available" } unless database_available?

          db = get_database_connection
          result = db.exec_params(
            "SELECT EXISTS(
               SELECT 1 FROM task_namespaces ns
               JOIN named_tasks nt ON ns.id = nt.namespace_id
               WHERE ns.name = $1 AND nt.name = $2 AND nt.version = $3
             )",
            [namespace, name, version]
          )

          registered = result[0]['exists'] == 't'
          {
            success: true,
            registered: registered,
            template_key: template_key,
            message: registered ? "TaskTemplate found in database" : "TaskTemplate not found in database"
          }
        rescue StandardError => e
          error_msg = "Failed to check template status: #{e.message}"
          logger.error error_msg
          { success: false, error: error_msg }
        end
      end

      # Bootstrap handler registry by loading task templates from database
      # @return [Hash] Bootstrap operation result
      def bootstrap_handlers
        logger.info "🔧 Bootstrapping distributed handler registry"

        begin
          # TODO: Implement database TaskTemplate loading in Phase 4.5
          # For now, this will be a placeholder that prepares for handler registration

          registered_count = 0
          failed_count = 0

          # Load known handler classes from configuration or discovery
          discoverable_handlers = discover_handler_classes

          discoverable_handlers.each do |handler_class|
            begin
              # Try to register handler class if it exists and has proper interface
              register_class(handler_class, handler_class.constantize)
              registered_count += 1
              logger.debug "✅ Handler registered: #{handler_class}"
            rescue NameError
              logger.debug "⚠️ Handler class not found: #{handler_class}"
              failed_count += 1
            rescue StandardError => e
              logger.warn "❌ Failed to register handler #{handler_class}: #{e.message}"
              failed_count += 1
            end
          end

          {
            'status' => 'success',
            'registered_handlers' => registered_count,
            'failed_handlers' => failed_count,
            'total_callables' => @callables.size,
            'bootstrapped_at' => Time.now.utc.iso8601
          }
        rescue StandardError => e
          logger.error "❌ Handler bootstrap failed: #{e.message}"
          {
            'status' => 'error',
            'error' => e.message,
            'bootstrapped_at' => Time.now.utc.iso8601
          }
        end
      end

      private

      # Validate that a callable has the correct interface
      def validate_callable_interface!(callable)
        unless callable.respond_to?(:call)
          raise ArgumentError, "Callable must respond to .call method"
        end

        # Check arity if possible (some callables don't support arity inspection)
        if callable.respond_to?(:arity)
          expected_arity = 3 # (task, sequence, step)
          actual_arity = callable.arity

          # Handle variable arity (-1 means accepts any number of args)
          if actual_arity >= 0 && actual_arity != expected_arity
            logger.warn "Callable arity is #{actual_arity}, expected #{expected_arity} (task, sequence, step)"
          end
        end

        # Additional validation for common callable types
        case callable
        when Proc, Method
          # These are always valid if they respond to call
          true
        when Class
          # Class should have a call method
          unless callable.respond_to?(:call)
            raise ArgumentError, "Class #{callable.name} must have a .call class method"
          end
        else
          # Instance should have call method (already checked above)
          true
        end
      end

      # Discover and load TaskTemplate configurations from YAML files and database
      # @return [Array<String>] List of handler class names loaded from TaskTemplates
      def discover_handler_classes
        handlers = []

        # Load TaskTemplates from YAML files into database
        task_templates = discover_task_templates

        task_templates.each do |template|
          handlers.concat(template.handler_class_names)
        end

        # Only include handlers that actually exist in the current environment
        handlers.uniq.select do |handler_class|
          begin
            handler_class.constantize
            true
          rescue NameError
            false
          end
        end
      end

      # Discover TaskTemplate YAML files and load them into database
      # @return [Array<TaskerCore::Types::TaskTemplate>] Array of TaskTemplate instances
      def discover_task_templates
        templates = []

        # Get search paths from configuration
        search_patterns = get_search_patterns_from_config

        search_patterns.each do |pattern|
          Dir.glob(pattern).each do |file_path|
            begin
              template = load_task_template_from_file(file_path)
              if template&.valid_for_registration?
                # Register in database using the database-first registry approach
                register_task_template_in_database(template)
                templates << template
                logger.info "✅ Loaded TaskTemplate from #{file_path}: #{template.template_key}"
              else
                logger.warn "⚠️ Invalid TaskTemplate in #{file_path}"
              end
            rescue StandardError => e
              logger.warn "⚠️ Failed to load TaskTemplate from #{file_path}: #{e.message}"
            end
          end
        end

        # Also load from database (for templates loaded by other processes)
        database_templates = load_task_templates_from_database
        templates.concat(database_templates)

        logger.info "🔍 Discovered #{templates.size} TaskTemplates total"
        templates
      end

      # Get search patterns from configuration for current environment
      # @return [Array<String>] Array of glob patterns to search
      def get_search_patterns_from_config
        config = load_tasker_config
        env = current_environment

        patterns = config.dig('task_templates', 'search_paths', env) || []

        if patterns.empty?
          logger.warn "No TaskTemplate search paths configured for environment: #{env}, using defaults"
          # Fallback to default patterns for test environment
          patterns = default_search_patterns_for_environment(env)
        end

        # Expand relative paths from project root
        project_root = find_project_root
        expanded_patterns = patterns.map do |pattern|
          if pattern.start_with?('/')
            pattern # Absolute path
          else
            File.join(project_root, pattern)
          end
        end

        logger.debug "📁 TaskTemplate search patterns for #{env}: #{expanded_patterns}"
        expanded_patterns
      end

      # Find the project root directory
      # @return [String] Path to project root
      def find_project_root
        # Start from current file and walk up until we find a Cargo.toml or Gemfile
        current_dir = File.dirname(__FILE__)

        while current_dir != '/'
          return current_dir if File.exist?(File.join(current_dir, 'Cargo.toml'))
          return current_dir if File.exist?(File.join(current_dir, 'Gemfile'))
          current_dir = File.dirname(current_dir)
        end

        # Fallback to current directory
        Dir.pwd
      end

      # Load TaskTemplate data from a YAML file using dry-struct
      # @param file_path [String] Path to the YAML file
      # @return [TaskerCore::Types::TaskTemplate, nil] TaskTemplate instance or nil if invalid
      def load_task_template_from_file(file_path)
        raw_data = YAML.load_file(file_path)
        return nil unless raw_data.is_a?(Hash)

        # Convert string keys to symbols and set defaults
        normalized_data = {
          name: raw_data['name'] || File.basename(file_path, File.extname(file_path)),
          namespace_name: raw_data['namespace_name'] || 'default',
          version: raw_data['version'] || '1.0.0',
          task_handler_class: raw_data['task_handler_class'],
          module_namespace: raw_data['module_namespace'],
          description: raw_data['description'],
          default_dependent_system: raw_data['default_dependent_system'],
          schema: raw_data['schema'],
          named_steps: raw_data['named_steps'] || [],
          step_templates: normalize_step_templates_to_structs(raw_data['step_templates'] || []),
          environments: normalize_environments_to_structs(raw_data['environments'] || {}),
          handler_config: raw_data['handler_config'] || {},
          custom_events: raw_data['custom_events'] || [],
          loaded_from: file_path
        }

        # Create TaskTemplate using dry-struct (handles validation automatically)
        TaskerCore::Types::TaskTemplate.new(normalized_data)
      rescue Dry::Struct::Error => e
        logger.warn "TaskTemplate validation failed for #{file_path}: #{e.message}"
        nil
      rescue StandardError => e
        logger.error "Error loading TaskTemplate from #{file_path}: #{e.message}"
        nil
      end

      # Normalize step templates to dry-struct instances
      # @param step_templates [Array] Raw step template data
      # @return [Array<TaskerCore::Types::StepTemplate>] Normalized step templates
      def normalize_step_templates_to_structs(step_templates)
        return [] unless step_templates.is_a?(Array)

        step_templates.map do |step|
          TaskerCore::Types::StepTemplate.new(
            name: step['name'],
            description: step['description'],
            handler_class: step['handler_class'],
            handler_config: step['handler_config'] || {},
            depends_on_step: step['depends_on_step'],
            depends_on_steps: step['depends_on_steps'] || [],
            default_retryable: step['default_retryable'] || false,
            default_retry_limit: step['default_retry_limit'] || 0,
            timeout_seconds: step['timeout_seconds']
          )
        end
      rescue Dry::Struct::Error => e
        logger.warn "Step template validation failed: #{e.message}"
        []
      end

      # Normalize environments to dry-struct instances
      # @param environments [Hash] Raw environment data
      # @return [Hash] Normalized environment configurations
      def normalize_environments_to_structs(environments)
        return {} unless environments.is_a?(Hash)

        environments.transform_values do |env_config|
          step_overrides = {}
          if env_config['step_overrides'].is_a?(Hash)
            env_config['step_overrides'].each do |step_name, override_data|
              step_overrides[step_name] = TaskerCore::Types::StepOverride.new(
                retry_limit: override_data['retry_limit'],
                handler_config: override_data['handler_config'] || {}
              )
            end
          end

          TaskerCore::Types::EnvironmentConfig.new(step_overrides: step_overrides)
        end
      rescue Dry::Struct::Error => e
        logger.warn "Environment configuration validation failed: #{e.message}"
        {}
      end

      # Register TaskTemplate in database using database-first approach
      # This mimics the Rust TaskHandlerRegistry database operations
      # @param template [TaskerCore::Types::TaskTemplate] TaskTemplate instance
      # @return [Boolean] Success indicator
      def register_task_template_in_database(template)
        return false unless database_available?

        begin
          logger.debug "📝 Database registration: #{template.template_key}"

          # Use database connection from embedded orchestrator or pgmq client
          db = get_database_connection

          db.transaction do
            # 1. Find or create namespace
            namespace_result = db.exec_params(
              "INSERT INTO task_namespaces (name, created_at, updated_at)
               VALUES ($1, NOW(), NOW())
               ON CONFLICT (name) DO UPDATE SET updated_at = NOW()
               RETURNING id",
              [template.namespace_name]
            )
            namespace_id = namespace_result[0]['id']

            # 2. Insert or update named task
            configuration = {
              description: template.description,
              default_dependent_system: template.default_dependent_system,
              schema: template.schema,
              named_steps: template.named_steps,
              environments: serialize_environments(template.environments),
              handler_config: template.handler_config,
              custom_events: template.custom_events
            }

            task_result = db.exec_params(
              "INSERT INTO named_tasks (namespace_id, name, version, task_handler_class, module_namespace, description, configuration, created_at, updated_at)
               VALUES ($1, $2, $3, $4, $5, $6, $7, NOW(), NOW())
               ON CONFLICT (namespace_id, name, version) DO UPDATE SET
                 task_handler_class = EXCLUDED.task_handler_class,
                 module_namespace = EXCLUDED.module_namespace,
                 description = EXCLUDED.description,
                 configuration = EXCLUDED.configuration,
                 updated_at = NOW()
               RETURNING id",
              [namespace_id, template.name, template.version, template.task_handler_class,
               template.module_namespace, template.description, configuration.to_json]
            )
            task_id = task_result[0]['id']

            # 3. Clear existing step templates and insert new ones
            db.exec_params("DELETE FROM step_templates WHERE named_task_id = $1", [task_id])

            template.step_templates.each_with_index do |step, index|
              step_config = {
                description: step.description,
                handler_config: step.handler_config,
                depends_on_step: step.depends_on_step,
                depends_on_steps: step.depends_on_steps,
                default_retryable: step.default_retryable,
                default_retry_limit: step.default_retry_limit,
                timeout_seconds: step.timeout_seconds
              }

              db.exec_params(
                "INSERT INTO step_templates (named_task_id, name, handler_class, position, configuration, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, NOW(), NOW())",
                [task_id, step.name, step.handler_class, index, step_config.to_json]
              )
            end
          end

          logger.info "✅ TaskTemplate registered in database: #{template.template_key}"
          true
        rescue StandardError => e
          logger.error "Database registration failed for #{template.template_key}: #{e.message}"
          false
        end
      end

      # Load TaskTemplates from database (for templates registered by other processes)
      # @return [Array<TaskerCore::Types::TaskTemplate>] Array of TaskTemplate instances from database
      def load_task_templates_from_database
        return [] unless database_available?

        begin
          logger.debug "🔍 Loading TaskTemplates from database"

          db = get_database_connection

          # Query all named tasks with their namespaces and step templates
          result = db.exec(
            "SELECT nt.id, ns.name as namespace_name, nt.name, nt.version,
                    nt.task_handler_class, nt.module_namespace, nt.description, nt.configuration,
                    array_agg(
                      json_build_object(
                        'name', st.name,
                        'handler_class', st.handler_class,
                        'position', st.position,
                        'configuration', st.configuration
                      ) ORDER BY st.position
                    ) FILTER (WHERE st.id IS NOT NULL) as step_templates
             FROM task_namespaces ns
             JOIN named_tasks nt ON ns.id = nt.namespace_id
             LEFT JOIN step_templates st ON nt.id = st.named_task_id
             GROUP BY nt.id, ns.name, nt.name, nt.version, nt.task_handler_class,
                      nt.module_namespace, nt.description, nt.configuration
             ORDER BY ns.name, nt.name, nt.version"
          )

          templates = result.map do |row|
            config = JSON.parse(row['configuration'] || '{}')
            step_templates_data = JSON.parse(row['step_templates'] || '[]')

            # Convert step templates to dry-structs
            step_templates = step_templates_data.map do |step_data|
              step_config = JSON.parse(step_data['configuration'] || '{}')
              TaskerCore::Types::StepTemplate.new(
                name: step_data['name'],
                description: step_config['description'],
                handler_class: step_data['handler_class'],
                handler_config: step_config['handler_config'] || {},
                depends_on_step: step_config['depends_on_step'],
                depends_on_steps: step_config['depends_on_steps'] || [],
                default_retryable: step_config['default_retryable'] || false,
                default_retry_limit: step_config['default_retry_limit'] || 0,
                timeout_seconds: step_config['timeout_seconds']
              )
            end

            # Convert environments to dry-structs
            environments = normalize_environments_to_structs(config['environments'] || {})

            TaskerCore::Types::TaskTemplate.new(
              name: row['name'],
              namespace_name: row['namespace_name'],
              version: row['version'],
              task_handler_class: row['task_handler_class'],
              module_namespace: row['module_namespace'],
              description: row['description'],
              default_dependent_system: config['default_dependent_system'],
              schema: config['schema'],
              named_steps: config['named_steps'] || [],
              step_templates: step_templates,
              environments: environments,
              handler_config: config['handler_config'] || {},
              custom_events: config['custom_events'] || [],
              loaded_from: 'database'
            )
          end

          logger.info "📄 Loaded #{templates.size} TaskTemplates from database"
          templates
        rescue StandardError => e
          logger.warn "Failed to load TaskTemplates from database: #{e.message}"
          []
        end
      end

      # Check if database is available for TaskTemplate operations
      # @return [Boolean] Database availability
      def database_available?
        begin
          db = get_database_connection
          # Test connection with a simple query
          db.exec("SELECT 1")
          true
        rescue StandardError => e
          logger.debug "Database not available: #{e.message}"
          false
        end
      end

      # Get database connection from orchestration system
      # @return [PG::Connection] Database connection
      def get_database_connection
        # Try to get connection from embedded orchestrator first
        if defined?(TaskerCore::EmbeddedOrchestrator)
          orchestrator = TaskerCore::EmbeddedOrchestrator.instance
          return orchestrator.database_connection if orchestrator.running?
        end

        # Fallback to pgmq client connection

        pgmq_client = TaskerCore::Messaging::PgmqClient.new
        pgmq_client.send(:connection)
      rescue StandardError => e
        logger.error "Failed to get database connection: #{e.message}"
        raise e
      end

      # Serialize environments hash for database storage
      # @param environments [Hash] Environment configurations
      # @return [Hash] Serializable environment data
      def serialize_environments(environments)
        return {} if environments.empty?

        environments.transform_values do |env_config|
          {
            'step_overrides' => env_config.step_overrides.transform_values do |override|
              {
                'retry_limit' => override.retry_limit,
                'handler_config' => override.handler_config
              }
            end
          }
        end
      end

      # Get current environment name
      # @return [String] Current environment (test, development, production, etc.)
      def current_environment
        # Try Rails first, then ENV, then default
        if defined?(Rails) && Rails.respond_to?(:env)
          Rails.env
        else
          ENV['RAILS_ENV'] || ENV['RUBY_ENV'] || ENV['RACK_ENV'] || ENV['TASKER_ENV'] || 'development'
        end
      end

      # Load Tasker configuration from YAML files
      # @return [Hash] Configuration hash
      def load_tasker_config
        env = current_environment
        project_root = find_project_root

        # Try to load environment-specific config first
        config_file = File.join(project_root, 'config', "tasker-config-#{env}.yaml")
        if File.exist?(config_file)
          return YAML.load_file(config_file)
        end

        # Fallback to default config
        default_config_file = File.join(project_root, 'config', 'tasker-config.yaml')
        if File.exist?(default_config_file)
          return YAML.load_file(default_config_file)
        end

        logger.warn "No TaskerCore configuration file found, using empty config"
        {}
      rescue StandardError => e
        logger.warn "Error loading Tasker configuration: #{e.message}"
        {}
      end

      # Default search patterns for environment when not configured
      # @param env [String] Environment name
      # @return [Array<String>] Default search patterns
      def default_search_patterns_for_environment(env)
        case env
        when 'test'
          [
            'spec/handlers/examples/**/config/*.{yml,yaml}',
            'spec/fixtures/task_templates/*.{yml,yaml}'
          ]
        when 'development'
          [
            'config/tasks/*.{yml,yaml}',
            'config/tasker/tasks/*.{yml,yaml}',
            'lib/tasks/*.{yml,yaml}'
          ]
        when 'production'
          [
            'config/tasks/*.{yml,yaml}',
            'config/tasker/tasks/*.{yml,yaml}'
          ]
        else
          []
        end
      end
    end
  end
end
