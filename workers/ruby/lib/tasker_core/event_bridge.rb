# frozen_string_literal: true

require_relative 'models'

module TaskerCore
  module Worker
    # Event bridge between Rust and Ruby using dry-events
    # Handles StepExecutionEvent (from Rust) and StepExecutionCompletionEvent (to Rust)
    class EventBridge
      include Singleton
      include Dry::Events::Publisher[:tasker_core]

      attr_reader :logger, :active

      def initialize
        @logger = TaskerCore::Logger.instance
        @active = true

        setup_event_schema!
      end

      # Check if bridge is active
      def active?
        @active
      end

      # Stop the event bridge
      def stop!
        @active = false
        logger.info 'Event bridge stopped'
      end

      # Called by Rust FFI when StepExecutionEvent is received
      # This is the entry point for events from Rust
      def publish_step_execution(event_data)
        return unless active?

        event_data = event_data.to_h.deep_symbolize_keys
        logger.debug "Publishing step execution event: #{event_data[:event_id]}"

        # Wrap the raw data in accessor objects for easier use
        wrapped_event = wrap_step_execution_event(event_data)

        # Publish to dry-events subscribers (Ruby handlers)
        publish('step.execution.received', wrapped_event)

        logger.debug 'Step execution event published successfully'
        true
      rescue StandardError => e
        logger.error "Failed to publish step execution: #{e.message}"
        logger.error e.backtrace.join("\n")
        raise
      end

      # Subscribe to step execution events (used by StepExecutionSubscriber)
      def subscribe_to_step_execution(&)
        subscribe('step.execution.received', &)
      end

      # Send completion event back to Rust
      # Called by StepExecutionSubscriber after handler execution
      def publish_step_completion(completion_data)
        return unless active?

        logger.debug "Sending step completion to Rust: #{completion_data[:event_id]}"

        # Validate completion data
        validate_completion!(completion_data)

        # Send to Rust via FFI
        TaskerCore::FFI.send_step_completion_event(completion_data)

        # Also publish locally for monitoring/debugging
        publish('step.completion.sent', completion_data)

        logger.debug 'Step completion sent to Rust'
      rescue StandardError => e
        logger.error "Failed to send step completion: #{e.message}"
        logger.error e.backtrace.join("\n")
        raise
      end

      private

      def setup_event_schema!
        # Register event types
        register_event('step.execution.received')
        register_event('step.completion.sent')
        register_event('bridge.error')
      end

      def wrap_step_execution_event(event_data)
        {
          event_id: event_data[:event_id],
          task_uuid: event_data[:task_uuid],
          step_uuid: event_data[:step_uuid],
          task_sequence_step: TaskerCore::Models::TaskSequenceStepWrapper.new(event_data[:task_sequence_step])
        }
      end

      def validate_completion!(completion_data)
        required_fields = %i[event_id task_uuid step_uuid success]
        missing_fields = required_fields - completion_data.keys

        if missing_fields.any?
          raise ArgumentError, "Missing required fields in completion: #{missing_fields.join(', ')}"
        end

        # Ensure metadata is a hash
        completion_data[:metadata] ||= {}

        # Ensure timestamps
        completion_data[:completed_at] ||= Time.now.utc.iso8601
      end
    end
  end
end
