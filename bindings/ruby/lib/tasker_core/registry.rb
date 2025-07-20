# frozen_string_literal: true

module TaskerCore
  # Domain module for handler management with singleton handle management
  #
  # This module provides a clean, Ruby-idiomatic API for managing task handlers
  # while internally managing a persistent OrchestrationHandle for optimal performance.
  #
  # Examples:
  #   TaskerCore::Registry.register(namespace: "payments", name: "process_card", handler_class: "PaymentHandler")
  #   handler = TaskerCore::Registry.find(namespace: "payments", name: "process_card")
  #   exists = TaskerCore::Registry.include?("payments::process_card")
  #   handlers = TaskerCore::Registry.list("payments")
  module Registry
    class << self
      # Register a new handler in the registry
      # @param handler_data [Hash] Handler registration data
      # @option handler_data [String] :namespace Handler namespace (default: "default")
      # @option handler_data [String] :name Handler name (required)
      # @option handler_data [String] :version Handler version (default: "0.1.0")
      # @option handler_data [String] :handler_class Ruby class name (required)
      # @option handler_data [Hash] :config_schema Optional configuration schema
      # @return [Hash] Registration result with status
      # @raise [TaskerCore::Error] If registration fails
      def register(handler_data)
        result = handle.register_handler(handler_data)

        # Check if result indicates an error
        if result.is_a?(Hash) && result['status'] == 'error'
          raise TaskerCore::Error, result['error']
        end

        result
      rescue => e
        raise TaskerCore::Error, "Failed to register handler: #{e.message}"
      end

      # Find a handler by task request
      # @param task_request [Hash] Task request with namespace, name, version
      # @return [Hash, nil] Handler metadata if found, nil otherwise
      def find(task_request)
        result = handle.find_handler(task_request)

        if result.is_a?(Hash) && result['found']
          result
        else
          nil
        end
      rescue => e
        TaskerCore::Logging::Logger.instance.warn("Handler lookup failed: #{e.message}")
        nil
      end

      # Check if a handler exists in the registry
      # @param handler_key [String] Handler key to check
      # @return [Boolean] True if handler exists, false otherwise
      def include?(handler_key)
        # Convert handler key to task request format for find operation
        namespace, name = handler_key.to_s.split('::', 2)
        if name.nil?
          name = namespace
          namespace = "default"
        end

        task_request = {
          namespace: namespace,
          name: name,
          version: "0.1.0"  # Default version for existence check
        }

        !find(task_request).nil?
      rescue => e
        TaskerCore::Logging::Logger.instance.warn("Handler existence check failed: #{e.message}")
        false
      end

      # List all handlers in a namespace
      # @param namespace [String, nil] Namespace to filter by (nil for all)
      # @return [Array<Hash>] List of handler metadata
      # @note Currently returns empty array - full implementation pending
      def list(namespace = nil)
        # TODO: Implement proper handler listing once core registry supports it
        # For now, return empty array to maintain API compatibility
        []
      rescue => e
        TaskerCore::Logging::Logger.instance.warn("Handler listing failed: #{e.message}")
        []
      end

      # Get information about the internal handle for debugging
      # @return [Hash] Handle status and metadata
      def handle_info
        info = handle.info
        # Handle OrchestrationHandleInfo object (which has no accessible methods from Ruby)
        if info.is_a?(Hash)
          info.merge('domain' => 'Registry')
        else
          # OrchestrationHandleInfo object - use consistent handle ID
          {
            'handle_id' => "shared_orchestration_handle",
            'status' => 'operational',
            'domain' => 'Registry',
            'handle_type' => 'orchestration_handle',
            'created_at' => Time.now.utc.iso8601,
            'available_methods' => %w[register find include? list]
          }
        end
      rescue => e
        { error: e.message, status: "unavailable" }
      end

      private

      # Get the shared orchestration handle from OrchestrationManager
      # This ensures all domains use the same handle instance for optimal
      # performance and resource utilization.
      def handle
        TaskerCore::Internal::OrchestrationManager.instance.orchestration_handle
      end
    end
  end
end
