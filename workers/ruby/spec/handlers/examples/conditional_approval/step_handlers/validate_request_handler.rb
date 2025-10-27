# frozen_string_literal: true

module ConditionalApproval
  module StepHandlers
    # Validate Request: Initial step that validates the approval request
    class ValidateRequestHandler < TaskerCore::StepHandler::Base
      def call(task, _sequence, _step)
        # Get request data from task context
        amount = task.context['amount']
        requester = task.context['requester']
        purpose = task.context['purpose']

        # Validate required fields
        raise 'Task context must contain amount' unless amount
        raise 'Amount must be positive' unless amount.positive?
        raise 'Task context must contain requester' unless requester && !requester.empty?
        raise 'Task context must contain purpose' unless purpose && !purpose.empty?

        logger.info "Validating approval request: #{requester} requesting $#{amount} for #{purpose}"

        # Return validated request data
        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            amount: amount,
            requester: requester,
            purpose: purpose,
            validated_at: Time.now.iso8601
          },
          metadata: {
            operation: 'validate',
            step_type: 'initial',
            validation_checks: %w[amount_positive requester_present purpose_present]
          }
        )
      end
    end
  end
end
