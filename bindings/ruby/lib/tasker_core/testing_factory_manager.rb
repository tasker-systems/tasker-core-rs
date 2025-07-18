# frozen_string_literal: true

require 'singleton'

module TaskerCore
  # Ruby-level singleton coordinator for TestingFactory operations
  #
  # This ensures that the Rust-side TestingFactory singleton is initialized
  # exactly once and all subsequent factory operations use the same instance.
  # The actual factory logic and singleton pattern is handled on the Rust side.
  class TestingFactoryManager
    include Singleton

    attr_reader :initialized_at, :status, :logger

    def initialize
      @initialized = false
      @status = 'not_initialized'
      @initialized_at = nil
      @logger = TaskerCore::Logging::Logger.instance
    end

    # Initialize the Rust TestingFactory singleton coordination
    def initialize_factory_coordination!
      return true if @initialized

      begin
        # 🎯 CRITICAL FIX: Ensure OrchestrationManager is initialized first
        # This prevents TestingFactory from creating its own orchestration system
        orchestration_manager = TaskerCore::OrchestrationManager.instance
        orchestration_manager.orchestration_system # Ensure orchestration system is ready

        # Trigger the Rust-side singleton initialization by calling a setup function
        # This ensures the Rust `get_global_testing_factory()` singleton is created
        TaskerCore::TestHelpers.setup_factory_singleton

        @initialized = true
        @status = 'initialized'
        @initialized_at = defined?(Rails) ? Time.current : Time.now

        logger.info "TestingFactoryManager: Factory coordination initialized successfully (with OrchestrationManager coordination)"
        true
      rescue StandardError => e
        @status = 'failed'
        error_msg = "Failed to initialize factory coordination: #{e.message}"

        logger.error error_msg

        raise e
      end
    end

    # Check if factory coordination is initialized
    def initialized?
      @initialized
    end

    # Get factory coordination status information
    def info
      {
        initialized: @initialized,
        status: @status,
        initialized_at: @initialized_at,
        rust_singleton: @initialized ? 'active' : 'not_initialized'
      }
    end

    # Reset the factory coordination (for testing)
    def reset!
      @initialized = false
      @status = 'reset'
      @initialized_at = nil
    end

    # Factory operations with singleton coordination
    # These ensure the Rust singleton is initialized once, then delegate to the FFI functions

    def create_test_task(options = {})
      initialize_factory_coordination! unless @initialized
      TaskerCore::TestHelpers.create_test_task_with_factory(options)
    end

    def create_test_workflow_step(options = {})
      initialize_factory_coordination! unless @initialized
      TaskerCore::TestHelpers.create_test_workflow_step_with_factory(options)
    end

    def create_test_foundation(options = {})
      initialize_factory_coordination! unless @initialized
      TaskerCore::TestHelpers.create_test_foundation_with_factory(options)
    end
  end
end
