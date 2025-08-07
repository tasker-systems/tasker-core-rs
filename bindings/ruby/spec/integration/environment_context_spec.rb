# frozen_string_literal: true

require 'spec_helper'
require 'support/integration_helpers'

RSpec.describe 'Environment Context Integration', type: :integration do
  include IntegrationHelpers

  let(:project_root) { TaskerCore::Utils::PathResolver.project_root }

  describe 'path resolution consistency' do
    it 'finds the correct project root from any working directory' do
      test_directories = [
        '.',                          # Project root
        'bindings/ruby',              # Ruby bindings directory
        'src',                        # Rust source directory
        'config',                     # Config directory
        'bindings/ruby/spec'          # Test directory
      ]

      test_directories.each do |rel_dir|
        test_dir = File.join(project_root, rel_dir)
        next unless File.directory?(test_dir)

        original_pwd = Dir.pwd
        begin
          Dir.chdir(test_dir)
          TaskerCore::Utils::PathResolver.reset!

          detected_root = TaskerCore::Utils::PathResolver.project_root
          expect(detected_root).to eq(project_root),
                                   "Project root detection failed from #{rel_dir}. Expected: #{project_root}, Got: #{detected_root}"
        ensure
          Dir.chdir(original_pwd)
          TaskerCore::Utils::PathResolver.reset!
        end
      end
    end

    it 'resolves template search paths consistently from different directories' do
      with_clean_tasker_environment do
        # Get baseline from project root
        config = TaskerCore::Config.instance
        baseline_paths = config.task_template_search_paths
        baseline_files = baseline_paths.flat_map { |pattern| Dir.glob(pattern) }

        expect(baseline_files).not_to be_empty, 'No template files found from project root'

        # Test from different directories
        test_directories = ['bindings/ruby', 'src', 'config']

        test_directories.each do |rel_dir|
          test_dir = File.join(project_root, rel_dir)
          next unless File.directory?(test_dir)

          original_pwd = Dir.pwd
          begin
            Dir.chdir(test_dir)
            TaskerCore::Utils::PathResolver.reset!

            # Reset config to pick up new working directory context
            TaskerCore::Config.instance_variable_set(:@instance, nil)

            test_config = TaskerCore::Config.instance
            test_paths = test_config.task_template_search_paths
            test_files = test_paths.flat_map { |pattern| Dir.glob(pattern) }

            expect(test_files.sort).to eq(baseline_files.sort),
                                       "Template files differ when running from #{rel_dir}"

            expect(test_files).not_to be_empty,
                                      "No template files found when running from #{rel_dir}"
          ensure
            Dir.chdir(original_pwd)
            TaskerCore::Utils::PathResolver.reset!
            TaskerCore::Config.instance_variable_set(:@instance, nil)
          end
        end
      end
    end

    it 'validates configuration consistently across directories' do
      test_directories = ['bindings/ruby', 'config']

      test_directories.each do |rel_dir|
        test_dir = File.join(project_root, rel_dir)
        next unless File.directory?(test_dir)

        with_clean_tasker_environment do
          original_pwd = Dir.pwd
          begin
            Dir.chdir(test_dir)

            config = TaskerCore::Config.instance
            validator = TaskerCore::ConfigValidation::Validator.new(config)

            expect { validator.validate! }.not_to raise_error,
                                                  "Configuration validation failed when running from #{rel_dir}"

            summary = validator.validation_summary
            expect(summary[:errors]).to eq(0),
                                        "Configuration errors found when running from #{rel_dir}: #{summary[:error_messages]}"
          ensure
            Dir.chdir(original_pwd)
          end
        end
      end
    end
  end

  describe 'template discovery robustness' do
    it 'discovers all expected template files regardless of working directory' do
      expected_templates = %w[
        diamond_workflow
        linear_workflow
        mixed_dag_workflow
        order_fulfillment
        tree_workflow
      ]

      test_directories = ['.', 'bindings/ruby', 'src']

      test_directories.each do |rel_dir|
        test_dir = File.join(project_root, rel_dir)
        next unless File.directory?(test_dir)

        with_clean_tasker_environment do
          original_pwd = Dir.pwd
          begin
            Dir.chdir(test_dir)

            template_results = verify_template_loading

            expect(template_results[:total_files]).to be > 0,
                                                      "No template files discovered from #{rel_dir}"

            expect(template_results[:valid_files]).to eq(template_results[:total_files]),
                                                      "Invalid template files found from #{rel_dir}: #{template_results[:invalid_files]}"

            discovered_namespaces = template_results[:templates].map { |t| t[:namespace] }.uniq
            missing_namespaces = expected_templates - discovered_namespaces

            expect(missing_namespaces).to be_empty,
                                          "Missing expected template namespaces when running from #{rel_dir}: #{missing_namespaces}"
          ensure
            Dir.chdir(original_pwd)
          end
        end
      end
    end

    it 'handles path resolution errors gracefully' do
      with_clean_tasker_environment do
        # Test with an invalid working directory context
        temp_dir = Dir.mktmpdir
        begin
          Dir.chdir(temp_dir)
          TaskerCore::Utils::PathResolver.reset!

          # Should fall back gracefully
          project_root_detected = TaskerCore::Utils::PathResolver.project_root
          expect(project_root_detected).to be_a(String)
          expect(File.directory?(project_root_detected)).to be(true)
        ensure
          Dir.chdir(project_root)
          FileUtils.remove_entry(temp_dir)
          TaskerCore::Utils::PathResolver.reset!
        end
      end
    end
  end

  describe 'database connectivity' do
    it 'maintains database connection regardless of working directory' do
      skip 'Database not available' unless database_connected?

      test_directories = ['.', 'bindings/ruby']

      test_directories.each do |rel_dir|
        test_dir = File.join(project_root, rel_dir)
        next unless File.directory?(test_dir)

        with_clean_tasker_environment do
          original_pwd = Dir.pwd
          begin
            Dir.chdir(test_dir)

            # Establish database connection
            require 'tasker_core/database/connection'
            TaskerCore::Database::Connection.establish!

            expect(database_connected?).to be(true),
                                           "Database connection failed when running from #{rel_dir}"

            # Test basic database operations
            db_summary = database_state_summary
            expect(db_summary[:connected]).to be(true),
                                              "Database state check failed from #{rel_dir}"
          ensure
            Dir.chdir(original_pwd)
          end
        end
      end
    end
  end

  describe 'boot sequence reliability' do
    it 'completes boot sequence successfully from different directories' do
      test_directories = ['.', 'bindings/ruby']

      test_directories.each do |rel_dir|
        test_dir = File.join(project_root, rel_dir)
        next unless File.directory?(test_dir)

        with_clean_tasker_environment do
          original_pwd = Dir.pwd
          begin
            Dir.chdir(test_dir)

            # Simulate boot sequence steps
            expect do
              config = TaskerCore::Config.instance
              expect(config.environment).to eq('test')

              paths = config.task_template_search_paths
              expect(paths).not_to be_empty

              total_files = paths.sum { |pattern| Dir.glob(pattern).count }
              expect(total_files).to be > 0
            end.not_to raise_error, "Boot sequence failed from #{rel_dir}"
          ensure
            Dir.chdir(original_pwd)
          end
        end
      end
    end

    it 'provides consistent diagnostic information across directories' do
      test_directories = ['.', 'bindings/ruby']
      diagnostic_results = {}

      test_directories.each do |rel_dir|
        test_dir = File.join(project_root, rel_dir)
        next unless File.directory?(test_dir)

        with_clean_tasker_environment do
          original_pwd = Dir.pwd
          begin
            Dir.chdir(test_dir)

            # Capture diagnostic information
            config = TaskerCore::Config.instance
            validator = TaskerCore::ConfigValidation::Validator.new(config)

            diagnostic_results[rel_dir] = {
              environment: config.environment,
              search_paths: config.task_template_search_paths.length,
              template_files: config.task_template_search_paths.sum { |p| Dir.glob(p).count },
              validation_errors: validator.validation_summary[:errors],
              project_structure: TaskerCore::Utils::PathResolver.project_structure_summary
            }
          ensure
            Dir.chdir(original_pwd)
          end
        end
      end

      # Compare results between directories
      if diagnostic_results.keys.length > 1
        baseline = diagnostic_results.values.first
        diagnostic_results.each do |dir, results|
          expect(results[:environment]).to eq(baseline[:environment]),
                                           "Environment differs in #{dir}"
          expect(results[:search_paths]).to eq(baseline[:search_paths]),
                                            "Search path count differs in #{dir}"
          expect(results[:template_files]).to eq(baseline[:template_files]),
                                              "Template file count differs in #{dir}"
          expect(results[:validation_errors]).to eq(baseline[:validation_errors]),
                                                 "Validation error count differs in #{dir}"
        end
      end
    end
  end

  describe 'error handling and recovery' do
    it 'provides helpful error messages when configuration is missing' do
      with_clean_tasker_environment do
        # Test with missing config file
        ENV['TASKER_CONFIG_FILE'] = '/nonexistent/config.yaml'

        TaskerCore::Utils::PathResolver.reset!
        TaskerCore::Config.instance_variable_set(:@instance, nil)

        config = TaskerCore::Config.instance
        validator = TaskerCore::ConfigValidation::Validator.new(config)

        # Should not crash, but should provide helpful information
        summary = validator.validation_summary
        expect(summary).to be_a(Hash)
        expect(summary).to have_key(:errors)
        expect(summary).to have_key(:warnings)
      end
    end

    it 'recovers gracefully from path resolution failures' do
      with_clean_tasker_environment do
        # Temporarily break path resolution
        original_method = TaskerCore::Utils::PathResolver.method(:find_project_root)

        TaskerCore::Utils::PathResolver.define_singleton_method(:find_project_root) do
          raise 'Simulated path resolution failure'
        end

        begin
          TaskerCore::Utils::PathResolver.reset!

          # Should fall back gracefully
          expect do
            root = TaskerCore::Utils::PathResolver.project_root
            expect(root).to be_a(String)
          end.not_to raise_error
        ensure
          # Restore original method
          TaskerCore::Utils::PathResolver.define_singleton_method(:find_project_root, original_method)
          TaskerCore::Utils::PathResolver.reset!
        end
      end
    end
  end

  private

  def database_connected?
    return false unless defined?(ActiveRecord)

    ActiveRecord::Base.connected? && ActiveRecord::Base.connection.active?
  rescue StandardError
    false
  end
end
