# frozen_string_literal: true

require 'tasker_core/utils/task_template_migrator'
require 'tasker_core/utils/task_template_migrator_cli'

namespace :tasker_core do
  namespace :migrate do
    desc "Migrate legacy HandlerConfiguration YAML files to TaskTemplate format"
    task :templates do
      puts "🔄 TaskTemplate Migration Tool"
      puts "=" * 50
      
      # Check for common configuration directories
      config_directories = [
        'config/tasker/tasks',
        'config/tasks', 
        'spec/handlers/examples',
        'lib/tasks',
        'app/tasker'
      ].select { |dir| Dir.exist?(dir) }
      
      if config_directories.empty?
        puts "❌ No common configuration directories found."
        puts "   Looked for: config/tasker/tasks, config/tasks, spec/handlers/examples, lib/tasks, app/tasker"
        puts ""
        puts "📋 Usage:"
        puts "   rake tasker_core:migrate:file[path/to/file.yaml]"
        puts "   rake tasker_core:migrate:directory[path/to/directory]"
        exit 1
      end
      
      puts "📂 Found configuration directories:"
      config_directories.each { |dir| puts "   - #{dir}" }
      puts ""
      
      migrator = TaskerCore::Utils::TaskTemplateMigrator.new
      total_files = 0
      total_migrated = 0
      
      config_directories.each do |directory|
        puts "🔍 Scanning: #{directory}"
        results = migrator.migrate_directory(directory)
        
        total_files += results[:files_processed]
        total_migrated += results[:files_migrated]
        
        if results[:files_processed] > 0
          puts "   ✅ Processed: #{results[:files_processed]}, Migrated: #{results[:files_migrated]}"
        else
          puts "   📄 No YAML files found"
        end
      end
      
      puts ""
      puts "📊 Overall Results:"
      puts "   - Total files processed: #{total_files}"
      puts "   - Total files migrated: #{total_migrated}"
      
      if total_migrated > 0
        puts ""
        puts "🎉 Migration completed! Next steps:"
        puts "   1. Review .migrated.yaml files"
        puts "   2. Test with your handlers"
        puts "   3. Remove originals when satisfied"
        puts "   4. Rename .migrated.yaml → .yaml"
      end
    end
    
    desc "Migrate a single YAML file (usage: rake tasker_core:migrate:file[path/to/file.yaml])"
    task :file, [:file_path] do |_t, args|
      unless args.file_path
        puts "❌ Error: File path required"
        puts "   Usage: rake tasker_core:migrate:file[path/to/file.yaml]"
        exit 1
      end
      
      TaskerCore::Utils::TaskTemplateMigratorCLI.run(['migrate-file', args.file_path])
    end
    
    desc "Migrate all YAML files in a directory (usage: rake tasker_core:migrate:directory[path/to/directory])"
    task :directory, [:directory_path] do |_t, args|
      unless args.directory_path
        puts "❌ Error: Directory path required"
        puts "   Usage: rake tasker_core:migrate:directory[path/to/directory]"
        exit 1
      end
      
      TaskerCore::Utils::TaskTemplateMigratorCLI.run(['migrate-directory', args.directory_path])
    end
    
    desc "Show migration tool help"
    task :help do
      TaskerCore::Utils::TaskTemplateMigratorCLI.run(['help'])
    end
    
    desc "Dry run migration to see what would be changed (usage: rake tasker_core:migrate:dry_run[directory])"
    task :dry_run, [:directory_path] do |_t, args|
      directory = args.directory_path || 'config'
      
      unless Dir.exist?(directory)
        puts "❌ Error: Directory not found: #{directory}"
        exit 1
      end
      
      TaskerCore::Utils::TaskTemplateMigratorCLI.run(['--dry-run', 'migrate-directory', directory])
    end
  end
end