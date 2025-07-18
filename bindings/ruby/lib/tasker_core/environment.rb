# frozen_string_literal: true

module TaskerCore
  # Domain module for test environment management with singleton handle management
  # 
  # This module provides a clean, Ruby-idiomatic API for environment operations
  # while internally managing a persistent OrchestrationHandle for optimal performance.
  #
  # Examples:
  #   TaskerCore::Environment.setup_test
  #   TaskerCore::Environment.cleanup_test
  #   TaskerCore::Environment.create_testing_framework
  module Environment
    class << self
      # Setup test environment with all necessary components
      # @return [Hash] Setup result with status and metadata
      # @raise [TaskerCore::Error] If environment setup fails
      def setup_test
        OrchestrationManager.instance.setup_test_environment_with_handle
      rescue => e
        raise TaskerCore::Error, "Failed to setup test environment: #{e.message}"
      end

      # Cleanup test environment and resources
      # @return [Hash] Cleanup result with status and metadata
      # @raise [TaskerCore::Error] If environment cleanup fails
      def cleanup_test
        OrchestrationManager.instance.cleanup_test_environment_with_handle
      rescue => e
        raise TaskerCore::Error, "Failed to cleanup test environment: #{e.message}"
      end

      # Create testing framework instance
      # @return [Hash] Testing framework creation result
      # @raise [TaskerCore::Error] If testing framework creation fails
      def create_testing_framework
        OrchestrationManager.instance.create_testing_framework_with_handle
      rescue => e
        raise TaskerCore::Error, "Failed to create testing framework: #{e.message}"
      end

      # Get handle information for debugging
      # @return [Hash] Handle status and metadata
      def handle_info
        # Environment operations use OrchestrationManager singleton handle
        OrchestrationManager.instance.handle_info
      rescue => e
        { error: e.message, status: "unavailable" }
      end
    end
  end
end