# frozen_string_literal: true

module Payments
  module StepHandlers
    class ValidatePaymentEligibilityHandler < TaskerCore::StepHandler::Base
      def call(context)
        # Extract and validate inputs using Phase 1 pattern
        inputs = extract_and_validate_inputs(context)

        logger.info "💳 ValidatePaymentEligibilityHandler: Validating payment eligibility - task_uuid=#{context.task_uuid}, payment_id=#{inputs[:payment_id]}"

        # Simulate payment gateway validation
        validation_result = simulate_payment_gateway_validation(inputs)

        # Validate the response indicates eligibility
        ensure_payment_eligible!(validation_result)

        logger.info "✅ ValidatePaymentEligibilityHandler: Payment validated - payment_id=#{inputs[:payment_id]}, status=#{validation_result[:status]}"

        # Return standardized StepHandlerCallResult
        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            payment_validated: true,
            payment_id: inputs[:payment_id],
            original_amount: validation_result[:original_amount],
            refund_amount: inputs[:refund_amount],
            payment_method: validation_result[:payment_method],
            gateway_provider: validation_result[:gateway_provider],
            eligibility_status: validation_result[:status],
            validation_timestamp: Time.now.utc.iso8601,
            namespace: 'payments'
          },
          metadata: {
            operation: 'validate_payment_eligibility',
            execution_hints: {
              payment_id: inputs[:payment_id],
              gateway_provider: validation_result[:gateway_provider],
              eligibility_status: validation_result[:status]
            },
            http_headers: {
              'X-Payment-Gateway' => validation_result[:gateway_provider],
              'X-Payment-ID' => inputs[:payment_id],
              'X-Eligibility-Status' => validation_result[:status]
            },
            input_refs: {
              payment_id: 'context.task.context.payment_id',
              refund_amount: 'context.task.context.refund_amount'
            }
          }
        )
      rescue StandardError => e
        logger.error "❌ ValidatePaymentEligibilityHandler: Validation failed - #{e.class.name}: #{e.message}"
        raise
      end

      private

      # Extract and validate all required inputs
      def extract_and_validate_inputs(context)
        # Normalize context to symbols early
        task_context = context.task.context.deep_symbolize_keys

        # Validate required fields
        required_fields = %i[payment_id refund_amount]
        missing_fields = required_fields.select { |field| task_context[field].blank? }

        if missing_fields.any?
          raise TaskerCore::Errors::PermanentError.new(
            "Missing required fields for payment validation: #{missing_fields.join(', ')}",
            error_code: 'MISSING_REQUIRED_FIELDS'
          )
        end

        # Validate refund amount is positive
        refund_amount = task_context[:refund_amount]
        if refund_amount <= 0
          raise TaskerCore::Errors::PermanentError.new(
            "Refund amount must be positive, got: #{refund_amount}",
            error_code: 'INVALID_REFUND_AMOUNT'
          )
        end

        # Validate payment ID format (basic validation)
        payment_id = task_context[:payment_id]
        unless payment_id.match?(/^pay_[a-zA-Z0-9_]+$/)
          raise TaskerCore::Errors::PermanentError.new(
            "Invalid payment ID format: #{payment_id}",
            error_code: 'INVALID_PAYMENT_ID'
          )
        end

        {
          payment_id: payment_id,
          refund_amount: refund_amount,
          refund_reason: task_context[:refund_reason],
          partial_refund: task_context[:partial_refund] || false
        }
      end

      # Simulate payment gateway validation (self-contained)
      def simulate_payment_gateway_validation(inputs)
        payment_id = inputs[:payment_id]

        # Simulate different payment scenarios based on payment ID
        case payment_id
        when /pay_test_insufficient/
          {
            status: 'insufficient_funds',
            reason: 'Not enough funds available for refund',
            payment_id: payment_id
          }
        when /pay_test_processing/
          {
            status: 'processing',
            reason: 'Payment still processing',
            payment_id: payment_id
          }
        when /pay_test_ineligible/
          {
            status: 'ineligible',
            reason: 'Payment is past refund window',
            payment_id: payment_id
          }
        else
          # Success case - payment is eligible for refund
          {
            status: 'eligible',
            payment_id: payment_id,
            original_amount: inputs[:refund_amount] + 1000, # Original was higher
            payment_method: 'credit_card',
            gateway_provider: 'MockPaymentGateway',
            transaction_date: (Time.now - 5.days).utc.iso8601,
            refundable_amount: inputs[:refund_amount]
          }
        end
      end

      # Ensure payment is eligible for refund
      def ensure_payment_eligible!(validation_result)
        status = validation_result[:status]

        case status
        when 'eligible'
          # Payment is eligible for refund
          nil
        when 'ineligible'
          # Permanent error - payment cannot be refunded
          raise TaskerCore::Errors::PermanentError.new(
            "Payment is not eligible for refund: #{validation_result[:reason]}",
            error_code: 'PAYMENT_INELIGIBLE'
          )
        when 'processing', 'pending'
          # Temporary state - payment is still processing
          raise TaskerCore::Errors::RetryableError.new(
            'Payment is still processing, cannot refund yet',
            retry_after: 30
          )
        when 'insufficient_funds'
          # Permanent error - not enough funds to refund
          raise TaskerCore::Errors::PermanentError.new(
            'Insufficient funds available for refund',
            error_code: 'INSUFFICIENT_FUNDS'
          )
        else
          # Unknown status - treat as temporary issue
          raise TaskerCore::Errors::RetryableError.new(
            "Unknown payment eligibility status: #{status}",
            retry_after: 30
          )
        end
      end
    end
  end
end
