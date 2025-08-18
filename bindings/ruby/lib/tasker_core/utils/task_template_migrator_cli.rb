# frozen_string_literal: true

require 'optparse'
require 'fileutils'
require_relative 'task_template_migrator'

module TaskerCore
  module Utils
    # Command-line interface for TaskTemplate migration
    #
    # Provides easy-to-use command-line access to the TaskTemplateMigrator
    # for migrating legacy HandlerConfiguration YAML files to the new
    # self-describing TaskTemplate format.
    #
    # Usage:
    #   ruby -r tasker_core -e "TaskerCore::Utils::TaskTemplateMigratorCLI.run" -- migrate-file path/to/file.yaml
    #   ruby -r tasker_core -e "TaskerCore::Utils::TaskTemplateMigratorCLI.run" -- migrate-directory path/to/configs/
    #   ruby -r tasker_core -e "TaskerCore::Utils::TaskTemplateMigratorCLI.run" -- help
    class TaskTemplateMigratorCLI
      attr_reader :migrator, :options

      def initialize
        @migrator = TaskTemplateMigrator.new
        @options = {
          pattern: '**/*.yaml',
          verbose: false,
          dry_run: false
        }
      end

      # Main entry point for CLI
      # @param args [Array<String>] Command line arguments
      def self.run(args = ARGV)
        cli = new
        cli.run(args)
      end

      # Run the CLI with provided arguments
      # @param args [Array<String>] Command line arguments
      def run(args)
        command = parse_arguments(args)
        
        case command
        when 'migrate-file'
          migrate_single_file
        when 'migrate-directory'
          migrate_directory
        when 'help', '--help', '-h'
          show_help
        else
          show_help
          exit 1
        end
      rescue StandardError => e
        puts "❌ Error: #{e.message}"
        puts "Use 'help' command for usage information"
        exit 1
      end

      # Convenience method to migrate a single file directly
      # @param file_path [String] Path to the file to migrate
      # @return [Boolean] True if migration succeeded
      def migrate_file(file_path)
        @file_or_directory = file_path
        migrate_single_file
        true
      rescue StandardError => e
        puts "❌ Failed to migrate #{file_path}: #{e.message}"
        false
      end

      private

      # Parse command line arguments
      # @param args [Array<String>] Arguments to parse
      # @return [String] The command to execute
      def parse_arguments(args)
        command = args.shift

        OptionParser.new do |opts|
          opts.banner = "Usage: task_template_migrator [command] [options]"

          opts.on('-p', '--pattern PATTERN', 'File pattern for directory migration (default: **/*.yaml)') do |pattern|
            @options[:pattern] = pattern
          end

          opts.on('-v', '--verbose', 'Enable verbose output') do
            @options[:verbose] = true
          end

          opts.on('-n', '--dry-run', 'Show what would be migrated without making changes') do
            @options[:dry_run] = true
          end

          opts.on('-h', '--help', 'Show this help message') do
            puts opts
            exit
          end
        end.parse!(args)

        @file_or_directory = args.first
        command
      end

      # Migrate a single file
      def migrate_single_file
        unless @file_or_directory
          puts "❌ Error: File path required for migrate-file command"
          exit 1
        end

        unless File.exist?(@file_or_directory)
          puts "❌ Error: File not found: #{@file_or_directory}"
          exit 1
        end

        puts "🔄 Migrating file: #{@file_or_directory}"
        puts "📋 Dry run mode - no files will be modified" if @options[:dry_run]

        if @options[:dry_run]
          show_dry_run_file(@file_or_directory)
        else
          success = @migrator.migrate_file(@file_or_directory)
          show_migration_results(success)
        end
      end

      # Migrate a directory of files
      def migrate_directory
        unless @file_or_directory
          puts "❌ Error: Directory path required for migrate-directory command"
          exit 1
        end

        unless Dir.exist?(@file_or_directory)
          puts "❌ Error: Directory not found: #{@file_or_directory}"
          exit 1
        end

        puts "🔄 Migrating directory: #{@file_or_directory}"
        puts "📂 Using pattern: #{@options[:pattern]}"
        puts "📋 Dry run mode - no files will be modified" if @options[:dry_run]

        if @options[:dry_run]
          show_dry_run_directory(@file_or_directory)
        else
          results = @migrator.migrate_directory(@file_or_directory, pattern: @options[:pattern])
          show_directory_results(results)
        end
      end

      # Show what would happen in dry run mode for a single file
      # @param file_path [String] Path to the file
      def show_dry_run_file(file_path)
        puts "\n📄 Would migrate: #{file_path}"
        
        begin
          legacy_data = YAML.load_file(file_path)
          output_path = file_path.sub(/\.yaml$/, '.migrated.yaml')
          
          puts "📤 Output file: #{output_path}"
          puts "📊 Legacy template info:"
          puts "   - Name: #{legacy_data['name']}"
          puts "   - Namespace: #{legacy_data['namespace_name']}"
          puts "   - Version: #{legacy_data['version'] || '1.0.0'}"
          puts "   - Steps: #{legacy_data['step_templates']&.size || 0}"
          puts "   - Environments: #{legacy_data['environments']&.keys&.join(', ') || 'none'}"
          
          puts "\n✅ File would be successfully migrated"
        rescue StandardError => e
          puts "\n❌ Migration would fail: #{e.message}"
        end
      end

      # Show what would happen in dry run mode for a directory
      # @param directory_path [String] Path to the directory
      def show_dry_run_directory(directory_path)
        pattern_path = File.join(directory_path, @options[:pattern])
        yaml_files = Dir.glob(pattern_path).select { |f| File.file?(f) && !f.include?('.migrated.yaml') }
        
        puts "\n📂 Found #{yaml_files.size} YAML files to process:"
        
        yaml_files.each_with_index do |file, index|
          puts "\n#{index + 1}. #{file}"
          show_dry_run_file(file)
        end

        puts "\n📊 Summary:"
        puts "   - Files to process: #{yaml_files.size}"
        puts "   - Pattern: #{@options[:pattern]}"
        puts "   - Directory: #{directory_path}"
      end

      # Show results for single file migration
      # @param success [Boolean] Whether migration succeeded
      def show_migration_results(success)
        puts "\n📊 Migration Results:"
        
        if success
          puts "✅ Migration completed successfully"
        else
          puts "❌ Migration failed"
        end

        summary = @migrator.migration_summary
        puts "   - Files processed: #{summary[:files_processed]}"
        puts "   - Files migrated: #{summary[:files_migrated]}"
        puts "   - Files failed: #{summary[:files_failed]}"
        puts "   - Success rate: #{summary[:success_rate]}%"

        if @options[:verbose] && !summary[:errors].empty?
          puts "\n❌ Errors:"
          summary[:errors].each do |error|
            puts "   - #{error[:file]}: #{error[:error]}"
          end
        end
      end

      # Show results for directory migration
      # @param results [Hash] Migration results summary
      def show_directory_results(results)
        puts "\n📊 Migration Results:"
        puts "   - Files processed: #{results[:files_processed]}"
        puts "   - Files migrated: #{results[:files_migrated]}"
        puts "   - Files failed: #{results[:files_failed]}"
        puts "   - Success rate: #{results[:success_rate]}%"

        if results[:files_migrated] > 0
          puts "\n✅ Successfully migrated files can be found with .migrated.yaml extension"
          puts "   Review the migrated files before removing the original .yaml files"
        end

        if @options[:verbose] && !results[:errors].empty?
          puts "\n❌ Errors:"
          results[:errors].each do |error|
            puts "   - #{error[:file]}: #{error[:error]}"
          end
        end

        if results[:success_rate] == 100.0
          puts "\n🎉 All files migrated successfully!"
          show_next_steps
        end
      end

      # Show next steps after successful migration
      def show_next_steps
        puts "\n📋 Next Steps:"
        puts "   1. Review the .migrated.yaml files to ensure they look correct"
        puts "   2. Test your workflow handlers with the new configuration format"
        puts "   3. Update any handler classes if needed to use new structure"
        puts "   4. When satisfied, remove the original .yaml files:"
        puts "      find . -name '*.yaml' ! -name '*.migrated.yaml' -delete"
        puts "   5. Rename .migrated.yaml files to .yaml:"
        puts "      find . -name '*.migrated.yaml' -exec sh -c 'mv \"$1\" \"${1%.migrated.yaml}.yaml\"' _ {} \\;"
      end

      # Show help information
      def show_help
        puts <<~HELP
          TaskTemplate Migration Tool
          
          Migrates legacy HandlerConfiguration YAML files to the new self-describing
          TaskTemplate format used by TAS-36.
          
          Commands:
            migrate-file <file>      Migrate a single YAML file
            migrate-directory <dir>  Migrate all YAML files in a directory
            help                     Show this help message
          
          Options:
            -p, --pattern PATTERN    File pattern for directory migration (default: **/*.yaml)
            -v, --verbose           Enable verbose output
            -n, --dry-run           Show what would be migrated without making changes
            -h, --help              Show this help message
          
          Examples:
            # Migrate a single file
            ruby -r tasker_core -e "TaskerCore::Utils::TaskTemplateMigratorCLI.run" -- migrate-file config/order_processor.yaml
            
            # Migrate all YAML files in a directory
            ruby -r tasker_core -e "TaskerCore::Utils::TaskTemplateMigratorCLI.run" -- migrate-directory config/tasks/
            
            # Dry run to see what would be migrated
            ruby -r tasker_core -e "TaskerCore::Utils::TaskTemplateMigratorCLI.run" -- --dry-run migrate-directory config/
            
            # Migrate with custom pattern
            ruby -r tasker_core -e "TaskerCore::Utils::TaskTemplateMigratorCLI.run" -- --pattern "*_handler.yaml" migrate-directory handlers/
          
          Notes:
            - Original files are preserved unchanged
            - Migrated files are saved with .migrated.yaml extension
            - Uses dry-types validation to ensure correctness
            - Review migrated files before removing originals
        HELP
      end
    end
  end
end