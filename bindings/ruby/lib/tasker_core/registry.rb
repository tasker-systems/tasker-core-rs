# frozen_string_literal: true

require_relative 'errors'

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

      # Find a handler by name and version (alternative API for integration tests)
      # @param name [String] Handler name with namespace (e.g., "fulfillment/process_order")
      # @param version [String] Handler version (default: "1.0.0")
      # @return [Hash, nil] Handler metadata if found, nil otherwise
      def find_handler(name:, version: "1.0.0")
        # Parse namespace from name if present
        if name.include?('/')
          namespace, handler_name = name.split('/', 2)
        else
          namespace = "default"
          handler_name = name
        end

        task_request = {
          namespace: namespace,
          name: handler_name,
          version: version
        }

        find(task_request)
      end

      # Find handler and create initialized instance for production testing
      # This combines handler lookup with proper initialization
      # @param name [String] Handler name with namespace (e.g., "fulfillment/process_order")
      # @param version [String] Handler version (default: "1.0.0")
      # @param config_path [String, nil] Optional path to configuration file
      # @return [Hash] Result with handler lookup and initialized instance
      # @raise [TaskerCore::Errors::NotFoundError] If handler not found
      # @raise [TaskerCore::Errors::PermanentError] If initialization fails
      def find_handler_and_initialize(name:, version: "1.0.0", config_path: nil)
        # Look up handler in registry
        handler_lookup = find_handler(name: name, version: version)

        unless handler_lookup && handler_lookup['found']
          raise TaskerCore::Errors::NotFoundError.new(
            "Handler not found: #{name}/#{version}",
            resource_type: 'handler',
            error_code: 'HANDLER_NOT_FOUND',
            context: { name: name, version: version }
          )
        end

        # Get handler class and create instance
        handler_class_name = handler_lookup['handler_class']

        begin
          handler_class = Object.const_get(handler_class_name)
        rescue NameError => e
          raise TaskerCore::Errors::NotFoundError.new(
            "Handler class not found: #{handler_class_name}",
            resource_type: 'handler_class',
            error_code: 'HANDLER_CLASS_NOT_FOUND',
            context: { class_name: handler_class_name, lookup_error: e.message }
          )
        end

        # Initialize handler with configuration
        begin
          # Load configuration if path provided, otherwise use registry config
          config = if config_path && File.exist?(config_path)
                     require 'yaml'
                     YAML.load_file(config_path)
                   else
                     handler_lookup['config_schema'] || {}
                   end

          # Create handler instance with proper configuration
          handler_instance = handler_class.new(
            config: config['handler_config'] || {},
            task_config: config,
            task_config_path: config_path
          )

          # Return both lookup result and initialized instance
          {
            'found' => true,
            'handler_class' => handler_class_name,
            'handler_instance' => handler_instance,
            'metadata' => handler_lookup['metadata'] || {},
            'config_path' => config_path,
            'namespace' => handler_lookup['namespace'],
            'name' => handler_lookup['name'],
            'version' => handler_lookup['version']
          }
        rescue StandardError => e
          raise TaskerCore::Errors::PermanentError.new(
            "Handler initialization failed: #{e.message}",
            error_code: 'HANDLER_INITIALIZATION_FAILED',
            context: {
              class_name: handler_class_name,
              config_path: config_path,
              error_message: e.message,
              error_class: e.class.name
            }
          )
        end
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
