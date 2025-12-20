# frozen_string_literal: true

module ConditionalApproval
  module StepHandlers
    # Finance Review: Financial review for large requests
    #
    # This step is dynamically created by the routing_decision decision point
    # when the amount is >= $5,000 (in addition to manager_approval).
    class FinanceReviewHandler < TaskerCore::StepHandler::Base
      def call(context)
        amount = context.task.context['amount']
        requester = context.task.context['requester']
        purpose = context.task.context['purpose']

        logger.info "Finance reviewing approval request: #{requester} requesting $#{amount} for #{purpose}"

        # Simulate finance review logic
        # In a real system, this might check budgets, compliance, etc.
        budget_available = check_budget(amount, purpose)

        if budget_available
          logger.info "Finance approved: #{requester} for $#{amount}"
          TaskerCore::Types::StepHandlerCallResult.success(
            result: {
              approved: true,
              approval_type: 'finance',
              approved_amount: amount,
              approved_by: 'finance_system',
              approved_at: Time.now.iso8601,
              notes: 'Approved by finance review - budget confirmed',
              budget_code: generate_budget_code(purpose)
            },
            metadata: {
              operation: 'finance_review',
              step_type: 'dynamic_branch',
              approval_method: 'financial_review',
              reviewer_role: 'finance',
              compliance_checks: %w[budget_available compliance_verified]
            }
          )
        else
          raise TaskerCore::Errors::PermanentError.new(
            "Insufficient budget for $#{amount} request",
            error_code: 'INSUFFICIENT_BUDGET',
            context: { amount: amount, purpose: purpose }
          )
        end
      end

      private

      def check_budget(amount, _purpose)
        # Simplified budget check - in real system would query budget database
        # For demo purposes, approve amounts up to $25,000
        amount <= 25_000
      end

      def generate_budget_code(purpose)
        # Generate a simple budget code
        "BUDGET-#{purpose.upcase.gsub(/\s+/, '_')}-#{Time.now.to_i}"
      end
    end
  end
end
