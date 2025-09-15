# frozen_string_literal: true

require 'yaml'
require 'fileutils'
require 'pathname'
require_relative '../types/task_template'

module TaskerCore
  module Utils
    # TaskTemplate Migration Utility
    #
    # Converts legacy HandlerConfiguration YAML files to the new self-describing 
    # TaskTemplate format. Uses dry-types as middleware to ensure correctness and
    # consistency during transformation.
    #
    # Features:
    # - Transforms legacy HandlerConfiguration to TaskTemplate structure
    # - Fills sensible defaults for missing fields
    # - Includes comprehensive metadata for documentation and discovery
    # - Uses dry-types validation to ensure correctness
    # - Outputs .migrated.yaml files for evaluation before removing legacy versions
    # - Preserves original files for rollback capability
    # - Provides detailed migration reports
    class TaskTemplateMigrator
      attr_reader :logger, :migration_stats

      def initialize
        # Use a simple logger or fallback to puts if logger not available
        @logger = defined?(TaskerCore::Logging) ? TaskerCore::Logging.logger_for(self.class) : SimpleLogger.new
        @migration_stats = {
          files_processed: 0,
          files_migrated: 0,
          files_failed: 0,
          errors: []
        }
      end

      # Simple logger fallback for when TaskerCore logging isn't available
      class SimpleLogger
        def info(message)
          puts "ℹ️  #{message}"
        end

        def error(message)
          puts "❌ #{message}"
        end

        def debug(message)
          puts "🔧 #{message}"
        end
      end

      # Migrate a single YAML file from legacy to new format
      #
      # @param file_path [String] Path to the legacy YAML file
      # @return [Boolean] true if migration successful, false otherwise
      def migrate_file(file_path)
        file_path = Pathname.new(file_path).expand_path
        
        unless file_path.exist?
          @logger.error("📁 Migration failed: File not found: #{file_path}")
          record_error(file_path, "File not found")
          return false
        end

        @migration_stats[:files_processed] += 1
        @logger.info("🔄 Migrating: #{file_path}")

        begin
          # Load legacy YAML
          legacy_data = YAML.load_file(file_path)
          
          # Transform to new TaskTemplate format
          new_template_data = transform_legacy_to_task_template(legacy_data, file_path)
          
          # Validate using dry-types middleware
          validated_template = validate_with_dry_types(new_template_data)
          
          # Generate output file path
          output_path = file_path.sub_ext('.migrated.yaml')
          
          # Write migrated YAML with proper formatting
          write_migrated_yaml(output_path, validated_template)
          
          @migration_stats[:files_migrated] += 1
          @logger.info("✅ Migration successful: #{file_path} → #{output_path}")
          
          true
        rescue StandardError => e
          @logger.error("❌ Migration failed for #{file_path}: #{e.message}")
          record_error(file_path, e.message)
          false
        end
      end

      # Migrate all YAML files in a directory recursively
      #
      # @param directory_path [String] Path to directory containing YAML files
      # @param pattern [String] File pattern to match (default: '**/*.yaml')
      # @return [Hash] Migration results summary
      def migrate_directory(directory_path, pattern: '**/*.yaml')
        directory_path = Pathname.new(directory_path).expand_path
        
        unless directory_path.directory?
          @logger.error("📁 Migration failed: Directory not found: #{directory_path}")
          return migration_summary
        end

        @logger.info("🔍 Scanning directory: #{directory_path}")
        
        yaml_files = directory_path.glob(pattern).select(&:file?)
        @logger.info("📄 Found #{yaml_files.size} YAML files to process")

        yaml_files.each do |yaml_file|
          # Skip already migrated files
          next if yaml_file.to_s.include?('.migrated.yaml')
          
          migrate_file(yaml_file)
        end

        summary = migration_summary
        @logger.info("📊 Migration completed: #{summary}")
        summary
      end

      # Get migration summary
      #
      # @return [Hash] Summary of migration results
      def migration_summary
        {
          files_processed: @migration_stats[:files_processed],
          files_migrated: @migration_stats[:files_migrated],
          files_failed: @migration_stats[:files_failed],
          success_rate: calculate_success_rate,
          errors: @migration_stats[:errors]
        }
      end

      private

      # Transform legacy HandlerConfiguration data to TaskTemplate format
      #
      # @param legacy_data [Hash] Legacy YAML data
      # @param source_file [Pathname] Source file for metadata
      # @return [Hash] Transformed TaskTemplate data
      def transform_legacy_to_task_template(legacy_data, source_file)
        {
          # Core required attributes
          name: legacy_data['name'],
          namespace_name: legacy_data['namespace_name'],
          version: legacy_data['version'] || '1.0.0',
          
          # Self-describing structure with rich metadata
          description: legacy_data['description'] || generate_default_description(legacy_data),
          metadata: generate_metadata(legacy_data, source_file),
          
          # Task handler transformation
          task_handler: transform_task_handler(legacy_data),
          
          # System dependencies
          system_dependencies: transform_system_dependencies(legacy_data),
          
          # Domain events (new feature - start with empty array)
          domain_events: [],
          
          # Input schema transformation
          input_schema: legacy_data['schema'],
          
          # Step definitions transformation
          steps: transform_steps(legacy_data),
          
          # Environment overrides transformation
          environments: transform_environments(legacy_data),
          
          # Migration metadata
          loaded_from: source_file.to_s
        }
      end

      # Generate comprehensive metadata for the template
      #
      # @param legacy_data [Hash] Legacy YAML data
      # @param source_file [Pathname] Source file
      # @return [Hash] Template metadata
      def generate_metadata(legacy_data, source_file)
        {
          author: 'TaskerCore Migration Tool',
          tags: generate_tags(legacy_data),
          documentation_url: nil,
          created_at: Time.now.utc.iso8601,
          updated_at: Time.now.utc.iso8601,
          migration_info: {
            migrated_from: 'HandlerConfiguration',
            original_file: source_file.basename.to_s,
            migration_version: '1.0.0',
            migration_timestamp: Time.now.utc.iso8601
          }
        }
      end

      # Generate meaningful tags from legacy data
      #
      # @param legacy_data [Hash] Legacy YAML data
      # @return [Array<String>] Generated tags
      def generate_tags(legacy_data)
        tags = []
        
        # Add namespace-based tag
        tags << "namespace:#{legacy_data['namespace_name']}" if legacy_data['namespace_name']
        
        # Add workflow pattern tags based on step structure
        if legacy_data['step_templates']
          step_count = legacy_data['step_templates'].size
          case step_count
          when 1
            tags << 'pattern:single-step'
          when 2..4
            tags << 'pattern:linear' if appears_linear?(legacy_data['step_templates'])
          when 5..8
            tags << 'pattern:complex'
          else
            tags << 'pattern:large-workflow'
          end
          
          # Add dependency complexity tags
          has_dependencies = legacy_data['step_templates'].any? { |st| st['depends_on_step'] || st['depends_on_steps'] }
          tags << 'dependencies:none' unless has_dependencies
          tags << 'dependencies:simple' if has_dependencies && step_count <= 4
          tags << 'dependencies:complex' if has_dependencies && step_count > 4
        end
        
        # Add environment support tag
        tags << 'environments:configured' if legacy_data['environments']
        
        # Add migration tag
        tags << 'migrated:handler-configuration'
        
        tags
      end

      # Check if step templates appear to be linear (each step depends on previous)
      #
      # @param step_templates [Array<Hash>] Step templates
      # @return [Boolean] true if appears linear
      def appears_linear?(step_templates)
        return true if step_templates.size <= 1
        
        # Simple heuristic: count steps with dependencies
        steps_with_deps = step_templates.count { |st| st['depends_on_step'] || st['depends_on_steps'] }
        
        # Linear if most steps have exactly one dependency
        steps_with_deps.to_f / step_templates.size > 0.6
      end

      # Generate default description if none provided
      #
      # @param legacy_data [Hash] Legacy YAML data
      # @return [String] Generated description
      def generate_default_description(legacy_data)
        step_count = legacy_data['step_templates']&.size || 0
        "#{legacy_data['name']} workflow with #{step_count} steps (migrated from HandlerConfiguration)"
      end

      # Transform task handler information
      #
      # @param legacy_data [Hash] Legacy YAML data
      # @return [Hash, nil] Task handler definition
      def transform_task_handler(legacy_data)
        return nil unless legacy_data['task_handler_class']
        
        {
          callable: legacy_data['task_handler_class'],
          initialization: legacy_data['handler_config'] || {}
        }
      end

      # Transform system dependencies
      #
      # @param legacy_data [Hash] Legacy YAML data
      # @return [Hash] System dependencies
      def transform_system_dependencies(legacy_data)
        {
          primary: legacy_data['default_dependent_system'] || 'default',
          secondary: []
        }
      end

      # Transform step templates to step definitions
      #
      # @param legacy_data [Hash] Legacy YAML data
      # @return [Array<Hash>] Step definitions
      def transform_steps(legacy_data)
        return [] unless legacy_data['step_templates']
        
        legacy_data['step_templates'].map do |step_template|
          {
            name: step_template['name'],
            description: step_template['description'],
            handler: {
              callable: step_template['handler_class'],
              initialization: step_template['handler_config'] || {}
            },
            system_dependency: step_template['dependent_system'],
            dependencies: extract_dependencies(step_template),
            retry: {
              retryable: step_template['default_retryable'] || true,
              limit: step_template['default_retry_limit'] || 3,
              backoff: 'exponential',
              backoff_base_ms: 1000,
              max_backoff_ms: 30000
            },
            timeout_seconds: step_template['timeout_seconds'],
            publishes_events: step_template['custom_events']&.map { |event| event['name'] } || []
          }
        end
      end

      # Extract dependencies from legacy step template
      #
      # @param step_template [Hash] Legacy step template
      # @return [Array<String>] Dependencies
      def extract_dependencies(step_template)
        deps = []
        
        # Single dependency
        deps << step_template['depends_on_step'] if step_template['depends_on_step']
        
        # Multiple dependencies
        if step_template['depends_on_steps']
          deps.concat(step_template['depends_on_steps'])
        end
        
        deps.uniq
      end

      # Transform environment overrides
      #
      # @param legacy_data [Hash] Legacy YAML data
      # @return [Hash] Environment overrides
      def transform_environments(legacy_data)
        return {} unless legacy_data['environments']
        
        legacy_data['environments'].transform_values do |env_config|
          {
            task_handler: transform_environment_task_handler(env_config),
            steps: transform_environment_steps(env_config)
          }
        end
      end

      # Transform environment task handler override
      #
      # @param env_config [Hash] Environment configuration
      # @return [Hash, nil] Task handler override
      def transform_environment_task_handler(env_config)
        return nil unless env_config['default_context'] || env_config['default_options']
        
        initialization = {}
        initialization.merge!(env_config['default_context']) if env_config['default_context']
        initialization.merge!(env_config['default_options']) if env_config['default_options']
        
        return nil if initialization.empty?
        
        { initialization: initialization }
      end

      # Transform environment step overrides
      #
      # @param env_config [Hash] Environment configuration
      # @return [Array<Hash>] Step overrides
      def transform_environment_steps(env_config)
        return [] unless env_config['step_templates']
        
        env_config['step_templates'].map do |step_override|
          override = {
            name: step_override['name']
          }
          
          # Handler overrides
          if step_override['handler_config']
            override[:handler] = {
              initialization: step_override['handler_config']
            }
          end
          
          # Timeout overrides
          override[:timeout_seconds] = step_override['timeout_seconds'] if step_override['timeout_seconds']
          
          # Retry overrides
          if step_override['default_retryable'] || step_override['default_retry_limit']
            override[:retry] = {}
            override[:retry][:retryable] = step_override['default_retryable'] if step_override['default_retryable']
            override[:retry][:limit] = step_override['default_retry_limit'] if step_override['default_retry_limit']
          end
          
          override
        end
      end

      # Validate transformed data using dry-types middleware
      #
      # @param template_data [Hash] Transformed template data
      # @return [TaskerCore::Types::TaskTemplate] Validated template
      def validate_with_dry_types(template_data)
        # Use dry-types TaskTemplate as validation middleware
        TaskerCore::Types::TaskTemplate.new(template_data)
      rescue Dry::Struct::Error => e
        @logger.error("🔍 Dry-types validation failed: #{e.message}")
        @logger.debug("🔍 Template data: #{template_data.inspect}")
        raise "TaskTemplate validation failed: #{e.message}"
      end

      # Write migrated YAML with proper formatting and comments
      #
      # @param output_path [Pathname] Output file path
      # @param validated_template [TaskerCore::Types::TaskTemplate] Validated template
      def write_migrated_yaml(output_path, validated_template)
        # Convert dry-struct to hash for YAML output
        template_hash = validated_template.to_h
        
        # Add header comment
        yaml_content = generate_yaml_header(validated_template)
        yaml_content += template_hash.to_yaml
        
        # Write to file
        File.write(output_path, yaml_content)
        @logger.debug("📝 Written migrated template to: #{output_path}")
      end

      # Generate informative YAML header comment
      #
      # @param template [TaskerCore::Types::TaskTemplate] Validated template
      # @return [String] YAML header comment
      def generate_yaml_header(template)
        <<~HEADER
          # TaskTemplate Configuration (Migrated)
          # 
          # This file has been automatically migrated from the legacy HandlerConfiguration
          # format to the new self-describing TaskTemplate format.
          #
          # Template: #{template.template_key}
          # Migration Date: #{Time.now.utc.iso8601}
          # Migration Tool: TaskerCore::Utils::TaskTemplateMigrator v1.0.0
          #
          # Key Changes:
          # - Legacy 'step_templates' → 'steps' with enhanced structure
          # - Legacy 'handler_class' → 'handler.callable'
          # - Legacy 'handler_config' → 'handler.initialization'
          # - Added comprehensive metadata and system dependencies
          # - Enhanced retry configuration with backoff strategies
          # - Added support for domain events and improved environment overrides
          #
          # Next Steps:
          # 1. Review this migrated configuration
          # 2. Test with your workflow handlers
          # 3. Update handler classes to use new structure if needed
          # 4. Remove the original .yaml file when satisfied
          #
        HEADER
      end

      # Record migration error
      #
      # @param file_path [Pathname] File that failed
      # @param error_message [String] Error message
      def record_error(file_path, error_message)
        @migration_stats[:files_failed] += 1
        @migration_stats[:errors] << {
          file: file_path.to_s,
          error: error_message,
          timestamp: Time.now.utc.iso8601
        }
      end

      # Calculate success rate
      #
      # @return [Float] Success rate as percentage
      def calculate_success_rate
        return 0.0 if @migration_stats[:files_processed] == 0
        
        (@migration_stats[:files_migrated].to_f / @migration_stats[:files_processed] * 100).round(2)
      end
    end
  end
end