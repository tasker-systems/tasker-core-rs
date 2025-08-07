# frozen_string_literal: true

require_relative '../../lib/tasker_core/utils/path_resolver'

module IntegrationHelpers
  # Clean test environment setup and teardown
  #
  # This module provides helpers to ensure integration tests run in a consistent
  # environment regardless of working directory or previous test state.
  #
  # @example Basic usage in RSpec
  #   RSpec.describe "Integration Test" do
  #     include IntegrationHelpers
  #
  #     it "works from any directory" do
  #       with_clean_tasker_environment do
  #         # Test code here runs in clean environment
  #       end
  #     end
  #   end

  # Set up a clean TaskerCore environment for testing
  #
  # This method:
  # - Preserves and restores original environment variables
  # - Preserves and restores working directory
  # - Resets TaskerCore internal state
  # - Sets up consistent test environment variables
  # - Ensures tests run from project root context
  #
  # @yield Block to execute in clean environment
  def with_clean_tasker_environment
    original_env = preserve_environment
    original_pwd = Dir.pwd
    original_state = preserve_tasker_state

    begin
      # Reset TaskerCore state
      reset_tasker_state

      # Set up test environment
      setup_test_environment

      # Change to project root for consistent path resolution
      Dir.chdir(TaskerCore::Utils::PathResolver.project_root)

      yield
    ensure
      restore_environment(original_env)
      restore_tasker_state(original_state)
      Dir.chdir(original_pwd)
    end
  end

  # Test path resolution from different working directories
  #
  # This helper runs the same test block from multiple directories to ensure
  # path resolution works consistently regardless of working directory.
  #
  # @param directories [Array<String>] Relative paths to test from
  # @yield Block to execute from each directory
  def test_from_different_directories(directories = nil, &block)
    directories ||= default_test_directories
    project_root = TaskerCore::Utils::PathResolver.project_root

    directories.each do |rel_dir|
      test_dir = File.join(project_root, rel_dir)
      next unless File.directory?(test_dir)

      context "when running from #{rel_dir}" do
        around do |example|
          original_pwd = Dir.pwd
          begin
            Dir.chdir(test_dir)
            example.run
          ensure
            Dir.chdir(original_pwd)
          end
        end

        it 'resolves paths correctly', &block
      end
    end
  end

  # Verify TaskerCore can find templates regardless of working directory
  #
  # @return [Boolean] True if templates are found from all test directories
  def verify_template_discovery_from_all_directories
    project_root = TaskerCore::Utils::PathResolver.project_root
    directories = default_test_directories

    directories.all? do |rel_dir|
      test_dir = File.join(project_root, rel_dir)
      next true unless File.directory?(test_dir)

      original_pwd = Dir.pwd
      begin
        Dir.chdir(test_dir)
        TaskerCore::Utils::PathResolver.reset!
        config = TaskerCore::Config.instance
        paths = config.task_template_search_paths
        total_files = paths.sum { |pattern| Dir.glob(pattern).count }
        total_files > 0
      ensure
        Dir.chdir(original_pwd)
        TaskerCore::Utils::PathResolver.reset!
      end
    end
  end

  # Create a temporary TaskerCore configuration for testing
  #
  # @param config_overrides [Hash] Configuration values to override
  # @return [String] Path to temporary config file
  def create_test_config(config_overrides = {})
    require 'tempfile'
    require 'yaml'

    # Default test configuration
    default_config = {
      'test' => {
        'execution' => {
          'environment' => 'test'
        },
        'database' => {
          'database' => 'tasker_rust_test',
          'host' => 'localhost',
          'username' => 'tasker',
          'password' => 'tasker'
        },
        'task_templates' => {
          'search_paths' => [
            'bindings/ruby/spec/handlers/examples/**/config/*.{yml,yaml}'
          ]
        }
      }
    }

    # Merge overrides
    config = deep_merge(default_config, config_overrides)

    # Create temporary file
    temp_file = Tempfile.new(['tasker-config-test', '.yaml'])
    temp_file.write(config.to_yaml)
    temp_file.close

    temp_file.path
  end

  # Wait for orchestration system to be ready
  #
  # @param timeout [Integer] Timeout in seconds
  # @return [Boolean] True if system is ready
  def wait_for_orchestration_ready(timeout: 10)
    start_time = Time.now

    while Time.now - start_time < timeout
      begin
        # Check if orchestration system is running
        # This is a placeholder - adjust based on actual orchestration API
        return true if orchestration_system_ready?
      rescue StandardError
        # Ignore errors during readiness check
      end

      sleep 0.1
    end

    false
  end

  # Check if the database contains expected test data
  #
  # @return [Hash] Summary of database state
  def database_state_summary
    return { connected: false } unless database_connected?

    {
      connected: true,
      task_namespaces: count_records('TaskerCore::Database::Models::TaskNamespace'),
      named_tasks: count_records('TaskerCore::Database::Models::NamedTask'),
      tasks: count_records('TaskerCore::Database::Models::Task'),
      workflow_steps: count_records('TaskerCore::Database::Models::TaskStep')
    }
  rescue StandardError => e
    { connected: false, error: e.message }
  end

  # Verify that TaskTemplate files can be loaded
  #
  # @return [Hash] Summary of template loading results
  def verify_template_loading
    config = TaskerCore::Config.instance
    paths = config.task_template_search_paths

    results = {
      search_paths: paths.length,
      total_files: 0,
      valid_files: 0,
      invalid_files: [],
      templates: []
    }

    all_files = paths.flat_map { |pattern| Dir.glob(pattern) }
    results[:total_files] = all_files.length

    all_files.each do |file_path|
      yaml_data = YAML.load_file(file_path)
      if yaml_data.is_a?(Hash) && yaml_data['name'] && yaml_data['namespace']
        results[:valid_files] += 1
        results[:templates] << {
          name: yaml_data['name'],
          namespace: yaml_data['namespace'],
          version: yaml_data['version'],
          file: TaskerCore::Utils::PathResolver.relative_path_from_root(file_path)
        }
      else
        results[:invalid_files] << { file: file_path, error: 'Invalid structure' }
      end
    rescue StandardError => e
      results[:invalid_files] << { file: file_path, error: e.message }
    end

    results
  end

  private

  def preserve_environment
    ENV.to_h
  end

  def restore_environment(original_env)
    ENV.clear
    ENV.update(original_env)
  end

  def preserve_tasker_state
    state = {}

    # Reset PathResolver cache
    if defined?(TaskerCore::Utils::PathResolver)
      state[:path_resolver_cache] = TaskerCore::Utils::PathResolver.instance_variable_get(:@project_root)
    end

    # Reset Config instance
    state[:config_instance] = TaskerCore::Config.instance_variable_get(:@instance) if defined?(TaskerCore::Config)

    state
  end

  def reset_tasker_state
    # Reset PathResolver cache
    TaskerCore::Utils::PathResolver.reset! if defined?(TaskerCore::Utils::PathResolver)

    # Reset Config singleton
    TaskerCore::Config.instance_variable_set(:@instance, nil) if defined?(TaskerCore::Config)

    # Reset Boot state if it exists
    return unless defined?(TaskerCore::Boot)

    TaskerCore::Boot.instance_variable_set(:@booted, false) if TaskerCore::Boot.instance_variable_defined?(:@booted)
  end

  def restore_tasker_state(original_state)
    # Restore PathResolver cache
    if defined?(TaskerCore::Utils::PathResolver) && original_state[:path_resolver_cache]
      TaskerCore::Utils::PathResolver.instance_variable_set(:@project_root, original_state[:path_resolver_cache])
    end

    # Restore Config instance
    return unless defined?(TaskerCore::Config) && original_state[:config_instance]

    TaskerCore::Config.instance_variable_set(:@instance, original_state[:config_instance])
  end

  def setup_test_environment
    ENV['TASKER_ENV'] = 'test'
    ENV['RAILS_ENV'] = 'test'
    ENV['DATABASE_URL'] ||= 'postgresql://tasker:tasker@localhost/tasker_rust_test'
    ENV['TASKER_LOG_LEVEL'] ||= 'INFO'
  end

  def default_test_directories
    [
      '.',                          # Project root
      'bindings/ruby',              # Ruby bindings directory
      'src',                        # Rust source directory
      'config',                     # Config directory
      'bindings/ruby/spec'          # Test directory
    ]
  end

  def orchestration_system_ready?
    # Placeholder - implement based on actual orchestration system API
    # This might check for:
    # - Database connectivity
    # - Required queues existence
    # - Orchestration process status
    database_connected?
  end

  def database_connected?
    return false unless defined?(ActiveRecord)

    ActiveRecord::Base.connected? && ActiveRecord::Base.connection.active?
  rescue StandardError
    false
  end

  def count_records(model_class_name)
    model_class = Object.const_get(model_class_name)
    model_class.count
  rescue NameError
    0
  rescue StandardError
    0
  end

  def deep_merge(base, override)
    base.merge(override) do |_key, base_val, override_val|
      if base_val.is_a?(Hash) && override_val.is_a?(Hash)
        deep_merge(base_val, override_val)
      else
        override_val
      end
    end
  end
end

# RSpec configuration to include helpers globally
if defined?(RSpec)
  RSpec.configure do |config|
    config.include IntegrationHelpers, type: :integration
  end
end
