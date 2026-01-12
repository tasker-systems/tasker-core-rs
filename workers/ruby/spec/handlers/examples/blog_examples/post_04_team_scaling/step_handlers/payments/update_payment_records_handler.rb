# frozen_string_literal: true

module Payments
  module StepHandlers
    # TAS-137 Best Practices Demonstrated:
    # - get_input(): Access task context fields (refund_reason)
    # - get_input_or(): Access task context with defaults (refund_reason)
    # - get_dependency_result(): Access upstream step results (process_gateway_refund, validate_payment_eligibility)
    # - get_dependency_field(): Extract nested fields from dependency results (payment_id, refund_id, refund_amount, gateway_transaction_id, original_amount)
    class UpdatePaymentRecordsHandler < TaskerCore::StepHandler::Base
      def call(context)
        # Extract and validate inputs
        inputs = extract_and_validate_inputs(context)

        logger.info "📝 UpdatePaymentRecordsHandler: Updating payment records - task_uuid=#{context.task_uuid}, payment_id=#{inputs[:payment_id]}"

        # Simulate updating payment records
        update_result = simulate_payment_record_update(inputs)

        # Ensure update was successful
        ensure_update_successful!(update_result)

        logger.info "✅ UpdatePaymentRecordsHandler: Records updated - payment_id=#{inputs[:payment_id]}, record_id=#{update_result[:record_id]}"

        # Return standardized StepHandlerCallResult
        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            records_updated: true,
            payment_id: inputs[:payment_id],
            refund_id: inputs[:refund_id],
            record_id: update_result[:record_id],
            payment_status: update_result[:payment_status],
            refund_status: update_result[:refund_status],
            history_entries_created: update_result[:history_entries_created],
            updated_at: update_result[:updated_at],
            namespace: 'payments'
          },
          metadata: {
            operation: 'update_payment_records',
            execution_hints: {
              payment_id: inputs[:payment_id],
              refund_id: inputs[:refund_id],
              record_id: update_result[:record_id]
            },
            http_headers: {
              'X-Payment-Record-Service' => 'MockPaymentRecordSystem',
              'X-Record-ID' => update_result[:record_id],
              'X-Payment-Status' => update_result[:payment_status]
            },
            input_refs: {
              refund_reason: 'context.get_input_or("refund_reason", "customer_request")',
              payment_id: 'context.get_dependency_field("process_gateway_refund", "payment_id")',
              refund_id: 'context.get_dependency_field("process_gateway_refund", "refund_id")',
              refund_amount: 'context.get_dependency_field("process_gateway_refund", "refund_amount")',
              gateway_transaction_id: 'context.get_dependency_field("process_gateway_refund", "gateway_transaction_id")',
              original_amount: 'context.get_dependency_field("validate_payment_eligibility", "original_amount")'
            }
          }
        )
      rescue StandardError => e
        logger.error "❌ UpdatePaymentRecordsHandler: Update failed - #{e.class.name}: #{e.message}"
        raise
      end

      private

      # TAS-137: Extract and validate inputs using StepContext API
      def extract_and_validate_inputs(context)
        # TAS-137: Validate dependency result exists
        refund_result = context.get_dependency_result('process_gateway_refund')
        refund_result = refund_result.deep_symbolize_keys if refund_result

        unless refund_result&.dig(:refund_processed)
          raise TaskerCore::Errors::PermanentError.new(
            'Gateway refund must be completed before updating records',
            error_code: 'MISSING_REFUND'
          )
        end

        {
          # TAS-137: Use get_dependency_field for nested dependency access
          payment_id: context.get_dependency_field('process_gateway_refund', 'payment_id'),
          refund_id: context.get_dependency_field('process_gateway_refund', 'refund_id'),
          refund_amount: context.get_dependency_field('process_gateway_refund', 'refund_amount'),
          # TAS-137: Use get_input_or for task context with default
          refund_reason: context.get_input_or('refund_reason', 'customer_request'),
          # TAS-137: Use get_dependency_field for nested dependency access
          gateway_transaction_id: context.get_dependency_field('process_gateway_refund', 'gateway_transaction_id'),
          original_amount: context.get_dependency_field('validate_payment_eligibility', 'original_amount')
        }
      end

      # Simulate payment record update (self-contained)
      def simulate_payment_record_update(inputs)
        payment_id = inputs[:payment_id]

        # Simulate different record update scenarios
        case payment_id
        when /pay_test_record_lock/
          { status: 'locked', error: 'Record locked by another process' }
        when /pay_test_record_error/
          { status: 'error', error: 'Database error' }
        else
          # Success case
          record_id = "rec_#{SecureRandom.hex(8)}"
          {
            status: 'updated',
            record_id: record_id,
            payment_id: payment_id,
            refund_id: inputs[:refund_id],
            payment_status: 'refunded',
            refund_status: 'completed',
            history_entries_created: 2, # Payment history + refund history
            updated_at: Time.now.utc.iso8601,
            history_entries: [
              {
                type: 'refund_initiated',
                refund_id: inputs[:refund_id],
                amount: inputs[:refund_amount],
                reason: inputs[:refund_reason],
                timestamp: Time.now.utc.iso8601
              },
              {
                type: 'refund_completed',
                refund_id: inputs[:refund_id],
                gateway_transaction_id: inputs[:gateway_transaction_id],
                timestamp: Time.now.utc.iso8601
              }
            ]
          }
        end
      end

      # Ensure record update was successful
      def ensure_update_successful!(update_result)
        status = update_result[:status]

        case status
        when 'updated', 'success'
          # Update successful
          nil
        when 'locked'
          # Temporary error - record locked
          raise TaskerCore::Errors::RetryableError.new(
            'Payment record locked, will retry',
            retry_after: 10
          )
        when 'not_found'
          # Permanent error - payment not found
          raise TaskerCore::Errors::PermanentError.new(
            'Payment record not found',
            error_code: 'PAYMENT_NOT_FOUND'
          )
        when 'error', 'failed'
          # Temporary error - database issue
          raise TaskerCore::Errors::RetryableError.new(
            "Record update failed: #{update_result[:error]}",
            retry_after: 30
          )
        else
          # Unknown status - treat as retryable
          raise TaskerCore::Errors::RetryableError.new(
            "Unknown update status: #{status}",
            retry_after: 30
          )
        end
      end
    end
  end
end
