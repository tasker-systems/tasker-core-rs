# frozen_string_literal: true

module TaskerCore
  module DomainEvents
    # TAS-65: Base class for custom domain event publishers
    #
    # Domain event publishers transform step execution results into business events.
    # They are declared in task template YAML via the `publisher:` field and registered
    # at bootstrap time.
    #
    # @example Creating a custom publisher
    #   class PaymentEventPublisher < TaskerCore::DomainEvents::BasePublisher
    #     def name
    #       'PaymentEventPublisher'
    #     end
    #
    #     def transform_payload(step_result, event_declaration)
    #       # Extract business-specific payload from step result
    #       {
    #         payment_id: step_result[:result][:payment_id],
    #         amount: step_result[:result][:amount],
    #         currency: step_result[:result][:currency],
    #         status: step_result[:success] ? 'succeeded' : 'failed'
    #       }
    #     end
    #
    #     def should_publish?(step_result, event_declaration)
    #       # Only publish for successful payments
    #       step_result[:success] && step_result[:result][:payment_id].present?
    #     end
    #   end
    #
    # @example Registering a custom publisher
    #   TaskerCore::DomainEvents::PublisherRegistry.instance.register(
    #     PaymentEventPublisher.new
    #   )
    #
    # @example YAML declaration
    #   steps:
    #     - name: process_payment
    #       publishes_events:
    #         - name: payment.processed
    #           publisher: PaymentEventPublisher
    #           delivery_mode: durable
    #           condition: success
    #
    class BasePublisher
      attr_reader :logger

      def initialize
        @logger = TaskerCore::Logger.instance
      end

      # The publisher name used for registry lookup
      # Must match the `publisher:` field in YAML
      #
      # @return [String] The publisher name
      def name
        raise NotImplementedError, "#{self.class} must implement #name"
      end

      # Transform step result into business event payload
      #
      # Override this to customize the event payload for your domain.
      # The default implementation returns the step result as-is.
      #
      # @param step_result [Hash] The step execution result
      #   - :success [Boolean] Whether the step succeeded
      #   - :result [Hash] The step handler's return value
      #   - :metadata [Hash] Execution metadata
      # @param event_declaration [Hash] The event declaration from YAML
      #   - :name [String] The event name (e.g., "order.processed")
      #   - :delivery_mode [String] "durable" or "fast"
      #   - :condition [String] "success", "failure", or "always"
      # @param step_context [Hash] The step execution context
      #   - :task [Hash] Task information
      #   - :workflow_step [Hash] Workflow step information
      #   - :step_definition [Hash] Step definition from YAML
      #
      # @return [Hash] The business event payload to publish
      def transform_payload(step_result, _event_declaration, _step_context = nil)
        step_result[:result] || {}
      end

      # Determine if this event should be published
      #
      # Override this to add custom publishing conditions beyond the
      # YAML `condition:` field. The YAML condition is evaluated first,
      # then this method is called.
      #
      # @param step_result [Hash] The step execution result
      # @param event_declaration [Hash] The event declaration from YAML
      # @param step_context [Hash] The step execution context
      #
      # @return [Boolean] true if the event should be published
      def should_publish?(_step_result, _event_declaration, _step_context = nil)
        true
      end

      # Add additional metadata to the event
      #
      # Override this to add custom metadata fields to the event.
      # Default returns empty hash.
      #
      # @param step_result [Hash] The step execution result
      # @param event_declaration [Hash] The event declaration from YAML
      # @param step_context [Hash] The step execution context
      #
      # @return [Hash] Additional metadata to merge into event metadata
      def additional_metadata(_step_result, _event_declaration, _step_context = nil)
        {}
      end

      # Hook called before publishing
      #
      # Override for pre-publish validation, logging, or metrics.
      # Raise an exception to prevent publishing.
      #
      # @param event_name [String] The event name
      # @param payload [Hash] The transformed payload
      # @param metadata [Hash] The event metadata
      def before_publish(event_name, _payload, _metadata)
        logger.debug "Publishing event: #{event_name}"
      end

      # Hook called after successful publishing
      #
      # Override for post-publish logging, metrics, or cleanup.
      #
      # @param event_name [String] The event name
      # @param payload [Hash] The transformed payload
      # @param metadata [Hash] The event metadata
      def after_publish(event_name, _payload, _metadata)
        logger.debug "Event published: #{event_name}"
      end

      # Hook called if publishing fails
      #
      # Override for error handling, logging, or fallback behavior.
      # Default logs the error but does not re-raise.
      #
      # @param event_name [String] The event name
      # @param error [Exception] The error that occurred
      # @param payload [Hash] The transformed payload
      def on_publish_error(event_name, error, _payload)
        logger.error "Failed to publish event #{event_name}: #{error.message}"
      end

      # ========================================================================
      # CROSS-LANGUAGE STANDARD API (TAS-96)
      # ========================================================================

      # Cross-language standard: Publish an event with unified context
      #
      # This method coordinates the existing hooks (should_publish?, transform_payload,
      # additional_metadata, before_publish, after_publish) into a single publish call.
      #
      # @param ctx [Hash] Step event context containing:
      #   - :event_name [String] The event name (e.g., "payment.processed")
      #   - :step_result [Hash] The step execution result
      #   - :event_declaration [Hash] The event declaration from YAML
      #   - :step_context [Hash, TaskerCore::Types::StepContext] Step execution context
      # @return [Boolean] true if event was published, false if skipped
      #
      # @example Publishing with context
      #   ctx = {
      #     event_name: 'payment.processed',
      #     step_result: { success: true, result: { payment_id: '123' } },
      #     event_declaration: { name: 'payment.processed', delivery_mode: 'durable' },
      #     step_context: step_context
      #   }
      #   publisher.publish(ctx)
      def publish(ctx)
        event_name = ctx[:event_name]
        step_result = ctx[:step_result]
        event_declaration = ctx[:event_declaration] || {}
        step_context = ctx[:step_context]

        # Check publishing conditions
        return false unless should_publish?(step_result, event_declaration, step_context)

        # Transform payload
        payload = transform_payload(step_result, event_declaration, step_context)

        # Build metadata
        base_metadata = {
          publisher: name,
          published_at: Time.now.utc.iso8601
        }

        # Add step context info if available
        if step_context.respond_to?(:task_uuid)
          base_metadata[:task_uuid] = step_context.task_uuid
          base_metadata[:step_uuid] = step_context.step_uuid
          base_metadata[:step_name] = step_context.step_name if step_context.respond_to?(:step_name)
          base_metadata[:namespace] = step_context.namespace_name if step_context.respond_to?(:namespace_name)
        end

        metadata = base_metadata.merge(additional_metadata(step_result, event_declaration, step_context))

        begin
          # Pre-publish hook
          before_publish(event_name, payload, metadata)

          # Actual publishing is handled by the event router/bridge
          # This method just prepares and validates the event
          # Subclasses can override to perform actual publishing

          # Post-publish hook
          after_publish(event_name, payload, metadata)

          true
        rescue StandardError => e
          on_publish_error(event_name, e, payload)
          false
        end
      end
    end
  end
end
