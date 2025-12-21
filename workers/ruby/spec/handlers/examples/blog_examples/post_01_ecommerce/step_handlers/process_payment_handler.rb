# frozen_string_literal: true

module Ecommerce
  module StepHandlers
    class ProcessPaymentHandler < TaskerCore::StepHandler::Base
      def call(context)
        # Extract and validate all required inputs
        payment_inputs = extract_and_validate_inputs(context)

        logger.info "💳 ProcessPaymentHandler: Processing payment - task_uuid=#{context.task_uuid}, amount=$#{payment_inputs[:amount_to_charge]}"

        # Simulate payment processing - this is the core integration
        payment_result = simulate_payment_processing(
          amount: payment_inputs[:amount_to_charge],
          method: payment_inputs[:payment_method],
          token: payment_inputs[:payment_token]
        )

        # Ensure the payment was successful, handling different error types appropriately
        ensure_payment_successful!(payment_result)

        logger.info "✅ ProcessPaymentHandler: Payment processed - payment_id=#{payment_result[:payment_id]}, amount_charged=$#{payment_result[:amount_charged]}"

        # Return standardized StepHandlerCallResult
        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            payment_id: payment_result[:payment_id],
            amount_charged: payment_result[:amount_charged],
            currency: payment_result[:currency] || 'USD',
            payment_method_type: payment_result[:payment_method_type] || 'credit_card',
            transaction_id: payment_result[:transaction_id],
            processed_at: Time.now.utc.iso8601,
            status: 'completed'
          },
          metadata: {
            operation: 'process_payment',
            execution_hints: {
              payment_gateway: 'mock',
              amount_charged: payment_result[:amount_charged],
              fees: payment_result[:fees],
              processing_time_ms: 150
            },
            http_headers: {
              'X-Payment-Gateway' => 'SimulatedPaymentService',
              'X-Payment-ID' => payment_result[:payment_id],
              'X-Transaction-ID' => payment_result[:transaction_id]
            },
            input_refs: {
              amount: 'sequence.validate_cart.result.total',
              payment_info: 'context.task.context.payment_info'
            }
          }
        )
      rescue StandardError => e
        logger.error "❌ ProcessPaymentHandler: Payment processing error - #{e.class.name}: #{e.message}"
        raise
      end

      private

      # Simulate payment processing with realistic responses
      def simulate_payment_processing(amount:, method:, token:)
        # Simulate different payment outcomes based on token
        case token
        when 'tok_test_declined'
          # Simulate card declined
          {
            status: 'card_declined',
            error: 'Card was declined by issuer',
            payment_id: nil,
            transaction_id: nil
          }
        when 'tok_test_insufficient_funds'
          # Simulate insufficient funds
          {
            status: 'insufficient_funds',
            error: 'Insufficient funds',
            payment_id: nil,
            transaction_id: nil
          }
        when 'tok_test_network_error'
          # Simulate network timeout
          {
            status: 'timeout',
            error: 'Payment gateway timeout',
            payment_id: nil,
            transaction_id: nil
          }
        else
          # Success case
          {
            payment_id: "pay_#{SecureRandom.hex(12)}",
            status: 'succeeded',
            amount_charged: amount,
            currency: 'USD',
            payment_method_type: method,
            transaction_id: "txn_#{SecureRandom.hex(12)}",
            fees: calculate_payment_fees(amount)
          }
        end
      end

      # Calculate payment processing fees
      def calculate_payment_fees(amount)
        processing_fee = (amount * 0.029).round(2) # 2.9%
        fixed_fee = 0.30
        {
          processing_fee: processing_fee,
          fixed_fee: fixed_fee,
          total_fees: processing_fee + fixed_fee
        }
      end

      # Extract and validate all required inputs for payment processing
      def extract_and_validate_inputs(context)
        # Normalize all hash keys to symbols for consistent access
        task_context = context.task.context.deep_symbolize_keys
        payment_info = task_context[:payment_info]
        cart_validation = context.get_dependency_result('validate_cart')

        # Ensure cart_validation is symbolized
        cart_validation = cart_validation.deep_symbolize_keys if cart_validation

        # Early validation - throw PermanentError for missing required data
        payment_method = payment_info&.dig(:method)
        payment_token = payment_info&.dig(:token)
        amount_to_charge = cart_validation&.dig(:total)

        unless payment_method
          raise TaskerCore::Errors::PermanentError.new(
            'Payment method is required but was not provided',
            error_code: 'MISSING_PAYMENT_METHOD'
          )
        end

        unless payment_token
          raise TaskerCore::Errors::PermanentError.new(
            'Payment token is required but was not provided',
            error_code: 'MISSING_PAYMENT_TOKEN'
          )
        end

        unless amount_to_charge
          raise TaskerCore::Errors::PermanentError.new(
            'Cart total is required but was not found from validate_cart step',
            error_code: 'MISSING_CART_TOTAL'
          )
        end

        # Validate payment amount matches cart total
        expected_amount = amount_to_charge.to_f
        provided_amount = payment_info[:amount].to_f

        if (provided_amount - expected_amount).abs > 0.01 # Allow for floating point rounding
          raise TaskerCore::Errors::PermanentError.new(
            "Payment amount mismatch. Expected: $#{expected_amount}, Provided: $#{provided_amount}",
            error_code: 'PAYMENT_AMOUNT_MISMATCH',
            context: {
              expected_amount: expected_amount,
              provided_amount: provided_amount,
              difference: (provided_amount - expected_amount).round(2)
            }
          )
        end

        {
          amount_to_charge: amount_to_charge,
          payment_method: payment_method,
          payment_token: payment_token,
          expected_amount: expected_amount,
          provided_amount: provided_amount
        }
      end

      # Ensure payment was successful, intelligently handling different error types
      def ensure_payment_successful!(payment_result)
        case payment_result[:status]
        when 'succeeded'
          # Success - continue processing
          nil
        when 'insufficient_funds', 'card_declined', 'invalid_card'
          # These are permanent customer/card issues - don't retry
          error_message = payment_result[:error] || 'Payment declined'
          raise TaskerCore::Errors::PermanentError.new(
            "Payment declined: #{error_message}",
            error_code: 'PAYMENT_DECLINED',
            context: { payment_status: payment_result[:status] }
          )
        when 'rate_limited'
          # Temporary issue - can be retried
          raise TaskerCore::Errors::RetryableError.new(
            'Payment service rate limited',
            retry_after: 30,
            context: { payment_status: payment_result[:status] }
          )
        when 'service_unavailable', 'timeout'
          # Temporary service issues - can be retried
          raise TaskerCore::Errors::RetryableError.new(
            'Payment service temporarily unavailable',
            retry_after: 15,
            context: { payment_status: payment_result[:status] }
          )
        else
          # Unknown status - treat as retryable for safety
          error_message = payment_result[:error] || 'Unknown payment error'
          logger.error "❓ ProcessPaymentHandler: Unknown payment status - #{payment_result.inspect}"
          raise TaskerCore::Errors::RetryableError.new(
            "Payment service returned unknown status: #{error_message}",
            retry_after: 30,
            context: { payment_status: payment_result[:status], payment_result: payment_result }
          )
        end
      end
    end
  end
end
