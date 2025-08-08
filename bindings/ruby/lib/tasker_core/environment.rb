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
      # Get handle information for debugging
      # @return [Hash] Handle status and metadata
      def handle_info
        # Environment operations use OrchestrationManager singleton handle
        info = Internal::OrchestrationManager.instance.handle_info
        # Handle OrchestrationHandleInfo object (which has no accessible methods from Ruby)
        if info.is_a?(Hash)
          info.merge('domain' => 'Environment')
        else
          # OrchestrationHandleInfo object - return basic info since methods aren't accessible
          {
            'handle_id' => "environment_handle_#{info.object_id}",
            'status' => 'operational',
            'domain' => 'Environment',
            'handle_type' => 'orchestration_handle',
            'created_at' => Time.now.utc.iso8601,
            'available_methods' => %w[setup_test cleanup_test create_testing_framework]
          }
        end
      rescue StandardError => e
        { error: e.message, status: 'unavailable' }
      end
    end
  end
end
