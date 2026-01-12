# frozen_string_literal: true

module Ecommerce
  module StepHandlers
    # TAS-137 Best Practices Demonstrated:
    # - get_input() for task context field access (cross-language standard)
    # - get_dependency_result() for upstream step results (auto-unwraps)
    # - get_dependency_field() for nested field extraction from dependencies
    class SendConfirmationHandler < TaskerCore::StepHandler::Base
      def call(context)
        # Extract and validate all required inputs
        email_inputs = extract_and_validate_inputs(context)

        logger.info "📧 SendConfirmationHandler: Sending confirmation email - task_uuid=#{context.task_uuid}, recipient=#{email_inputs[:customer_info][:email]}"

        # Send confirmation email - this is the core integration
        email_response = send_confirmation_email(email_inputs)

        delivery_result = email_response[:delivery_result]
        customer_email = email_response[:customer_email]

        logger.info "✅ SendConfirmationHandler: Confirmation email sent - recipient=#{customer_email}"

        # Return standardized StepHandlerCallResult
        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            email_sent: true,
            recipient: customer_email,
            email_type: 'order_confirmation',
            sent_at: Time.now.utc.iso8601,
            message_id: delivery_result[:message_id] || "mock_#{SecureRandom.hex(8)}"
          },
          metadata: {
            operation: 'send_confirmation_email',
            execution_hints: {
              recipient: customer_email,
              email_type: 'order_confirmation',
              delivery_status: delivery_result[:status],
              sent_timestamp: email_response[:sent_timestamp]
            },
            http_headers: {
              'X-Email-Service' => 'MockEmailService',
              'X-Message-ID' => delivery_result[:message_id] || 'N/A',
              'X-Recipient' => customer_email
            },
            input_refs: {
              customer_info: 'context.get_input("customer_info")',
              order_result: 'context.get_dependency_result("create_order")',
              cart_validation: 'context.get_dependency_result("validate_cart")'
            }
          }
        )
      rescue StandardError => e
        logger.error "❌ SendConfirmationHandler: Email sending failed - #{e.class.name}: #{e.message}"
        raise
      end

      private

      # Extract and validate all required inputs for email sending
      def extract_and_validate_inputs(context)
        # TAS-137: Use get_input() for task context access (cross-language standard)
        customer_info = context.get_input('customer_info')
        customer_info = customer_info.deep_symbolize_keys if customer_info

        # TAS-137: Use get_dependency_result() for upstream step results (auto-unwraps)
        order_result = context.get_dependency_result('create_order')
        order_result = order_result.deep_symbolize_keys if order_result

        cart_validation = context.get_dependency_result('validate_cart')
        cart_validation = cart_validation.deep_symbolize_keys if cart_validation

        unless customer_info&.dig(:email)
          raise TaskerCore::Errors::PermanentError.new(
            'Customer email is required but was not provided',
            error_code: 'MISSING_CUSTOMER_EMAIL'
          )
        end

        unless order_result&.dig(:order_id)
          raise TaskerCore::Errors::PermanentError.new(
            'Order results are required but were not found from create_order step',
            error_code: 'MISSING_ORDER_RESULT'
          )
        end

        unless cart_validation&.dig(:validated_items)&.any?
          raise TaskerCore::Errors::PermanentError.new(
            'Cart validation results are required but were not found from validate_cart step',
            error_code: 'MISSING_CART_VALIDATION'
          )
        end

        {
          customer_info: customer_info,
          order_result: order_result,
          cart_validation: cart_validation
        }
      end

      # Send confirmation email using validated inputs
      def send_confirmation_email(email_inputs)
        customer_info = email_inputs[:customer_info]
        order_result = email_inputs[:order_result]
        cart_validation = email_inputs[:cart_validation]

        # Prepare email data
        email_data = {
          to: customer_info[:email],
          customer_name: customer_info[:name],
          order_number: order_result[:order_number],
          order_id: order_result[:order_id],
          total_amount: order_result[:total_amount],
          estimated_delivery: order_result[:estimated_delivery],
          items: cart_validation[:validated_items],
          order_url: "https://example.com/orders/#{order_result[:order_id]}"
        }

        # Simulate email sending
        delivery_result = simulate_email_delivery(email_data)

        # Ensure email was sent successfully
        ensure_email_sent_successfully!(delivery_result)

        {
          delivery_result: delivery_result,
          customer_email: customer_info[:email],
          email_data: email_data,
          sent_timestamp: Time.now.utc.iso8601
        }
      end

      # Simulate email delivery with realistic responses
      def simulate_email_delivery(email_data)
        # Validate email address format
        email = email_data[:to]
        if email.nil? || !email.match?(/\A[^@\s]+@[^@\s]+\z/)
          return {
            status: 'invalid_email',
            error: 'Invalid email address',
            email: email
          }
        end

        # Success case
        {
          status: 'sent',
          message_id: "msg_#{SecureRandom.hex(12)}",
          email: email,
          sent_at: Time.now.utc.iso8601
        }
      end

      # Ensure email was sent successfully, handling different error types
      def ensure_email_sent_successfully!(delivery_result)
        case delivery_result[:status]
        when 'sent', 'delivered'
          # Success - continue processing
          nil
        when 'rate_limited'
          # Temporary issue - can be retried
          raise TaskerCore::Errors::RetryableError.new(
            'Email service rate limited',
            retry_after: 60,
            context: { delivery_status: 'rate_limited' }
          )
        when 'service_unavailable', 'timeout'
          # Temporary service issues - can be retried
          raise TaskerCore::Errors::RetryableError.new(
            'Email service temporarily unavailable',
            retry_after: 30,
            context: { delivery_status: delivery_result[:status] }
          )
        when 'invalid_email'
          # Permanent issue - bad email address
          raise TaskerCore::Errors::PermanentError.new(
            'Invalid email address provided',
            error_code: 'INVALID_EMAIL',
            context: { email: delivery_result[:email] }
          )
        else
          # Unknown status or generic error - treat as retryable for email delivery
          error_message = delivery_result[:error] || 'Unknown email delivery error'
          raise TaskerCore::Errors::RetryableError.new(
            "Email delivery failed: #{error_message}",
            retry_after: 30,
            context: {
              delivery_status: delivery_result[:status],
              delivery_result: delivery_result
            }
          )
        end
      end
    end
  end
end
