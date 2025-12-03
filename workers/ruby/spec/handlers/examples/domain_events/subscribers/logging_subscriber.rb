# frozen_string_literal: true

# TAS-65: Example logging subscriber for fast/in-process domain events
#
# Demonstrates how to create a simple subscriber that logs all events
# matching a pattern. Useful for debugging and audit trails.
#
# @example Register in bootstrap
#   registry = TaskerCore::DomainEvents::SubscriberRegistry.instance
#   registry.register(DomainEvents::Subscribers::LoggingSubscriber)
#   registry.start_all!
#
# @example Output
#   INFO [LoggingSubscriber] Event: payment.processed
#     Task: 550e8400-e29b-41d4-a716-446655440000
#     Step: process_payment
#     Namespace: payments
#     Correlation: 7c9e6679-7425-40de-944b-e07fc1f90ae7
#
module DomainEvents
  module Subscribers
    class LoggingSubscriber < TaskerCore::DomainEvents::BaseSubscriber
      # Subscribe to all events
      subscribes_to '*'

      # Handle any domain event by logging its details
      #
      # @param event [Hash] The domain event
      def handle(event)
        event_name = event[:event_name]
        metadata = event[:metadata] || {}

        logger.info "[LoggingSubscriber] Event: #{event_name}"
        logger.info "  Task: #{metadata[:task_uuid]}"
        logger.info "  Step: #{metadata[:step_name]}"
        logger.info "  Namespace: #{metadata[:namespace]}"
        logger.info "  Correlation: #{metadata[:correlation_id]}"
      end
    end

    # Debug-level logging subscriber
    #
    # Same as LoggingSubscriber but logs at DEBUG level.
    # Useful for verbose environments where INFO is too noisy.
    class DebugLoggingSubscriber < TaskerCore::DomainEvents::BaseSubscriber
      subscribes_to '*'

      def handle(event)
        event_name = event[:event_name]
        metadata = event[:metadata] || {}

        logger.debug "[DebugLoggingSubscriber] Event: #{event_name}"
        logger.debug "  Task: #{metadata[:task_uuid]}"
        logger.debug "  Step: #{metadata[:step_name]}"
      end
    end

    # Verbose logging subscriber that includes payload
    #
    # Logs full event details including business payload.
    # Use with caution in production as payloads may contain sensitive data.
    class VerboseLoggingSubscriber < TaskerCore::DomainEvents::BaseSubscriber
      subscribes_to '*'

      # Maximum characters to log from payload
      MAX_PAYLOAD_LENGTH = 500

      def handle(event)
        event_name = event[:event_name]
        metadata = event[:metadata] || {}
        payload = event[:business_payload] || {}

        # Truncate payload for logging
        payload_str = payload.to_json
        payload_preview = if payload_str.length > MAX_PAYLOAD_LENGTH
                            "#{payload_str[0...MAX_PAYLOAD_LENGTH]}...(truncated)"
                          else
                            payload_str
                          end

        logger.info "[VerboseLoggingSubscriber] Event: #{event_name}"
        logger.info "  Task: #{metadata[:task_uuid]}"
        logger.info "  Step: #{metadata[:step_name]}"
        logger.info "  Namespace: #{metadata[:namespace]}"
        logger.info "  Correlation: #{metadata[:correlation_id]}"
        logger.info "  Payload: #{payload_preview}"
      end
    end
  end
end
