# frozen_string_literal: true

require_relative 'models'

module TaskerCore
  module Worker
    # Event bridge between Rust and Ruby using dry-events
    #
    # Handles bidirectional event communication between the Rust orchestration
    # layer and Ruby business logic handlers. The bridge uses dry-events for
    # Ruby-side pub/sub and FFI for cross-language communication.
    #
    # Event Flow:
    # 1. **Rust → Ruby**: StepExecutionEvent indicates step is ready for processing
    # 2. **Ruby Processing**: Handler executes business logic
    # 3. **Ruby → Rust**: StepExecutionCompletionEvent returns results
    #
    # The EventBridge automatically wraps raw FFI data in accessor objects for
    # developer convenience and maintains event schema validation.
    #
    # @example Publishing step execution (from Rust FFI)
    #   # This is called automatically by Rust via FFI when a step is ready
    #   bridge = TaskerCore::Worker::EventBridge.instance
    #   bridge.publish_step_execution({
    #     event_id: "550e8400-e29b-41d4-a716-446655440000",
    #     task_uuid: "7c9e6679-7425-40de-944b-e07fc1f90ae7",
    #     step_uuid: "123e4567-e89b-12d3-a456-426614174000",
    #     task_sequence_step: {
    #       task: { context: { amount: 100.00, currency: "USD" } },
    #       step: { name: "process_payment", handler_class: "ProcessPaymentHandler" },
    #       workflow_step: { state: "in_progress", attempts: 1 }
    #     }
    #   })
    #   # => Publishes 'step.execution.received' event to Ruby subscribers
    #
    # @example Subscribing to step execution events
    #   # This is typically done in StepExecutionSubscriber
    #   bridge.subscribe_to_step_execution do |event|
    #     # Resolve handler
    #     handler_class = event[:task_sequence_step].handler_class
    #     handler = registry.resolve_handler(handler_class)
    #
    #     # Execute handler
    #     result = handler.call(
    #       event[:task_sequence_step].task,
    #       event[:task_sequence_step].sequence,
    #       event[:task_sequence_step].workflow_step
    #     )
    #
    #     # Send completion back to Rust
    #     bridge.publish_step_completion({
    #       event_id: event[:event_id],
    #       task_uuid: event[:task_uuid],
    #       step_uuid: event[:step_uuid],
    #       success: true,
    #       result: result
    #     })
    #   end
    #
    # @example Sending completion back to Rust
    #   bridge.publish_step_completion({
    #     event_id: "550e8400-e29b-41d4-a716-446655440000",
    #     task_uuid: "7c9e6679-7425-40de-944b-e07fc1f90ae7",
    #     step_uuid: "123e4567-e89b-12d3-a456-426614174000",
    #     success: true,
    #     result: { payment_id: "pay_123", status: "succeeded" },
    #     metadata: {
    #       handler_class: "ProcessPaymentHandler",
    #       execution_time_ms: 125
    #     }
    #   })
    #   # => Sends completion to Rust via FFI and publishes 'step.completion.sent'
    #
    # @example Handling errors in step execution
    #   begin
    #     result = handler.call(task, sequence, step)
    #     bridge.publish_step_completion({
    #       event_id: event_id,
    #       task_uuid: task_uuid,
    #       step_uuid: step_uuid,
    #       success: true,
    #       result: result
    #     })
    #   rescue TaskerCore::Errors::RetryableError => e
    #     bridge.publish_step_completion({
    #       event_id: event_id,
    #       task_uuid: task_uuid,
    #       step_uuid: step_uuid,
    #       success: false,
    #       error_message: e.message,
    #       error_class: e.class.name,
    #       retryable: true,
    #       retry_after: e.retry_after
    #     })
    #   rescue TaskerCore::Errors::PermanentError => e
    #     bridge.publish_step_completion({
    #       event_id: event_id,
    #       task_uuid: task_uuid,
    #       step_uuid: step_uuid,
    #       success: false,
    #       error_message: e.message,
    #       error_class: e.class.name,
    #       retryable: false
    #     })
    #   end
    #
    # Event Flow Diagram:
    #
    # ```
    # Rust Orchestration                EventBridge                 Ruby Handler
    # -----------------                -----------                 ------------
    #       |                                |                           |
    #       | 1. Step ready for execution    |                           |
    #       |------------------------------->|                           |
    #       |    publish_step_execution      |                           |
    #       |                                | 2. Publish event          |
    #       |                                |-------------------------->|
    #       |                                |    step.execution.received|
    #       |                                |                           |
    #       |                                |                  3. Execute handler
    #       |                                |                           |
    #       |                                | 4. Completion             |
    #       |                                |<--------------------------|
    #       | 5. FFI completion              |    publish_step_completion|
    #       |<-------------------------------|                           |
    #       |    send_step_completion_event  |                           |
    # ```
    #
    # Registered Events:
    # - **step.execution.received**: Step ready for execution (Rust → Ruby)
    # - **step.completion.sent**: Step execution completed (Ruby → Rust)
    # - **bridge.error**: Error in event processing
    #
    # Completion Data Validation:
    # The bridge validates completion data before sending to Rust:
    # - **event_id**: Required, UUID of the original execution event
    # - **task_uuid**: Required, UUID of the task
    # - **step_uuid**: Required, UUID of the step
    # - **success**: Required, boolean indicating success/failure
    # - **metadata**: Optional, hash with additional context
    # - **completed_at**: Optional, ISO 8601 timestamp (auto-generated if missing)
    #
    # @see TaskerCore::Subscriber For event subscription implementation
    # @see TaskerCore::Worker::EventPoller For polling mechanism
    # @see TaskerCore::FFI For Rust FFI operations
    # @see TaskerCore::Models::TaskSequenceStepWrapper For data wrappers
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
        wrapped = {
          event_id: event_data[:event_id],
          task_uuid: event_data[:task_uuid],
          step_uuid: event_data[:step_uuid],
          task_sequence_step: TaskerCore::Models::TaskSequenceStepWrapper.new(event_data[:task_sequence_step])
        }

        # TAS-29: Expose correlation_id at top level for easy access
        wrapped[:correlation_id] = event_data[:correlation_id] if event_data[:correlation_id]
        wrapped[:parent_correlation_id] = event_data[:parent_correlation_id] if event_data[:parent_correlation_id]

        # TAS-65 Phase 1.5b: Expose trace_id and span_id for distributed tracing
        wrapped[:trace_id] = event_data[:trace_id] if event_data[:trace_id]
        wrapped[:span_id] = event_data[:span_id] if event_data[:span_id]

        wrapped
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
