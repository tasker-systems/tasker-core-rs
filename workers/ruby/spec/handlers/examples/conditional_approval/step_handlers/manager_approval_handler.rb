# frozen_string_literal: true

module ConditionalApproval
  module StepHandlers
    # Manager Approval: Simulates manager approval for medium/large requests
    #
    # This step is dynamically created by the routing_decision decision point
    # when the amount is >= $1,000.
    class ManagerApprovalHandler < TaskerCore::StepHandler::Base
      def call(context)
        amount = context.task.context['amount']
        requester = context.task.context['requester']
        purpose = context.task.context['purpose']

        logger.info "Manager reviewing approval request: #{requester} requesting $#{amount} for #{purpose}"

        # Simulate manager review logic
        # In a real system, this might wait for external input or call an API
        approved = amount <= 10_000 # Manager can approve up to $10k

        if approved
          logger.info "Manager approved: #{requester} for $#{amount}"
          TaskerCore::Types::StepHandlerCallResult.success(
            result: {
              approved: true,
              approval_type: 'manager',
              approved_amount: amount,
              approved_by: 'manager_system',
              approved_at: Time.now.iso8601,
              notes: 'Approved by manager review'
            },
            metadata: {
              operation: 'manager_approval',
              step_type: 'dynamic_branch',
              approval_method: 'manual_review',
              reviewer_role: 'manager'
            }
          )
        else
          raise TaskerCore::Errors::PermanentError.new(
            "Amount $#{amount} exceeds manager approval limit of $10,000",
            error_code: 'AMOUNT_EXCEEDS_MANAGER_LIMIT',
            context: { amount: amount, limit: 10_000 }
          )
        end
      end
    end
  end
end
