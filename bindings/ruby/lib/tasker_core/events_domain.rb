# frozen_string_literal: true

module TaskerCore
  module Events
    # Domain API for clean event operations with singleton handle management
    # 
    # This provides a clean, Ruby-idiomatic API for event operations
    # while internally managing handle-based operations for optimal performance.
    #
    # Examples:
    #   TaskerCore::Events::Domain.publish(type: "task.completed", task_id: 123)
    #   TaskerCore::Events::Domain.subscribe(callback_instance)
    #   TaskerCore::Events::Domain.register_bridge(bridge_instance)
    module Domain
      class << self
        # Publish a simple event with the given data
        # @param event_data [Hash] Event data to publish
        # @option event_data [String] :type Event type identifier
        # @option event_data [Hash] :payload Event payload data
        # @return [Hash] Publication result
        # @raise [TaskerCore::Error] If event publishing fails
        def publish(event_data)
          OrchestrationManager.instance.publish_simple_event_with_handle(event_data)
        rescue => e
          raise TaskerCore::Error, "Failed to publish event: #{e.message}"
        end

        # Subscribe to events with a callback
        # @param subscription_data [Hash] Subscription configuration
        # @return [Hash] Subscription result
        # @raise [TaskerCore::Error] If subscription fails
        def subscribe(subscription_data)
          OrchestrationManager.instance.subscribe_to_events_with_handle(subscription_data)
        rescue => e
          raise TaskerCore::Error, "Failed to subscribe to events: #{e.message}"
        end

        # Register an event bridge for cross-language event forwarding
        # @param bridge_instance [Object] Bridge instance to register
        # @return [Hash] Registration result
        # @raise [TaskerCore::Error] If bridge registration fails
        def register_bridge(bridge_instance)
          OrchestrationManager.instance.register_event_bridge_with_handle(bridge_instance)
        rescue => e
          raise TaskerCore::Error, "Failed to register event bridge: #{e.message}"
        end

        # Unregister the current event bridge
        # @return [Hash] Unregistration result
        # @raise [TaskerCore::Error] If bridge unregistration fails
        def unregister_bridge
          OrchestrationManager.instance.unregister_event_bridge_with_handle
        rescue => e
          raise TaskerCore::Error, "Failed to unregister event bridge: #{e.message}"
        end

        # Get handle information for debugging
        # @return [Hash] Handle status and metadata
        def handle_info
          # Events operations use OrchestrationManager singleton handle
          OrchestrationManager.instance.handle_info
        rescue => e
          { error: e.message, status: "unavailable" }
        end
      end
    end
  end
end