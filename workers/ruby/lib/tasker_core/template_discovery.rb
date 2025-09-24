# frozen_string_literal: true

require 'yaml'

module TaskerCore
  module TemplateDiscovery
    # Template path discovery following the same patterns as Rust TaskTemplateManager
    # Finds task template configuration directory using environment variables and workspace detection
    class TemplatePath
      class << self
        # Find the template configuration directory using the same logic as Rust
        # Returns path to directory containing .yaml template files
        def find_template_config_directory
          # First try TASK_TEMPLATE_PATH environment variable
          return ENV['TASK_TEMPLATE_PATH'] if ENV['TASK_TEMPLATE_PATH'] && Dir.exist?(ENV['TASK_TEMPLATE_PATH'])

          # Try WORKSPACE_PATH environment variable (for compatibility)
          if ENV['WORKSPACE_PATH']
            template_dir = File.join(ENV['WORKSPACE_PATH'], 'config', 'tasks')
            return template_dir if Dir.exist?(template_dir)
          end

          # Try workspace detection from current directory
          workspace_root = find_workspace_root
          if workspace_root
            template_dir = File.join(workspace_root, 'config', 'tasks')
            return template_dir if Dir.exist?(template_dir)
          end

          # Default fallback to current directory
          default_dir = 'config/tasks'
          return default_dir if Dir.exist?(default_dir)

          # If nothing found, return nil
          nil
        end

        # Discover all YAML template files in the template directory
        def discover_template_files(template_dir = nil)
          template_dir ||= find_template_config_directory
          return [] unless template_dir

          # Look for both .yaml and .yml file extensions
          yaml_files = Dir.glob(File.join(template_dir, '**', '*.yaml'))
          yml_files = Dir.glob(File.join(template_dir, '**', '*.yml'))
          (yaml_files + yml_files).sort.uniq
        end

        private

        # Find workspace root by looking for Cargo.toml or other workspace indicators
        def find_workspace_root(start_dir = Dir.pwd)
          current_dir = File.expand_path(start_dir)

          loop do
            # Look for workspace indicators
            return current_dir if workspace_indicators.any? do |indicator|
              File.exist?(File.join(current_dir, indicator))
            end

            parent_dir = File.dirname(current_dir)
            break if parent_dir == current_dir # Reached filesystem root

            current_dir = parent_dir
          end

          nil
        end

        def workspace_indicators
          ['Cargo.toml', '.git', 'Gemfile', 'package.json', 'tasker-core.code-workspace']
        end
      end
    end

    # YAML template parser that extracts handler information
    class TemplateParser
      class << self
        # Parse a YAML template file and extract handler callables
        # Returns array of handler class names found in the template
        def extract_handler_callables(template_file)
          return [] unless File.exist?(template_file)

          begin
            template_data = YAML.load_file(template_file)
            extract_handlers_from_template(template_data)
          rescue StandardError => e
            TaskerCore::Logger.instance.warn("Failed to parse template #{template_file}: #{e.message}")
            []
          end
        end

        # Extract handler callables from parsed template data
        def extract_handlers_from_template(template_data)
          handlers = []

          # Add task handler if present
          if template_data['task_handler'] && template_data['task_handler']['callable']
            handlers << template_data['task_handler']['callable']
          end

          # Add step handlers
          if template_data['steps'].is_a?(Array)
            template_data['steps'].each do |step|
              handlers << step['handler']['callable'] if step['handler'] && step['handler']['callable']
            end
          end

          handlers.uniq
        end

        # Get template metadata for debugging/introspection
        def extract_template_metadata(template_file)
          return nil unless File.exist?(template_file)

          begin
            template_data = YAML.load_file(template_file)
            {
              name: template_data['name'],
              namespace_name: template_data['namespace_name'],
              version: template_data['version'],
              description: template_data['description'],
              file_path: template_file,
              handlers: extract_handlers_from_template(template_data)
            }
          rescue StandardError => e
            TaskerCore::Logger.instance.warn("Failed to extract metadata from #{template_file}: #{e.message}")
            nil
          end
        end
      end
    end

    # Main discovery coordinator that combines path and parsing logic
    class HandlerDiscovery
      class << self
        # Discover all handlers from templates in the configured template directory
        # Returns array of unique handler class names
        def discover_all_handlers(template_dir = nil)
          template_files = TemplatePath.discover_template_files(template_dir)

          handlers = template_files.flat_map do |file|
            TemplateParser.extract_handler_callables(file)
          end

          handlers.uniq.sort
        end

        # Get detailed information about discovered templates
        # Returns array of template metadata hashes
        def discover_template_metadata(template_dir = nil)
          template_files = TemplatePath.discover_template_files(template_dir)

          template_files.filter_map do |file|
            TemplateParser.extract_template_metadata(file)
          end
        end

        # Get handlers grouped by namespace
        # Returns hash of namespace => [handler_names]
        def discover_handlers_by_namespace(template_dir = nil)
          metadata = discover_template_metadata(template_dir)

          handlers_by_namespace = {}
          metadata.each do |meta|
            namespace = meta[:namespace_name] || 'default'
            handlers_by_namespace[namespace] ||= []
            handlers_by_namespace[namespace].concat(meta[:handlers])
          end

          # Remove duplicates and sort
          handlers_by_namespace.each do |namespace, handlers|
            handlers_by_namespace[namespace] = handlers.uniq.sort
          end

          handlers_by_namespace
        end
      end
    end
  end
end
