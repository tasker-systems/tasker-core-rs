# frozen_string_literal: true

module ConditionalApproval
  module StepHandlers
    # Auto Approve: Automatically approves small requests
    #
    # This step is dynamically created by the routing_decision decision point
    # when the amount is below the small threshold ($1,000).
    class AutoApproveHandler < TaskerCore::StepHandler::Base
      def call(task, _sequence, _step)
        amount = task.context['amount']
        requester = task.context['requester']

        logger.info "Auto-approving request: #{requester} for $#{amount}"

        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            approved: true,
            approval_type: 'automatic',
            approved_amount: amount,
            approved_by: 'system',
            approved_at: Time.now.iso8601,
            notes: 'Automatically approved - below manual review threshold'
          },
          metadata: {
            operation: 'auto_approve',
            step_type: 'dynamic_branch',
            approval_method: 'automated'
          }
        )
      end
    end
  end
end
