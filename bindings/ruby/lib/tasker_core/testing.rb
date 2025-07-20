# frozen_string_literal: true

require_relative 'internal/orchestration_manager'

module TaskerCore
  # Clean Testing Domain API
  # 
  # This provides a clean, Ruby-idiomatic API for all testing operations
  # following the proven Factory/Registry pattern with optimized FFI boundaries.
  #
  # Examples:
  #   TaskerCore::Testing.create_task(namespace: "test", name: "sample_task")
  #   TaskerCore::Testing.create_step(task_id: 123, name: "process_data")
  #   TaskerCore::Testing.setup_environment
  #   TaskerCore::Testing.cleanup
  module Testing
    class << self
      # Create a test task with primitive parameters (optimized FFI boundary)
      # @param namespace [String] Task namespace (default: "test")
      # @param name [String] Task name (default: "test_task")
      # @param version [String] Task version
      # @param context [Hash, String] Task context as hash or JSON string
      # @param initiator [String] Task initiator identifier
      # @return [TaskerCore::TestHelpers::TestTask] Structured task object
      # @raise [TaskerCore::Error] If task creation fails
      def create_task(namespace: nil, name: nil, version: nil, context: nil, initiator: nil)
        context_json = context.is_a?(String) ? context : context&.to_json
        
        handle.create_test_task_optimized(namespace, name, version, context_json, initiator)
      rescue => e
        raise TaskerCore::Error, "Failed to create test task: #{e.message}"
      end

      # Create a test step with primitive parameters (optimized FFI boundary)
      # @param task_id [Integer] Parent task ID (required)
      # @param name [String] Step name (default: "test_step")
      # @param handler_class [String] Handler class name
      # @param dependencies [Array<Integer>] Step dependency IDs
      # @param config [Hash, String] Step configuration as hash or JSON string
      # @return [TaskerCore::TestHelpers::TestStep] Structured step object
      # @raise [TaskerCore::Error] If step creation fails
      def create_step(task_id:, name: nil, handler_class: nil, dependencies: nil, config: nil)
        config_json = config.is_a?(String) ? config : config&.to_json
        
        handle.create_test_step_optimized(task_id, name, handler_class, dependencies, config_json)
      rescue => e
        raise TaskerCore::Error, "Failed to create test step for task #{task_id}: #{e.message}"
      end

      # Setup test environment
      # @return [Hash] Environment setup result
      # @raise [TaskerCore::Error] If environment setup fails
      def setup_environment
        handle.setup_test_environment
      rescue => e
        raise TaskerCore::Error, "Failed to setup test environment: #{e.message}"
      end

      # Cleanup test environment
      # @return [Hash] Environment cleanup result
      # @raise [TaskerCore::Error] If environment cleanup fails
      def cleanup_environment
        handle.cleanup_test_environment
      rescue => e
        raise TaskerCore::Error, "Failed to cleanup test environment: #{e.message}"
      end

      # Create comprehensive test foundation (task + steps)
      # @param namespace [String] Foundation namespace (default: "test")
      # @param task_name [String] Task name (default: "test_task")
      # @param step_name [String] Step name (default: "test_step")
      # @return [Hash] Foundation creation result
      # @raise [TaskerCore::Error] If foundation creation fails
      def create_foundation(namespace: nil, task_name: nil, step_name: nil)
        foundation_options = {
          namespace: namespace || "test",
          task_name: task_name || "test_task",
          step_name: step_name || "test_step"
        }
        
        handle.create_test_foundation(foundation_options)
      rescue => e
        raise TaskerCore::Error, "Failed to create test foundation: #{e.message}"
      end

      # Database cleanup operations
      # @return [Hash] Cleanup result
      # @raise [TaskerCore::Error] If database cleanup fails
      def cleanup_database
        handle.cleanup_test_database
      rescue => e
        raise TaskerCore::Error, "Failed to cleanup test database: #{e.message}"
      end

      # Validate test environment health
      # @return [Hash] Validation result with status and metrics
      def validate_environment
        setup_result = setup_environment
        handle_status = handle_info
        
        {
          status: "healthy",
          environment: setup_result,
          handle: handle_status,
          validated_at: Time.now.utc.iso8601
        }
      rescue => e
        { 
          status: "unhealthy", 
          error: e.message, 
          validated_at: Time.now.utc.iso8601 
        }
      end

      # Get comprehensive handle information for debugging
      # @return [Hash] Handle status, validation info, and metadata
      def handle_info
        handle.info.merge(
          domain: "testing",
          available_methods: %w[
            create_task create_step setup_environment cleanup_environment
            create_foundation cleanup_database validate_environment
          ]
        )
      rescue => e
        { error: e.message, status: "unavailable", domain: "testing" }
      end

      private

      # Get orchestration handle with automatic refresh
      # @return [OrchestrationHandle] Active handle instance
      def handle
        Internal::OrchestrationManager.instance.orchestration_handle
      end
    end

    # Legacy compatibility - import existing testing managers for transition
    require_relative 'internal/testing_manager'
    require_relative 'internal/testing_factory_manager'
    
    # Provide Manager classes for backward compatibility during transition
    Manager = TestingManager
    FactoryManager = TestingFactoryManager
  end
end