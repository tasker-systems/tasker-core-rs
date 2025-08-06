# frozen_string_literal: true

require_relative '../utils/path_resolver'
require_relative '../config/validator'

module TaskerCore
  module CLI
    # CLI diagnostics system for troubleshooting TaskerCore configuration issues
    #
    # This class provides comprehensive diagnostic capabilities including:
    # - Configuration analysis and validation
    # - Project structure verification
    # - Search path analysis
    # - Database connectivity testing
    # - Template discovery and validation
    # - Environment context analysis
    #
    # @example Run full diagnostics
    #   TaskerCore::CLI::Diagnostics.run_full_diagnostics
    #
    # @example Run specific checks
    #   TaskerCore::CLI::Diagnostics.check_config
    #   TaskerCore::CLI::Diagnostics.check_search_paths
    #
    class Diagnostics
      class << self
        # Run comprehensive diagnostics
        def run_full_diagnostics
          puts header("TaskerCore Configuration Diagnostics")

          check_environment
          check_project_structure
          check_config
          check_search_paths
          check_database_connection
          check_template_discovery
          check_boot_sequence

          puts footer
        end

        # Quick configuration check
        def run_config_check
          puts header("TaskerCore Configuration Check")

          check_environment
          check_project_structure
          check_config
          check_search_paths

          puts footer
        end

        # Environment and context analysis
        def check_environment
          puts section_header("Environment Analysis")

          env_vars = {
            'TASKER_ENV' => ENV['TASKER_ENV'],
            'RAILS_ENV' => ENV['RAILS_ENV'],
            'RUBY_ENV' => ENV['RUBY_ENV'],
            'DATABASE_URL' => ENV['DATABASE_URL'] ? '[SET]' : '[NOT SET]'
          }

          env_vars.each do |key, value|
            status = value ? "✅" : "⚠️"
            puts "  #{status} #{key}: #{value || '[NOT SET]'}"
          end

          puts "  📁 Working directory: #{Dir.pwd}"
          puts "  🗂️  Relative to project: #{Utils::PathResolver.relative_path_from_root(Dir.pwd)}"
          puts "  🐛 Development mode: #{Utils::PathResolver.development_mode? ? 'Yes' : 'No'}"
          puts
        end

        # Project structure verification
        def check_project_structure
          puts section_header("Project Structure")

          structure = Utils::PathResolver.project_structure_summary

          puts "  📂 Project root: #{structure[:project_root]}"
          puts "    #{status_icon(structure[:exists])} Directory exists"
          puts "    #{status_icon(structure[:cargo_toml])} Cargo.toml present"
          puts "    #{status_icon(structure[:config_dir])} config/ directory"
          puts "    #{status_icon(structure[:bindings_dir])} bindings/ directory"
          puts "    #{status_icon(structure[:ruby_bindings])} Ruby bindings"

          if structure[:working_directory] != structure[:project_root]
            puts "  ℹ️  Running from: #{structure[:relative_working_dir]}"
          end
          puts
        end

        # Configuration analysis
        def check_config
          puts section_header("Configuration Analysis")

          begin
            config = TaskerCore::Config.instance
            puts "  ✅ Configuration loaded successfully"
            puts "    🌍 Environment: #{config.environment}"
            puts "    📄 Config file: #{config.find_config_file}"
            puts "    #{status_icon(File.exist?(config.find_config_file))} Config file exists"

            # Validate configuration
            validator = TaskerCore::ConfigValidation::Validator.new(config)
            summary = validator.validation_summary

            if summary[:errors] == 0
              puts "  ✅ Configuration validation passed"
            else
              puts "  ❌ Configuration validation failed (#{summary[:errors]} errors)"
              summary[:error_messages].each { |error| puts "    • #{error}" }
            end

            if summary[:warnings] > 0
              puts "  ⚠️  Configuration warnings (#{summary[:warnings]}):"
              summary[:warning_messages].each { |warning| puts "    • #{warning}" }
            end

          rescue => e
            puts "  ❌ Configuration loading failed: #{e.message}"
            puts "    #{e.backtrace.first}" if Utils::PathResolver.development_mode?
          end
          puts
        end

        # Search path analysis
        def check_search_paths
          puts section_header("Search Path Analysis")

          begin
            config = TaskerCore::Config.instance
            paths = config.task_template_search_paths

            puts "  📋 Configured search paths (#{paths.length}):"

            if paths.empty?
              puts "    ❌ No search paths configured"
              return
            end

            total_files = 0
            paths.each_with_index do |path, index|
              files = Dir.glob(path)
              total_files += files.length

              status = files.any? ? "✅" : "⚠️"
              puts "    #{index + 1}. #{status} #{Utils::PathResolver.relative_path_from_root(path)}"
              puts "       Absolute: #{path}"
              puts "       Files found: #{files.length}"

              if files.any?
                files.first(3).each do |file|
                  rel_file = Utils::PathResolver.relative_path_from_root(file)
                  puts "         - #{rel_file}"
                end
                puts "         ... and #{files.length - 3} more" if files.length > 3
              end
            end

            puts "  📊 Summary: #{total_files} total files in #{paths.length} search paths"

          rescue => e
            puts "  ❌ Search path analysis failed: #{e.message}"
          end
          puts
        end

        # Database connectivity testing
        def check_database_connection
          puts section_header("Database Connection")

          begin
            # Try to establish connection
            require_relative '../database/connection'
            TaskerCore::Database::Connection.establish!

            puts "  ✅ Database connection established"

            # Test basic queries
            if defined?(ActiveRecord) && ActiveRecord::Base.connected?
              result = ActiveRecord::Base.connection.execute("SELECT version()")
              version = result.first['version'] if result.respond_to?(:first)
              puts "  ✅ PostgreSQL version: #{version&.split&.first(2)&.join(' ')}"

              # Check for required tables
              tables_to_check = %w[
                tasker_task_namespaces
                tasker_named_tasks
                tasker_tasks
                tasker_workflow_steps
              ]

              puts "  📋 Table status:"
              tables_to_check.each do |table|
                exists = ActiveRecord::Base.connection.table_exists?(table)
                puts "    #{status_icon(exists)} #{table}"
              end

            else
              puts "  ⚠️  ActiveRecord not connected"
            end

          rescue => e
            puts "  ❌ Database connection failed: #{e.message}"
            puts "    Check DATABASE_URL and database server status"
          end
          puts
        end

        # Template discovery and validation
        def check_template_discovery
          puts section_header("Template Discovery")

          begin
            config = TaskerCore::Config.instance
            paths = config.task_template_search_paths

            all_files = paths.flat_map { |pattern| Dir.glob(pattern) }

            if all_files.empty?
              puts "  ⚠️  No template files discovered"
              return
            end

            puts "  📋 Template file analysis (#{all_files.length} files):"

            valid_files = 0
            invalid_files = []

            all_files.each do |file_path|
              rel_path = Utils::PathResolver.relative_path_from_root(file_path)

              begin
                yaml_data = YAML.load_file(file_path)

                if yaml_data.is_a?(Hash)
                  name = yaml_data['name'] || '[unnamed]'
                  namespace = yaml_data['namespace'] || '[no namespace]'
                  version = yaml_data['version'] || '[no version]'

                  puts "    ✅ #{rel_path}"
                  puts "       #{namespace}/#{name}:#{version}"
                  valid_files += 1
                else
                  puts "    ❌ #{rel_path}: Invalid YAML structure"
                  invalid_files << file_path
                end

              rescue Psych::SyntaxError => e
                puts "    ❌ #{rel_path}: YAML syntax error"
                puts "       #{e.message}"
                invalid_files << file_path
              rescue => e
                puts "    ❌ #{rel_path}: #{e.class}"
                invalid_files << file_path
              end
            end

            puts "  📊 Summary: #{valid_files} valid, #{invalid_files.length} invalid"

          rescue => e
            puts "  ❌ Template discovery failed: #{e.message}"
          end
          puts
        end

        # Boot sequence simulation
        def check_boot_sequence
          puts section_header("Boot Sequence Test")

          begin
            puts "  🚀 Testing TaskerCore boot sequence..."

            # Reset any existing state
            if defined?(TaskerCore::Boot)
              puts "    🔄 Resetting boot state"
              # TaskerCore::Boot.reset! if TaskerCore::Boot.respond_to?(:reset!)
            end

            # Try configuration loading
            config = TaskerCore::Config.instance
            puts "    ✅ Configuration loaded"

            # Try database connection
            require_relative '../database/connection'
            TaskerCore::Database::Connection.establish!
            puts "    ✅ Database connection established"

            # Try model loading
            if File.exist?(Utils::PathResolver.resolve_config_path('bindings/ruby/lib/tasker_core/database/models.rb'))
              require_relative '../database/models'
              puts "    ✅ Database models loaded"
            end

            puts "  ✅ Boot sequence test completed successfully"

          rescue => e
            puts "  ❌ Boot sequence test failed: #{e.message}"
            puts "    #{e.backtrace.first}" if Utils::PathResolver.development_mode?
          end
          puts
        end

        # Generate configuration documentation
        def generate_config_docs
          puts header("TaskerCore Configuration Documentation")

          config = TaskerCore::Config.instance
          structure = Utils::PathResolver.project_structure_summary

          puts "## Project Information"
          puts "- **Project Root**: `#{structure[:project_root]}`"
          puts "- **Environment**: `#{config.environment}`"
          puts "- **Working Directory**: `#{Dir.pwd}`"
          puts "- **Development Mode**: #{Utils::PathResolver.development_mode?}"
          puts

          puts "## Search Paths"
          paths = config.task_template_search_paths
          paths.each do |path|
            rel_path = Utils::PathResolver.relative_path_from_root(path)
            files = Dir.glob(path)
            puts "- `#{rel_path}` (#{files.length} files)"
          end
          puts

          puts "## Available Templates"
          all_files = paths.flat_map { |pattern| Dir.glob(pattern) }
          if all_files.any?
            all_files.each do |file_path|
              begin
                yaml_data = YAML.load_file(file_path)
                name = yaml_data['name'] || '[unnamed]'
                namespace = yaml_data['namespace'] || '[no namespace]'
                version = yaml_data['version'] || '[no version]'
                rel_path = Utils::PathResolver.relative_path_from_root(file_path)
                puts "- **#{namespace}/#{name}:#{version}** (`#{rel_path}`)"
              rescue
                # Skip invalid files
              end
            end
          else
            puts "- No templates found"
          end
          puts

          puts footer
        end

        private

        def header(title)
          <<~HEADER
            ╔═══════════════════════════════════════════════════════════════════════════════╗
            ║ #{title.center(77)} ║
            ╚═══════════════════════════════════════════════════════════════════════════════╝

          HEADER
        end

        def footer
          <<~FOOTER

            ╔═══════════════════════════════════════════════════════════════════════════════╗
            ║ Need help? Check the documentation or run specific diagnostics:              ║
            ║                                                                               ║
            ║   bundle exec tasker-core diagnose config    # Configuration only           ║
            ║   bundle exec tasker-core diagnose docs      # Generate documentation       ║
            ║                                                                               ║
            ╚═══════════════════════════════════════════════════════════════════════════════╝
          FOOTER
        end

        def section_header(title)
          "┌─ #{title} " + "─" * (75 - title.length) + "┐"
        end

        def status_icon(condition)
          condition ? "✅" : "❌"
        end
      end
    end
  end
end
