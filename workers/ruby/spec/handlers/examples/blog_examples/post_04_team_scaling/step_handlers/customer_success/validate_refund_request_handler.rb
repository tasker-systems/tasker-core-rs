# frozen_string_literal: true

module CustomerSuccess
  module StepHandlers
    class ValidateRefundRequestHandler < TaskerCore::StepHandler::Base
      def call(context)
        # Extract and validate inputs
        inputs = extract_and_validate_inputs(context)

        logger.info "🎫 ValidateRefundRequestHandler: Validating refund request - task_uuid=#{context.task_uuid}, ticket_id=#{inputs[:ticket_id]}"

        # Simulate customer service system validation
        service_response = simulate_customer_service_validation(inputs)

        # Ensure request is valid
        ensure_request_valid!(service_response)

        logger.info "✅ ValidateRefundRequestHandler: Request validated - ticket_id=#{inputs[:ticket_id]}, customer_tier=#{service_response[:customer_tier]}"

        # Return standardized StepHandlerCallResult
        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            request_validated: true,
            ticket_id: service_response[:ticket_id],
            customer_id: service_response[:customer_id],
            ticket_status: service_response[:status],
            customer_tier: service_response[:customer_tier],
            original_purchase_date: service_response[:purchase_date],
            payment_id: service_response[:payment_id],
            validation_timestamp: Time.now.utc.iso8601,
            namespace: 'customer_success'
          },
          metadata: {
            operation: 'validate_refund_request',
            execution_hints: {
              ticket_id: inputs[:ticket_id],
              customer_tier: service_response[:customer_tier],
              ticket_status: service_response[:status]
            },
            http_headers: {
              'X-Customer-Service-Platform' => 'MockCustomerServiceSystem',
              'X-Ticket-ID' => inputs[:ticket_id],
              'X-Customer-Tier' => service_response[:customer_tier]
            },
            input_refs: {
              ticket_id: 'context.task.context.ticket_id',
              customer_id: 'context.task.context.customer_id'
            }
          }
        )
      rescue StandardError => e
        logger.error "❌ ValidateRefundRequestHandler: Validation failed - #{e.class.name}: #{e.message}"
        raise
      end

      private

      # Extract and validate inputs
      def extract_and_validate_inputs(context)
        task_context = context.task.context.deep_symbolize_keys

        # Validate required fields
        required_fields = %i[ticket_id customer_id refund_amount]
        missing_fields = required_fields.select { |field| task_context[field].blank? }

        if missing_fields.any?
          raise TaskerCore::Errors::PermanentError.new(
            "Missing required fields for refund validation: #{missing_fields.join(', ')}",
            error_code: 'MISSING_REQUIRED_FIELDS'
          )
        end

        {
          ticket_id: task_context[:ticket_id],
          customer_id: task_context[:customer_id],
          refund_amount: task_context[:refund_amount],
          refund_reason: task_context[:refund_reason]
        }
      end

      # Simulate customer service system validation (self-contained)
      def simulate_customer_service_validation(inputs)
        ticket_id = inputs[:ticket_id]

        # Simulate different ticket scenarios
        case ticket_id
        when /ticket_closed/
          {
            status: 'closed',
            ticket_id: ticket_id,
            customer_id: inputs[:customer_id],
            error: 'Ticket is closed'
          }
        when /ticket_cancelled/
          {
            status: 'cancelled',
            ticket_id: ticket_id,
            customer_id: inputs[:customer_id],
            error: 'Ticket was cancelled'
          }
        when /ticket_duplicate/
          {
            status: 'duplicate',
            ticket_id: ticket_id,
            customer_id: inputs[:customer_id],
            error: 'Duplicate ticket'
          }
        else
          # Success case - valid ticket
          {
            status: 'open',
            ticket_id: ticket_id,
            customer_id: inputs[:customer_id],
            customer_tier: determine_customer_tier(inputs[:customer_id]),
            purchase_date: (Time.now - 30.days).utc.iso8601,
            payment_id: "pay_#{SecureRandom.hex(8)}",
            ticket_created_at: (Time.now - 2.days).utc.iso8601,
            agent_assigned: "agent_#{rand(1..10)}"
          }
        end
      end

      # Determine customer tier based on customer ID
      def determine_customer_tier(customer_id)
        case customer_id
        when /vip/, /premium/
          'premium'
        when /gold/
          'gold'
        else
          'standard'
        end
      end

      # Ensure request is valid for processing
      def ensure_request_valid!(service_response)
        status = service_response[:status]

        case status
        when 'open', 'in_progress'
          # Ticket is active and can be processed
          nil
        when 'closed'
          # Permanent error - can't process refunds for closed tickets
          raise TaskerCore::Errors::PermanentError.new(
            'Cannot process refund for closed ticket',
            error_code: 'TICKET_CLOSED'
          )
        when 'cancelled'
          # Permanent error - ticket was cancelled
          raise TaskerCore::Errors::PermanentError.new(
            'Cannot process refund for cancelled ticket',
            error_code: 'TICKET_CANCELLED'
          )
        when 'duplicate'
          # Permanent error - duplicate ticket
          raise TaskerCore::Errors::PermanentError.new(
            'Cannot process refund for duplicate ticket',
            error_code: 'TICKET_DUPLICATE'
          )
        else
          # Unknown status - treat as temporary issue
          raise TaskerCore::Errors::RetryableError.new(
            "Unknown ticket status: #{status}",
            retry_after: 30
          )
        end
      end
    end
  end
end
