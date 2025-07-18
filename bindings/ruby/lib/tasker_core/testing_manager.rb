# frozen_string_literal: true

require 'singleton'

module TaskerCore
  # Ruby-level wrapper for the unified testing infrastructure
  # This provides a singleton pattern at the Ruby level to avoid
  # re-initializing the testing framework and factory on every call
  class TestingManager
    include Singleton

    attr_reader :initialized_at, :status, :logger

    def initialize
      @initialized = false
      @status = 'not_initialized'
      @initialized_at = nil
      @testing_framework = nil
      @testing_factory = nil
      @logger = TaskerCore::Logging::Logger.instance
    end

    # Initialize the unified testing infrastructure once
    def initialize_testing_infrastructure!
      return true if @initialized

      begin
        # 🎯 HANDLE-BASED: Initialize testing framework using OrchestrationManager handle method
        # We don't store the result, just ensure it's been initialized
        orchestration_manager = TaskerCore::OrchestrationManager.instance
        orchestration_manager.setup_test_environment_with_handle
        @initialized = true
        @status = 'initialized'
        @initialized_at = defined?(Rails) ? Time.current : Time.now

        logger.info "TestingManager: Unified testing infrastructure initialized successfully"
        true
      rescue StandardError => e
        @status = 'failed'
        error_msg = "Failed to initialize unified testing infrastructure: #{e.message}"

        logger.error error_msg

        raise e
      end
    end

    # Check if the testing infrastructure is initialized
    def initialized?
      @initialized
    end

    # Get testing infrastructure status information
    def info
      {
        initialized: @initialized,
        status: @status,
        initialized_at: @initialized_at
      }
    end

    # Reset the testing infrastructure (for testing)
    def reset!
      @initialized = false
      @status = 'reset'
      @initialized_at = nil
      @testing_framework = nil
      @testing_factory = nil
    end

    # Delegate common testing operations to avoid direct FFI calls

    def create_test_task(options = {})
      initialize_testing_infrastructure! unless @initialized
      # 🎯 HANDLE-BASED: Use OrchestrationManager handle method instead of direct TestHelpers call
      TaskerCore::OrchestrationManager.instance.create_test_task_with_handle(options)
    end

    def create_test_workflow_step(options = {})
      initialize_testing_infrastructure! unless @initialized
      # 🎯 HANDLE-BASED: Use OrchestrationManager handle method instead of direct TestHelpers call
      TaskerCore::OrchestrationManager.instance.create_test_workflow_step_with_handle(options)
    end

    def create_test_foundation(options = {})
      initialize_testing_infrastructure! unless @initialized
      # 🎯 HANDLE-BASED: Use OrchestrationManager handle method instead of direct TestHelpers call
      TaskerCore::OrchestrationManager.instance.create_test_foundation_with_handle(options)
    end

    def setup_test_environment
      initialize_testing_infrastructure! unless @initialized
      # 🎯 HANDLE-BASED: Use OrchestrationManager handle method instead of direct TestingFramework call
      TaskerCore::OrchestrationManager.instance.setup_test_environment_with_handle
    end

    def cleanup_test_environment
      initialize_testing_infrastructure! unless @initialized
      # 🎯 HANDLE-BASED: Use OrchestrationManager handle method instead of direct TestingFramework call
      TaskerCore::OrchestrationManager.instance.cleanup_test_environment_with_handle
    end
  end
end
