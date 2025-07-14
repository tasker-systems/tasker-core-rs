# frozen_string_literal: true

require_relative '../../errors'

module TaskerCore
  module Events
    module Subscribers
      # ErrorSurfacingSubscriber converts error events from Rust into Ruby exceptions
      #
      # This subscriber listens for error events published by Rust orchestration
      # and surfaces them as appropriate Ruby exceptions. This allows Ruby code
      # to handle errors in idiomatic ways while keeping Rust as the source of truth.
      #
      # ## Error Event Flow
      #
      # 1. Ruby requests transition via `publisher.publish_task_transition()`
      # 2. Rust validates transition in state machine
      # 3. If invalid, Rust publishes `workflow.invalid_transition` event
      # 4. This subscriber receives event and raises `InvalidTransitionError`
      # 5. Ruby caller can rescue and handle the error appropriately
      #
      # ## Usage
      #
      # The subscriber is automatically registered when TaskerCore events are initialized.
      # Ruby code can handle errors naturally:
      #
      # ```ruby
      # begin
      #   publisher.publish_task_transition(task_id, 'completed', 'pending', {})
      # rescue TaskerCore::InvalidTransitionError => e
      #   logger.error "Invalid transition: #{e.message}"
      #   # Handle error appropriately
      # end
      # ```
      class ErrorSurfacingSubscriber < BaseSubscriber
        # Subscribe to error events from Rust orchestration
        # Note: These subscriptions would be set up after event registration
        # subscribe_to 'workflow.invalid_transition', 'workflow.orchestration_error'

        def initialize(logger: nil)
          @logger = logger || default_logger
          @error_callbacks = {}
        end

        # Handle invalid transition events from Rust
        #
        # @param event [Hash] Event payload from Rust
        def handle_workflow_invalid_transition(event)
          entity_type = safe_get(event, :entity_type, 'unknown')
          entity_id = safe_get(event, :entity_id, 'unknown')
          from_state = safe_get(event, :from_state)
          to_state = safe_get(event, :to_state)
          reason = safe_get(event, :reason, 'Invalid state transition')

          error_message = "Cannot transition #{entity_type} #{entity_id} from '#{from_state}' to '#{to_state}': #{reason}"

          @logger.error(error_message)

          # Create and store the error for the calling code to handle
          error = TaskerCore::InvalidTransitionError.new(
            error_message,
            entity_type: entity_type,
            entity_id: entity_id,
            from_state: from_state,
            to_state: to_state,
            reason: reason
          )

          # Store error for retrieval by calling code
          store_error_for_entity(entity_type, entity_id, error)

          # Could also trigger callbacks or other error handling here
          trigger_error_callbacks(entity_type, entity_id, error)
        end

        # Handle general orchestration errors from Rust
        #
        # @param event [Hash] Event payload from Rust
        def handle_workflow_orchestration_error(event)
          entity_type = safe_get(event, :entity_type, 'workflow')
          entity_id = safe_get(event, :entity_id, 'unknown')
          error_code = safe_get(event, :error_code, 'ORCHESTRATION_ERROR')
          message = safe_get(event, :message, 'Orchestration error occurred')
          context = event.reject { |k, _| %i[entity_type entity_id error_code message].include?(k) }

          error_message = "#{entity_type} #{entity_id} orchestration error [#{error_code}]: #{message}"

          @logger.error(error_message)

          error = TaskerCore::OrchestrationError.new(
            error_message,
            error_code: error_code,
            context: context
          )

          store_error_for_entity(entity_type, entity_id, error)
          trigger_error_callbacks(entity_type, entity_id, error)
        end

        # Register a callback to be triggered when errors occur for specific entities
        #
        # @param entity_type [String] 'task' or 'step'
        # @param entity_id [String] The entity ID
        # @param callback [Proc] Callback to execute when error occurs
        def on_error(entity_type, entity_id, &callback)
          key = "#{entity_type}:#{entity_id}"
          @error_callbacks[key] ||= []
          @error_callbacks[key] << callback
        end

        # Get the last error for a specific entity (useful for testing)
        #
        # @param entity_type [String] 'task' or 'step'
        # @param entity_id [String] The entity ID
        # @return [Exception, nil] The last error or nil
        def last_error_for(entity_type, entity_id)
          key = "#{entity_type}:#{entity_id}"
          @stored_errors ||= {}
          @stored_errors[key]
        end

        # Clear stored errors (useful for testing)
        def clear_errors
          @stored_errors ||= {}
          @stored_errors.clear
        end

        private

        # Store error for later retrieval
        def store_error_for_entity(entity_type, entity_id, error)
          key = "#{entity_type}:#{entity_id}"
          @stored_errors ||= {}
          @stored_errors[key] = error
        end

        # Trigger any registered callbacks for this entity
        def trigger_error_callbacks(entity_type, entity_id, error)
          key = "#{entity_type}:#{entity_id}"
          callbacks = @error_callbacks[key] || []

          callbacks.each do |callback|
            callback.call(error)
          rescue StandardError => e
            @logger.error("Error in error callback: #{e.message}")
          end
        end

        def default_logger
          defined?(Rails) ? Rails.logger : Logger.new($stdout)
        end
      end
    end
  end
end
