# frozen_string_literal: true

# TAS-65: Custom publisher for notification-related domain events
#
# Demonstrates fast delivery mode with custom payload enrichment.
# This publisher covers the "fast + custom publisher" case (2d) in the
# 2x2 test matrix.
#
# ## 2x2 Test Matrix Coverage
#
# - Delivery mode: fast (in-memory)
# - Publisher: custom (NotificationEventPublisher)
#
# ## Features
#
# - Channel-specific payload enrichment (email, SMS, push)
# - Delivery status tracking
# - Lightweight event publishing (fast mode)
#
# @example YAML declaration
#   steps:
#     - name: send_notification
#       publishes_events:
#         - name: notification.sent
#           condition: success
#           delivery_mode: fast
#           publisher: DomainEvents::Publishers::NotificationEventPublisher
#
module DomainEvents
  module Publishers
    class NotificationEventPublisher < TaskerCore::DomainEvents::BasePublisher
      # The publisher name used for registry lookup
      # Must match the `publisher:` field in YAML
      #
      # @return [String] The publisher name
      def name
        'DomainEvents::Publishers::NotificationEventPublisher'
      end

      # Transform step result into notification event payload
      #
      # Adds channel-specific enrichment for email, SMS, and push notifications.
      #
      # @param step_result [Hash] The step execution result
      # @param event_declaration [Hash] The event declaration from YAML
      # @param step_context [Hash] The step execution context
      #
      # @return [Hash] The enriched notification event payload
      def transform_payload(step_result, event_declaration, step_context = nil)
        result = step_result[:result] || {}
        channel = result[:channel] || result['channel'] || 'unknown'

        payload = {
          notification_id: result[:notification_id] || result['notification_id'] || 'unknown',
          channel: channel,
          recipient: result[:recipient] || result['recipient'] || 'unknown',
          sent_at: result[:sent_at] || result['sent_at'] || Time.now.iso8601,
          status: result[:status] || result['status'] || 'unknown',
          delivery_mode: 'fast',
          publisher: 'DomainEvents::Publishers::NotificationEventPublisher'
        }

        # Add channel-specific metadata
        case channel.to_s
        when 'email'
          payload[:email_metadata] = {
            has_attachments: false,
            content_type: 'text/html',
            priority: 'normal'
          }
        when 'sms'
          payload[:sms_metadata] = {
            message_segments: 1,
            carrier: 'unknown'
          }
        when 'push'
          payload[:push_metadata] = {
            platform: 'all',
            badge_count: 1,
            sound: 'default'
          }
        end

        payload
      end

      # Determine if this event should be published
      #
      # Only publishes for successful notification deliveries.
      #
      # @param step_result [Hash] The step execution result
      # @param event_declaration [Hash] The event declaration from YAML
      # @param step_context [Hash] The step execution context
      #
      # @return [Boolean] true if the event should be published
      def should_publish?(step_result, event_declaration, step_context = nil)
        # Only publish on success
        return false unless step_result[:success]

        # Verify we have notification data
        result = step_result[:result] || {}
        notification_id = result[:notification_id] || result['notification_id']

        notification_id.present?
      end

      # Add execution metrics to event metadata
      #
      # @param step_result [Hash] The step execution result
      # @param event_declaration [Hash] The event declaration from YAML
      # @param step_context [Hash] The step execution context
      #
      # @return [Hash] Additional metadata
      def additional_metadata(step_result, event_declaration, step_context = nil)
        metadata = step_result[:metadata] || {}
        {
          execution_time_ms: metadata[:execution_time_ms] || metadata['execution_time_ms'],
          publisher_type: 'custom',
          publisher_name: 'DomainEvents::Publishers::NotificationEventPublisher'
        }
      end

      # Hook called before publishing
      #
      # @param event_name [String] The event name
      # @param payload [Hash] The transformed payload
      # @param metadata [Hash] The event metadata
      def before_publish(event_name, payload, metadata)
        logger.debug "[DomainEvents::Publishers::NotificationEventPublisher] Publishing #{event_name} via fast delivery"
        logger.debug "[DomainEvents::Publishers::NotificationEventPublisher] Channel: #{payload[:channel]}, Recipient: #{payload[:recipient]}"
      end

      # Hook called after successful publishing
      #
      # @param event_name [String] The event name
      # @param payload [Hash] The transformed payload
      # @param metadata [Hash] The event metadata
      def after_publish(event_name, payload, metadata)
        logger.info "[DomainEvents::Publishers::NotificationEventPublisher] Published #{event_name} (fast + custom publisher)"
      end
    end
  end
end
