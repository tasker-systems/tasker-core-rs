# frozen_string_literal: true

module CustomerSuccess
  module StepHandlers
    # TAS-137 Best Practices Demonstrated:
    # - get_input(): Access task context fields (refund_amount, refund_reason)
    # - get_dependency_result(): Access upstream step results (execute_refund_workflow, validate_refund_request)
    # - get_dependency_field(): Extract nested fields from dependency results (ticket_id, customer_id, delegated_task_id, correlation_id)
    class UpdateTicketStatusHandler < TaskerCore::StepHandler::Base
      def call(context)
        # Extract and validate inputs
        inputs = extract_and_validate_inputs(context)

        logger.info "🎫 UpdateTicketStatusHandler: Updating ticket status - task_uuid=#{context.task_uuid}, ticket_id=#{inputs[:ticket_id]}"

        # Simulate ticket status update
        update_result = simulate_ticket_update(inputs)

        # Ensure update was successful
        ensure_update_successful!(update_result)

        logger.info "✅ UpdateTicketStatusHandler: Ticket updated - ticket_id=#{inputs[:ticket_id]}, status=#{update_result[:new_status]}"

        # Return standardized StepHandlerCallResult
        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            ticket_updated: true,
            ticket_id: inputs[:ticket_id],
            previous_status: update_result[:previous_status],
            new_status: update_result[:new_status],
            resolution_note: update_result[:resolution_note],
            updated_at: update_result[:updated_at],
            refund_completed: true,
            delegated_task_id: inputs[:delegated_task_id],
            namespace: 'customer_success'
          },
          metadata: {
            operation: 'update_ticket_status',
            execution_hints: {
              ticket_id: inputs[:ticket_id],
              new_status: update_result[:new_status],
              refund_completed: true
            },
            http_headers: {
              'X-Customer-Service-Platform' => 'MockCustomerServiceSystem',
              'X-Ticket-ID' => inputs[:ticket_id],
              'X-Ticket-Status' => update_result[:new_status]
            },
            input_refs: {
              refund_amount: 'context.get_input("refund_amount")',
              refund_reason: 'context.get_input("refund_reason")',
              ticket_id: 'context.get_dependency_field("validate_refund_request", "ticket_id")',
              customer_id: 'context.get_dependency_field("validate_refund_request", "customer_id")',
              delegated_task_id: 'context.get_dependency_field("execute_refund_workflow", "delegated_task_id")',
              correlation_id: 'context.get_dependency_field("execute_refund_workflow", "correlation_id")'
            }
          }
        )
      rescue StandardError => e
        logger.error "❌ UpdateTicketStatusHandler: Ticket update failed - #{e.class.name}: #{e.message}"
        raise
      end

      private

      # TAS-137: Extract and validate inputs using StepContext API
      def extract_and_validate_inputs(context)
        # TAS-137: Validate dependency result exists
        delegation_result = context.get_dependency_result('execute_refund_workflow')
        delegation_result = delegation_result.deep_symbolize_keys if delegation_result

        unless delegation_result&.dig(:task_delegated)
          raise TaskerCore::Errors::PermanentError.new(
            'Refund workflow must be executed before updating ticket',
            error_code: 'MISSING_DELEGATION'
          )
        end

        {
          # TAS-137: Use get_dependency_field for nested dependency access
          ticket_id: context.get_dependency_field('validate_refund_request', 'ticket_id'),
          customer_id: context.get_dependency_field('validate_refund_request', 'customer_id'),
          # TAS-137: Use get_input for task context access
          refund_amount: context.get_input('refund_amount'),
          refund_reason: context.get_input('refund_reason'),
          # TAS-137: Use get_dependency_field for nested dependency access
          delegated_task_id: context.get_dependency_field('execute_refund_workflow', 'delegated_task_id'),
          correlation_id: context.get_dependency_field('execute_refund_workflow', 'correlation_id')
        }
      end

      # Simulate ticket status update (self-contained)
      def simulate_ticket_update(inputs)
        ticket_id = inputs[:ticket_id]

        # Simulate different update scenarios
        case ticket_id
        when /ticket_locked/
          {
            status: 'locked',
            error: 'Ticket locked by another agent'
          }
        when /ticket_update_error/
          {
            status: 'error',
            error: 'System error updating ticket'
          }
        else
          # Success case
          {
            status: 'updated',
            ticket_id: ticket_id,
            previous_status: 'in_progress',
            new_status: 'resolved',
            resolution_note: "Refund of $#{format('%.2f', inputs[:refund_amount] / 100.0)} processed successfully. " \
                             "Delegated task ID: #{inputs[:delegated_task_id]}. " \
                             "Correlation ID: #{inputs[:correlation_id]}",
            resolution_type: 'refund_processed',
            updated_at: Time.now.utc.iso8601,
            updated_by: 'automated_workflow',
            customer_notified: true
          }
        end
      end

      # Ensure ticket update was successful
      def ensure_update_successful!(update_result)
        status = update_result[:status]

        case status
        when 'updated', 'success'
          # Update successful
          nil
        when 'locked'
          # Temporary error - ticket locked
          raise TaskerCore::Errors::RetryableError.new(
            'Ticket locked by another agent, will retry',
            retry_after: 15
          )
        when 'not_found'
          # Permanent error - ticket not found
          raise TaskerCore::Errors::PermanentError.new(
            'Ticket not found in customer service system',
            error_code: 'TICKET_NOT_FOUND'
          )
        when 'error', 'failed'
          # Temporary error - system issue
          raise TaskerCore::Errors::RetryableError.new(
            "Ticket update failed: #{update_result[:error]}",
            retry_after: 30
          )
        when 'unauthorized'
          # Permanent error - permission issue
          raise TaskerCore::Errors::PermanentError.new(
            'Not authorized to update ticket',
            error_code: 'UNAUTHORIZED'
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
