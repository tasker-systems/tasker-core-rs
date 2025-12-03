# frozen_string_literal: true

# TAS-65: Custom publisher for payment-related domain events
#
# Demonstrates durable delivery mode with custom payload enrichment.
# This publisher covers the "durable + custom publisher" case (2b) in the
# 2x2 test matrix.
#
# ## 2x2 Test Matrix Coverage
#
# - Delivery mode: durable (PGMQ)
# - Publisher: custom (PaymentEventPublisher)
#
# ## Features
#
# - Conditional event publishing based on result status
# - Payload enrichment with business logic
# - Multiple events from a single step (success/failure)
# - Analytics event for all outcomes
#
# @example YAML declaration
#   steps:
#     - name: process_payment
#       publishes_events:
#         - name: payment.processed
#           condition: success
#           delivery_mode: durable
#           publisher: DomainEvents::Publishers::PaymentEventPublisher
#         - name: payment.failed
#           condition: failure
#           delivery_mode: durable
#           publisher: DomainEvents::Publishers::PaymentEventPublisher
#
module DomainEvents
  module Publishers
    class PaymentEventPublisher < TaskerCore::DomainEvents::BasePublisher
      # The publisher name used for registry lookup
      # Must match the `publisher:` field in YAML
      #
      # @return [String] The publisher name
      def name
        'DomainEvents::Publishers::PaymentEventPublisher'
      end

      # Transform step result into payment event payload
      #
      # Handles both success and failure cases with appropriate payloads.
      #
      # @param step_result [Hash] The step execution result
      # @param event_declaration [Hash] The event declaration from YAML
      # @param step_context [Hash] The step execution context
      #
      # @return [Hash] The enriched payment event payload
      def transform_payload(step_result, event_declaration, step_context = nil)
        result = step_result[:result] || {}
        event_name = event_declaration[:name] || event_declaration['name']

        if step_result[:success] && event_name&.include?('processed')
          build_success_payload(result, step_result, step_context)
        elsif !step_result[:success] && event_name&.include?('failed')
          build_failure_payload(result, step_result, step_context)
        else
          # Fallback for analytics or other events
          result
        end
      end

      # Determine if this event should be published
      #
      # Only publishes for payment-related steps with valid transaction data.
      #
      # @param step_result [Hash] The step execution result
      # @param event_declaration [Hash] The event declaration from YAML
      # @param step_context [Hash] The step execution context
      #
      # @return [Boolean] true if the event should be published
      def should_publish?(step_result, event_declaration, step_context = nil)
        result = step_result[:result] || {}
        event_name = event_declaration[:name] || event_declaration['name']

        # For success events, verify we have transaction data
        if event_name&.include?('processed')
          return step_result[:success] && (result[:transaction_id] || result['transaction_id']).present?
        end

        # For failure events, verify we have error info
        if event_name&.include?('failed')
          metadata = step_result[:metadata] || {}
          return !step_result[:success] && (metadata[:error_code] || metadata['error_code']).present?
        end

        # Default: always publish
        true
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
          publisher_name: 'DomainEvents::Publishers::PaymentEventPublisher',
          payment_provider: metadata[:payment_provider] || metadata['payment_provider']
        }
      end

      # Hook called before publishing
      #
      # @param event_name [String] The event name
      # @param payload [Hash] The transformed payload
      # @param metadata [Hash] The event metadata
      def before_publish(event_name, payload, metadata)
        logger.debug "[DomainEvents::Publishers::PaymentEventPublisher] Publishing #{event_name} via durable delivery"
      end

      # Hook called after successful publishing
      #
      # @param event_name [String] The event name
      # @param payload [Hash] The transformed payload
      # @param metadata [Hash] The event metadata
      def after_publish(event_name, payload, metadata)
        logger.info "[DomainEvents::Publishers::PaymentEventPublisher] Published #{event_name} (durable + custom publisher)"
      end

      private

      # Build payload for successful payment
      #
      # @param result [Hash] Step result data
      # @param step_result [Hash] Full step result
      # @param step_context [Hash] Step execution context
      # @return [Hash] Success event payload
      def build_success_payload(result, step_result, step_context)
        {
          transaction_id: result[:transaction_id] || result['transaction_id'],
          amount: result[:amount] || result['amount'],
          currency: result[:currency] || result['currency'] || 'USD',
          payment_method: result[:payment_method] || result['payment_method'] || 'credit_card',
          processed_at: result[:processed_at] || result['processed_at'] || Time.now.iso8601,
          customer_tier: determine_customer_tier(step_context),
          delivery_mode: 'durable',
          publisher: 'DomainEvents::Publishers::PaymentEventPublisher'
        }
      end

      # Build payload for failed payment
      #
      # @param result [Hash] Step result data
      # @param step_result [Hash] Full step result
      # @param step_context [Hash] Step execution context
      # @return [Hash] Failure event payload
      def build_failure_payload(result, step_result, step_context)
        metadata = step_result[:metadata] || {}
        {
          error_code: metadata[:error_code] || metadata['error_code'] || 'UNKNOWN',
          error_type: metadata[:error_type] || metadata['error_type'] || 'PaymentError',
          error_message: metadata[:error_message] || metadata['error_message'] || 'Payment failed',
          order_id: result[:order_id] || result['order_id'],
          failed_at: Time.now.iso8601,
          retry_after_seconds: calculate_retry_delay(metadata),
          is_retryable: metadata[:retryable] || metadata['retryable'] || false,
          delivery_mode: 'durable',
          publisher: 'DomainEvents::Publishers::PaymentEventPublisher'
        }
      end

      # Determine customer tier from task context
      #
      # @param step_context [Hash] Step execution context
      # @return [String] Customer tier
      def determine_customer_tier(step_context)
        return 'standard' unless step_context

        task = step_context[:task] || step_context['task'] || {}
        context = task[:context] || task['context'] || {}
        context[:customer_tier] || context['customer_tier'] || 'standard'
      end

      # Calculate retry delay based on error code
      #
      # @param metadata [Hash] Step metadata
      # @return [Integer] Retry delay in seconds
      def calculate_retry_delay(metadata)
        error_code = metadata[:error_code] || metadata['error_code'] || ''

        case error_code.to_s
        when 'RATE_LIMITED' then 600       # 10 minutes
        when 'TEMPORARY_FAILURE' then 300  # 5 minutes
        when 'NETWORK_ERROR' then 60       # 1 minute
        when 'CARD_DECLINED' then 0        # No retry (user action needed)
        else 120                           # 2 minutes default
        end
      end
    end
  end
end
