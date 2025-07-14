# frozen_string_literal: true

require 'json'

module TaskerCore
  # Test helpers for creating test data using Rust factory patterns
  #
  # Since TaskerCore is a standalone gem without Rails transactional specs,
  # these helpers provide:
  # 1. Access to Rust factory patterns for creating test data
  # 2. Database cleanup utilities
  # 3. Convenient Ruby APIs for common test scenarios
  #
  # ## Usage
  #
  # ```ruby
  # include TaskerCore::TestHelpers
  #
  # describe 'my workflow' do
  #   before(:each) { setup_test_database }
  #   after(:each) { cleanup_test_database }
  #
  #   it 'processes tasks' do
  #     task = create_test_task(name: 'test_task', context: { test: true })
  #     step = create_test_workflow_step(task_id: task.id, name: 'test_step')
  #
  #     # Your test logic here
  #   end
  # end
  # ```
  module TestHelpers
    # Database connection and transaction management
    
    def setup_test_database
      # Run migrations to ensure schema is up to date
      run_migrations
      # Initialize foundation data if needed
      create_test_foundation
    end

    def cleanup_test_database
      # Call Rust database cleanup through FFI wrapper
      result = TaskerCore::TestHelpers.cleanup_test_database
      result.is_a?(Hash) ? result : {}
    end

    def run_migrations
      # Run all migrations using Rust DatabaseMigrations
      database_url = ENV['DATABASE_URL'] || 'postgresql://tasker:tasker@localhost/tasker_rust_test'
      result = TaskerCore::TestHelpers.run_migrations(database_url)
      
      if result.is_a?(Hash) && result['status'] == 'error'
        raise "Migration failed: #{result['error']}"
      end
      
      result.is_a?(Hash) ? result : {}
    end

    def drop_schema
      # Drop and recreate schema using Rust DatabaseMigrations
      database_url = ENV['DATABASE_URL'] || 'postgresql://tasker:tasker@localhost/tasker_rust_test'
      result = TaskerCore::TestHelpers.drop_schema(database_url)
      
      if result.is_a?(Hash) && result['status'] == 'error'
        raise "Schema drop failed: #{result['error']}"
      end
      
      result.is_a?(Hash) ? result : {}
    end

    def reset_test_database
      # Complete database reset: drop schema + run migrations
      drop_schema
      run_migrations
    end

    # Factory methods that delegate to Rust factories
    
    # Create a test task using Rust TaskFactory
    #
    # @param [Hash] options Task creation options
    # @option options [String] :name Task name (defaults to test task)
    # @option options [Hash] :context Task context data
    # @option options [Array<String>] :tags Task tags
    # @option options [String] :initiator Who initiated the task
    # @option options [String] :reason Reason for task creation
    # @return [Hash] Task data
    def create_test_task(options = {})
      # Add database URL from environment if not provided
      options_with_db = options.merge(
        'database_url' => ENV['DATABASE_URL'] || 'postgresql://tasker:tasker@localhost/tasker_rust_test'
      )
      
      # Call Rust TaskFactory through FFI wrapper
      result = TaskerCore::TestHelpers.create_test_task_with_factory(options_with_db)
      result.is_a?(Hash) ? result : {}
    end

    # Create a test workflow step using Rust WorkflowStepFactory
    #
    # @param [Hash] options Step creation options
    # @option options [Integer] :task_id Associated task ID
    # @option options [String] :name Step name
    # @option options [String] :state Initial state (defaults to 'pending')
    # @return [Hash] Workflow step data
    def create_test_workflow_step(options = {})
      # Add database URL from environment if not provided
      options_with_db = options.merge(
        'database_url' => ENV['DATABASE_URL'] || 'postgresql://tasker:tasker@localhost/tasker_rust_test'
      )
      
      # Call Rust WorkflowStepFactory through FFI wrapper
      result = TaskerCore::TestHelpers.create_test_workflow_step_with_factory(options_with_db)
      result.is_a?(Hash) ? result : {}
    end

    # Create a test workflow with dependencies
    #
    # @param [Symbol] :type Workflow type (:linear, :diamond, :parallel, :tree)
    # @param [Hash] options Workflow creation options
    # @return [Hash] Workflow data with task and steps
    def create_test_workflow(type, options = {})
      case type
      when :linear
        create_linear_workflow(options)
      when :diamond
        create_diamond_workflow(options)
      when :parallel
        create_parallel_workflow(options)
      when :tree
        create_tree_workflow(options)
      else
        raise ArgumentError, "Unknown workflow type: #{type}"
      end
    end

    # Create a basic foundation (NamedTask, NamedStep, etc.)
    #
    # @param [Hash] options Foundation creation options
    # @return [Hash] Foundation data
    def create_test_foundation(options = {})
      # Add database URL from environment if not provided
      options_with_db = options.merge(
        'database_url' => ENV['DATABASE_URL'] || 'postgresql://tasker:tasker@localhost/tasker_rust_test'
      )
      
      # Call Rust StandardFoundation through FFI wrapper
      result = TaskerCore::TestHelpers.create_test_foundation_with_factory(options_with_db)
      result.is_a?(Hash) ? result : {}
    end

    private

    def create_linear_workflow(options)
      # Create A→B→C→D workflow using Rust factories
      task = create_test_task(options.merge(name: 'linear_workflow'))
      
      {
        'task' => task,
        'steps' => [
          create_test_workflow_step(task_id: task['id'], name: 'step_a'),
          create_test_workflow_step(task_id: task['id'], name: 'step_b'),
          create_test_workflow_step(task_id: task['id'], name: 'step_c'),
          create_test_workflow_step(task_id: task['id'], name: 'step_d')
        ],
        'type' => 'linear'
      }
    end

    def create_diamond_workflow(options)
      # Create A→(B,C)→D workflow using Rust factories
      task = create_test_task(options.merge(name: 'diamond_workflow'))
      
      {
        'task' => task,
        'steps' => [
          create_test_workflow_step(task_id: task['id'], name: 'setup'),
          create_test_workflow_step(task_id: task['id'], name: 'process_a'),
          create_test_workflow_step(task_id: task['id'], name: 'process_b'),
          create_test_workflow_step(task_id: task['id'], name: 'finalize')
        ],
        'type' => 'diamond'
      }
    end

    def create_parallel_workflow(options)
      # Create parallel workflow using Rust factories
      task = create_test_task(options.merge(name: 'parallel_workflow'))
      
      {
        'task' => task,
        'steps' => [
          create_test_workflow_step(task_id: task['id'], name: 'parallel_1'),
          create_test_workflow_step(task_id: task['id'], name: 'parallel_2'),
          create_test_workflow_step(task_id: task['id'], name: 'parallel_3')
        ],
        'type' => 'parallel'
      }
    end

    def create_tree_workflow(options)
      # Create tree workflow using Rust factories
      task = create_test_task(options.merge(name: 'tree_workflow'))
      
      {
        'task' => task,
        'steps' => [
          create_test_workflow_step(task_id: task['id'], name: 'root'),
          create_test_workflow_step(task_id: task['id'], name: 'branch_a'),
          create_test_workflow_step(task_id: task['id'], name: 'branch_b'),
          create_test_workflow_step(task_id: task['id'], name: 'leaf_1'),
          create_test_workflow_step(task_id: task['id'], name: 'leaf_2')
        ],
        'type' => 'tree'
      }
    end
  end
end