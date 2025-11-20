# frozen_string_literal: true

module Payments
  module StepHandlers
    class ProcessGatewayRefundHandler < TaskerCore::StepHandler::Base
      def call(task, sequence, _step)
        # Extract and validate inputs
        inputs = extract_and_validate_inputs(task, sequence, _step)

        logger.info "💰 ProcessGatewayRefundHandler: Processing gateway refund - task_uuid=#{task.task_uuid}, payment_id=#{inputs[:payment_id]}"

        # Simulate gateway refund processing
        refund_result = simulate_gateway_refund(inputs)

        # Ensure refund was successful
        ensure_refund_successful!(refund_result)

        logger.info "✅ ProcessGatewayRefundHandler: Refund processed - refund_id=#{refund_result[:refund_id]}, status=#{refund_result[:status]}"

        # Return standardized StepHandlerCallResult
        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            refund_processed: true,
            refund_id: refund_result[:refund_id],
            payment_id: inputs[:payment_id],
            refund_amount: inputs[:refund_amount],
            refund_status: refund_result[:status],
            gateway_transaction_id: refund_result[:gateway_transaction_id],
            gateway_provider: refund_result[:gateway_provider],
            processed_at: refund_result[:processed_at],
            estimated_arrival: refund_result[:estimated_arrival],
            namespace: 'payments'
          },
          metadata: {
            operation: 'process_gateway_refund',
            execution_hints: {
              refund_id: refund_result[:refund_id],
              gateway_provider: refund_result[:gateway_provider],
              refund_status: refund_result[:status]
            },
            http_headers: {
              'X-Gateway-Provider' => refund_result[:gateway_provider],
              'X-Refund-ID' => refund_result[:refund_id],
              'X-Gateway-Transaction-ID' => refund_result[:gateway_transaction_id]
            },
            input_refs: {
              payment_id: 'task.context.payment_id',
              validation_result: 'sequence.validate_payment_eligibility.result'
            }
          }
        )
      rescue StandardError => e
        logger.error "❌ ProcessGatewayRefundHandler: Refund failed - #{e.class.name}: #{e.message}"
        raise
      end

      private

      # Extract and validate inputs from task and previous step
      def extract_and_validate_inputs(task, sequence, _step)
        context = task.context.deep_symbolize_keys

        # Get validation results from previous step
        validation_result = sequence.get_results('validate_payment_eligibility')
        validation_result = validation_result.deep_symbolize_keys if validation_result

        unless validation_result&.dig(:payment_validated)
          raise TaskerCore::Errors::PermanentError.new(
            'Payment validation must be completed before processing refund',
            error_code: 'MISSING_VALIDATION'
          )
        end

        {
          payment_id: validation_result[:payment_id],
          refund_amount: validation_result[:refund_amount],
          refund_reason: context[:refund_reason] || 'customer_request',
          partial_refund: context[:partial_refund] || false,
          original_amount: validation_result[:original_amount]
        }
      end

      # Simulate gateway refund processing (self-contained)
      def simulate_gateway_refund(inputs)
        payment_id = inputs[:payment_id]

        # Simulate different gateway responses
        case payment_id
        when /pay_test_gateway_timeout/
          { status: 'timeout', error: 'Gateway timeout' }
        when /pay_test_gateway_error/
          { status: 'failed', error: 'Gateway error' }
        when /pay_test_rate_limit/
          { status: 'rate_limited', error: 'Rate limit exceeded' }
        else
          # Success case
          refund_id = "rfnd_#{SecureRandom.hex(12)}"
          {
            status: 'processed',
            refund_id: refund_id,
            payment_id: payment_id,
            gateway_transaction_id: "gtx_#{SecureRandom.hex(10)}",
            gateway_provider: 'MockPaymentGateway',
            refund_amount: inputs[:refund_amount],
            processed_at: Time.now.utc.iso8601,
            estimated_arrival: (Time.now + 5.days).utc.iso8601
          }
        end
      end

      # Ensure refund was processed successfully
      def ensure_refund_successful!(refund_result)
        status = refund_result[:status]

        case status
        when 'processed', 'succeeded'
          # Refund successful
          nil
        when 'failed'
          # Permanent error - refund failed
          raise TaskerCore::Errors::PermanentError.new(
            "Gateway refund failed: #{refund_result[:error]}",
            error_code: 'GATEWAY_REFUND_FAILED'
          )
        when 'timeout'
          # Temporary error - gateway timeout
          raise TaskerCore::Errors::RetryableError.new(
            'Gateway timeout, will retry',
            retry_after: 30
          )
        when 'rate_limited'
          # Temporary error - rate limit
          raise TaskerCore::Errors::RetryableError.new(
            'Gateway rate limited, will retry',
            retry_after: 60
          )
        when 'pending'
          # Temporary state - refund pending
          raise TaskerCore::Errors::RetryableError.new(
            'Refund pending, checking again',
            retry_after: 15
          )
        else
          # Unknown status - treat as retryable
          raise TaskerCore::Errors::RetryableError.new(
            "Unknown refund status: #{status}",
            retry_after: 30
          )
        end
      end
    end
  end
end
