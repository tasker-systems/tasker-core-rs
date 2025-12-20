# frozen_string_literal: true

module ConditionalApproval
  module StepHandlers
    # Routing Decision: DECISION POINT that routes approval based on amount
    #
    # This is a TAS-53 Decision Point step that makes runtime decisions about
    # which approval workflow steps to create dynamically.
    #
    # Routing logic:
    # - amount < $1,000:  auto_approve
    # - $1,000 <= amount < $5,000: manager_approval
    # - amount >= $5,000: manager_approval + finance_review
    #
    # Uses TaskerCore::StepHandler::Decision base class for clean, type-safe
    # decision outcome serialization consistent with Rust expectations.
    class RoutingDecisionHandler < TaskerCore::StepHandler::Decision
      SMALL_AMOUNT_THRESHOLD = 1_000
      LARGE_AMOUNT_THRESHOLD = 5_000

      def call(context)
        # Get amount from validated request
        amount = context.task.context['amount']
        raise 'Amount is required for routing decision' unless amount

        # Make routing decision based on amount
        route = determine_route(amount)

        logger.info "Routing decision for $#{amount}: #{route[:reasoning]} -> creating steps: #{route[:steps].join(', ')}"

        # TAS-53: Use Decision base class helper for clean outcome serialization
        decision_success(
          steps: route[:steps],
          result_data: {
            route_type: route[:type],
            reasoning: route[:reasoning],
            amount: amount
          },
          metadata: {
            operation: 'routing_decision',
            route_thresholds: {
              small: SMALL_AMOUNT_THRESHOLD,
              large: LARGE_AMOUNT_THRESHOLD
            }
          }
        )
      end

      private

      def determine_route(amount)
        if amount < SMALL_AMOUNT_THRESHOLD
          {
            type: 'auto_approval',
            steps: ['auto_approve'],
            reasoning: "Amount $#{amount} below $#{SMALL_AMOUNT_THRESHOLD} threshold - auto-approval"
          }
        elsif amount < LARGE_AMOUNT_THRESHOLD
          {
            type: 'manager_only',
            steps: ['manager_approval'],
            reasoning: "Amount $#{amount} requires manager approval (between $#{SMALL_AMOUNT_THRESHOLD} and $#{LARGE_AMOUNT_THRESHOLD})"
          }
        else
          {
            type: 'dual_approval',
            steps: %w[manager_approval finance_review],
            reasoning: "Amount $#{amount} >= $#{LARGE_AMOUNT_THRESHOLD} - requires both manager and finance approval"
          }
        end
      end
    end
  end
end
