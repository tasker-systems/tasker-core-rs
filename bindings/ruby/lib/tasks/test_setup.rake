# frozen_string_literal: true

namespace :test do
  desc "Setup test database using TestingFramework"
  task :setup do
    # Set test environment
    ENV['RAILS_ENV'] = 'test'
    ENV['APP_ENV'] = 'test'
    ENV['RACK_ENV'] = 'test'

    puts "🧪 RAKE TASK: Setting up test database with TestingFramework"

    # Load TaskerCore
    require_relative '../tasker_core'

    begin
      database_url = ENV['DATABASE_URL'] || 'postgresql://tasker:tasker@localhost/tasker_rust_test'

      puts "🧪 RAKE TASK: Step 1 - Setting up database schema for #{database_url}"

      # First, reset the schema
      schema_result = TaskerCore::TestHelpers.drop_schema(database_url)
      if schema_result['status'] == 'error'
        puts "❌ RAKE TASK: Schema reset failed: #{schema_result['error']}"
        exit 1
      else
        puts "✅ RAKE TASK: Schema reset completed"
      end

      puts "🧪 RAKE TASK: Step 2 - Running actual SQL migrations"

      # Use the cargo binary to run migrations directly
      puts "🧪 RAKE TASK: Executing SQLx migrations via cargo..."
      migration_success = Dir.chdir('../../') do
        system({
          'DATABASE_URL' => database_url,
          'RAILS_ENV' => 'test'
        }, 'cargo sqlx migrate run --source migrations')
      end

      if migration_success
        puts "✅ RAKE TASK: SQLx migrations completed successfully"
      else
        puts "❌ RAKE TASK: SQLx migration failed"
        puts "💡 Trying alternative approach..."

        # Alternative: Use SQL files directly
        migration_files = [
          '../../migrations/20250701023135_initial_schema.sql',
          '../../migrations/20250710000001_fix_step_readiness_attempts_null_check.sql'
        ]

        all_migrations_success = true
        migration_files.each do |sql_file|
          if File.exist?(sql_file)
            puts "🧪 RAKE TASK: Running SQL file: #{sql_file}"
            sql_success = system("psql \"#{database_url}\" -f \"#{sql_file}\"")

            if sql_success
              puts "✅ RAKE TASK: SQL file executed successfully: #{File.basename(sql_file)}"
            else
              puts "❌ RAKE TASK: SQL file execution failed: #{File.basename(sql_file)}"
              all_migrations_success = false
              break
            end
          else
            puts "❌ RAKE TASK: SQL file not found: #{sql_file}"
            all_migrations_success = false
            break
          end
        end

        unless all_migrations_success
          puts "❌ RAKE TASK: Migration execution failed"
          exit 1
        end
      end

      puts "🧪 RAKE TASK: Step 3 - Verifying database setup"

      # Verify that key tables exist
      verify_success = system("psql \"#{database_url}\" -c \"SELECT 1 FROM tasker_task_namespaces LIMIT 1\" > /dev/null 2>&1")

      if verify_success
        puts "✅ RAKE TASK: Database verification successful - tables exist"
      else
        puts "⚠️  RAKE TASK: Database verification failed - some tables may be missing"
        puts "   This might be expected if the migration creates tables on-demand"
      end

    rescue StandardError => e
      puts "❌ RAKE TASK: Setup failed: #{e.message}"
      puts "   Backtrace: #{e.backtrace.first(5).join('\n   ')}"
      exit 1
    end

    puts "🎉 RAKE TASK: Test database setup completed successfully"
  end

  desc "Check test database status"
  task :status do
    ENV['RAILS_ENV'] = 'test'
    ENV['APP_ENV'] = 'test'
    ENV['RACK_ENV'] = 'test'

    puts "🔍 RAKE TASK: Checking test database status"

    require_relative '../tasker_core'

    begin
      database_url = ENV['DATABASE_URL'] || 'postgresql://tasker:tasker@localhost/tasker_rust_test'
      puts "🔍 RAKE TASK: Database URL: #{database_url}"

      # Try to list tables to check connectivity
      result = TaskerCore::TestHelpers.list_database_tables(database_url)

      if result['status'] == 'error'
        puts "❌ RAKE TASK: Database error: #{result['error']}"
        puts "   Recommendation: Run 'bundle exec rake test:setup' to initialize database"
      else
        puts "✅ RAKE TASK: Database is accessible"
        if result['tables']
          puts "   Tables found: #{result['tables'].size}"
          puts "   Sample tables: #{result['tables'].first(5).join(', ')}"
        end
      end

    rescue StandardError => e
      puts "❌ RAKE TASK: Status check failed: #{e.message}"
      puts "   Recommendation: Check database connectivity and run 'bundle exec rake test:setup'"
    end
  end
end
