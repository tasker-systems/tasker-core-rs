# frozen_string_literal: true

module Payments
  module StepHandlers
    class NotifyCustomerHandler < TaskerCore::StepHandler::Base
      def call(context)
        # Extract and validate inputs
        inputs = extract_and_validate_inputs(context)

        logger.info "📧 NotifyCustomerHandler: Sending refund confirmation - task_uuid=#{context.task_uuid}, customer_email=#{inputs[:customer_email]}"

        # Simulate sending notification
        notification_result = simulate_notification_sending(inputs)

        # Ensure notification was sent successfully
        ensure_notification_sent!(notification_result)

        logger.info "✅ NotifyCustomerHandler: Notification sent - message_id=#{notification_result[:message_id]}, recipient=#{inputs[:customer_email]}"

        # Return standardized StepHandlerCallResult
        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            notification_sent: true,
            customer_email: inputs[:customer_email],
            message_id: notification_result[:message_id],
            notification_type: 'refund_confirmation',
            sent_at: notification_result[:sent_at],
            delivery_status: notification_result[:delivery_status],
            refund_id: inputs[:refund_id],
            refund_amount: inputs[:refund_amount],
            namespace: 'payments'
          },
          metadata: {
            operation: 'notify_customer',
            execution_hints: {
              customer_email: inputs[:customer_email],
              message_id: notification_result[:message_id],
              notification_type: 'refund_confirmation'
            },
            http_headers: {
              'X-Notification-Service' => 'MockEmailService',
              'X-Message-ID' => notification_result[:message_id],
              'X-Recipient' => inputs[:customer_email]
            },
            input_refs: {
              refund_result: 'sequence.process_gateway_refund.result',
              payment_records: 'sequence.update_payment_records.result'
            }
          }
        )
      rescue StandardError => e
        logger.error "❌ NotifyCustomerHandler: Notification failed - #{e.class.name}: #{e.message}"
        raise
      end

      private

      # Extract and validate inputs from task and previous steps
      def extract_and_validate_inputs(context)
        task_context = context.task.context.deep_symbolize_keys

        # Get refund results
        refund_result = context.get_dependency_result('process_gateway_refund')
        refund_result = refund_result.deep_symbolize_keys if refund_result

        unless refund_result&.dig(:refund_processed)
          raise TaskerCore::Errors::PermanentError.new(
            'Refund must be processed before sending notification',
            error_code: 'MISSING_REFUND'
          )
        end

        # Get customer email from context
        customer_email = task_context[:customer_email]
        unless customer_email
          raise TaskerCore::Errors::PermanentError.new(
            'Customer email is required for notification',
            error_code: 'MISSING_CUSTOMER_EMAIL'
          )
        end

        # Validate email format
        unless customer_email.match?(/\A[^@\s]+@[^@\s]+\z/)
          raise TaskerCore::Errors::PermanentError.new(
            "Invalid customer email format: #{customer_email}",
            error_code: 'INVALID_EMAIL_FORMAT'
          )
        end

        {
          customer_email: customer_email,
          refund_id: refund_result[:refund_id],
          refund_amount: refund_result[:refund_amount],
          payment_id: refund_result[:payment_id],
          estimated_arrival: refund_result[:estimated_arrival],
          refund_reason: task_context[:refund_reason] || 'customer_request'
        }
      end

      # Simulate notification sending (self-contained)
      def simulate_notification_sending(inputs)
        email = inputs[:customer_email]

        # Simulate different notification scenarios
        case email
        when /@test_bounce/
          { status: 'bounced', error: 'Email bounced' }
        when /@test_invalid/
          { status: 'invalid', error: 'Invalid email address' }
        when /@test_rate_limit/
          { status: 'rate_limited', error: 'Rate limit exceeded' }
        else
          # Success case
          message_id = "msg_#{SecureRandom.hex(12)}"
          {
            status: 'sent',
            message_id: message_id,
            delivery_status: 'delivered',
            recipient: email,
            sent_at: Time.now.utc.iso8601,
            notification_type: 'refund_confirmation',
            subject: "Your refund of $#{format('%.2f', inputs[:refund_amount] / 100.0)} has been processed",
            template_id: 'refund_confirmation_v2'
          }
        end
      end

      # Ensure notification was sent successfully
      def ensure_notification_sent!(notification_result)
        status = notification_result[:status]

        case status
        when 'sent', 'delivered', 'queued'
          # Notification successful
          nil
        when 'bounced'
          # Permanent error - email bounced
          raise TaskerCore::Errors::PermanentError.new(
            'Customer email bounced',
            error_code: 'EMAIL_BOUNCED'
          )
        when 'invalid'
          # Permanent error - invalid email
          raise TaskerCore::Errors::PermanentError.new(
            'Invalid customer email address',
            error_code: 'INVALID_EMAIL'
          )
        when 'rate_limited'
          # Temporary error - rate limit
          raise TaskerCore::Errors::RetryableError.new(
            'Email service rate limited, will retry',
            retry_after: 60
          )
        when 'failed', 'error'
          # Temporary error - service issue
          raise TaskerCore::Errors::RetryableError.new(
            "Notification failed: #{notification_result[:error]}",
            retry_after: 30
          )
        else
          # Unknown status - treat as retryable
          raise TaskerCore::Errors::RetryableError.new(
            "Unknown notification status: #{status}",
            retry_after: 30
          )
        end
      end
    end
  end
end
