# frozen_string_literal: true

require_relative 'orchestration/orchestration_manager'

module TaskerCore
  # Clean Events Domain API
  #
  # This provides a clean, Ruby-idiomatic API for all event operations
  # following the proven Factory/Registry pattern with handle-based optimization.
  #
  # Examples:
  #   TaskerCore::Events.publish(name: "task.completed", payload: {task_id: 123})
  #   TaskerCore::Events.publish_orchestration(type: "step.started", namespace: "workflow")
  #   TaskerCore::Events.statistics
  #   TaskerCore::Events.subscribe(pattern: "task.*", callback: my_callback)
  module Events
    class << self
      # Publish a simple event with primitive parameters (optimized FFI boundary)
      # @param name [String] Event name (required)
      # @param payload [Hash, String] Event payload as hash or JSON string
      # @param source [String] Event source identifier
      # @param metadata [Hash, String] Event metadata as hash or JSON string
      # @return [TaskerCore::Events::EventResult] Structured result object
      # @raise [TaskerCore::Error] If event publishing fails
      def publish(name:, payload: nil, source: nil, metadata: nil)
        payload_json = payload.is_a?(String) ? payload : payload&.to_json
        metadata_json = metadata.is_a?(String) ? metadata : metadata&.to_json

        handle.publish_simple_event_optimized(name, payload_json, source, metadata_json)
      rescue => e
        raise TaskerCore::Error, "Failed to publish event '#{name}': #{e.message}"
      end

      # Publish an orchestration event with structured parameters
      # @param type [String] Event type (required)
      # @param namespace [String] Event namespace (default: "tasker_orchestration")
      # @param version [String] Event version
      # @param data [Hash, String] Event data as hash or JSON string
      # @param context [Hash, String] Event context as hash or JSON string
      # @return [TaskerCore::Events::EventResult] Structured result object
      # @raise [TaskerCore::Error] If orchestration event publishing fails
      def publish_orchestration(type:, namespace: nil, version: nil, data: nil, context: nil)
        data_json = data.is_a?(String) ? data : data&.to_json
        context_json = context.is_a?(String) ? context : context&.to_json

        handle.publish_orchestration_event_optimized(type, namespace, version, data_json, context_json)
      rescue => e
        raise TaskerCore::Error, "Failed to publish orchestration event '#{type}': #{e.message}"
      end

      # Get event statistics with structured output
      # @return [TaskerCore::Events::EventStatistics] Structured statistics object
      # @raise [TaskerCore::Error] If statistics retrieval fails
      def statistics
        # Call the module-level function from the Events module where it's actually registered
        TaskerCore::Events.get_event_statistics_optimized
      rescue => e
        raise TaskerCore::Error, "Failed to get event statistics: #{e.message}"
      end

      # Subscribe to events with pattern matching (legacy compatibility)
      # @param pattern [String] Event pattern to match
      # @param callback [Proc, Object] Callback to invoke on events
      # @return [Hash] Subscription result
      # @raise [TaskerCore::Error] If subscription fails
      def subscribe(pattern:, callback:)
        subscription_data = { event_pattern: pattern, callback: callback }
        handle.subscribe_to_events_with_handle(subscription_data)
      rescue => e
        raise TaskerCore::Error, "Failed to subscribe to pattern '#{pattern}': #{e.message}"
      end

      # Register external event callback for cross-language integration
      # @param callback_name [String] Unique callback identifier
      # @param callback_data [Hash] Callback configuration
      # @return [Hash] Registration result
      # @raise [TaskerCore::Error] If callback registration fails
      def register_callback(callback_name:, callback_data: {})
        data = { callback_name: callback_name }.merge(callback_data)
        handle.register_external_event_callback_with_handle(data)
      rescue => e
        raise TaskerCore::Error, "Failed to register callback '#{callback_name}': #{e.message}"
      end

      # Get comprehensive handle information for debugging
      # @return [Hash] Handle status, validation info, and metadata
      def handle_info
        info = handle.info
        # Handle OrchestrationHandleInfo object (which has no accessible methods from Ruby)
        base_info = if info.is_a?(Hash)
          info
        else
          # OrchestrationHandleInfo object - use consistent handle ID
          {
            'handle_id' => "shared_orchestration_handle",
            'status' => 'operational',
            'handle_type' => 'orchestration_handle',
            'created_at' => Time.now.utc.iso8601
          }
        end

        base_info.merge(
          'domain' => 'events',
          'available_methods' => %w[
            publish publish_orchestration statistics subscribe register_callback
          ]
        )
      rescue => e
        { 'error' => e.message, 'status' => "unavailable", 'domain' => "events" }
      end

      # Health check for event system
      # @return [Hash] Health status and metrics
      def health
        stats = statistics
        {
          status: stats.health_status,
          total_events: stats.total_events_published,
          success_rate: stats.callback_success_rate,
          active_bindings: stats.active_language_bindings.size,
          last_checked: Time.now.utc.iso8601
        }
      rescue => e
        { status: "error", error: e.message, last_checked: Time.now.utc.iso8601 }
      end

      private

      # Get orchestration handle with automatic refresh
      # @return [OrchestrationHandle] Active handle instance
      def handle
        TaskerCore::Internal::OrchestrationManager.instance.orchestration_handle
      end
    end

    # Legacy compatibility - import existing Events::Domain for transition
    require_relative 'events/publisher'
    require_relative 'events/subscribers/base_subscriber'
    require_relative 'events/subscribers/error_surfacing_subscriber'
    require_relative 'events/concerns/event_based_transitions'

    # Provide Domain module for backward compatibility
    require_relative 'events_domain'
    # Note: Domain module is defined in events_domain.rb
  end
end
