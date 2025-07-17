# frozen_string_literal: true

require 'singleton'

module TaskerCore
  # Ruby-level wrapper for the unified testing infrastructure
  # This provides a singleton pattern at the Ruby level to avoid
  # re-initializing the testing framework and factory on every call
  class TestingManager
    include Singleton
    
    attr_reader :initialized_at, :status
    
    def initialize
      @initialized = false
      @status = 'not_initialized'
      @initialized_at = nil
      @testing_framework = nil
      @testing_factory = nil
    end
    
    # Initialize the unified testing infrastructure once
    def initialize_testing_infrastructure!
      return true if @initialized
      
      begin
        # Initialize testing framework using the unified orchestration system
        # We don't store the result, just ensure it's been initialized
        TaskerCore::TestingFramework.setup_test_environment_with_framework
        @initialized = true
        @status = 'initialized'
        @initialized_at = defined?(Rails) ? Time.current : Time.now
        
        Rails.logger.info "TestingManager: Unified testing infrastructure initialized successfully" if defined?(Rails)
        true
      rescue StandardError => e
        @status = 'failed'
        error_msg = "Failed to initialize unified testing infrastructure: #{e.message}"
        
        if defined?(Rails)
          Rails.logger.error error_msg
        else
          warn error_msg
        end
        
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
      TaskerCore::TestHelpers.create_test_task_with_factory(options)
    end
    
    def create_test_workflow_step(options = {})
      initialize_testing_infrastructure! unless @initialized
      TaskerCore::TestHelpers.create_test_workflow_step_with_factory(options)
    end
    
    def create_test_foundation(options = {})
      initialize_testing_infrastructure! unless @initialized
      TaskerCore::TestHelpers.create_test_foundation_with_factory(options)
    end
    
    def setup_test_environment
      initialize_testing_infrastructure! unless @initialized
      TaskerCore::TestingFramework.setup_test_environment_with_framework
    end
    
    def cleanup_test_environment
      initialize_testing_infrastructure! unless @initialized
      TaskerCore::TestingFramework.cleanup_test_environment_with_framework
    end
  end
end