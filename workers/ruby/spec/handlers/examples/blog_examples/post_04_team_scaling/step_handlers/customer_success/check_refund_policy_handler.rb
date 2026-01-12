# frozen_string_literal: true

module CustomerSuccess
  module StepHandlers
    # TAS-137 Best Practices Demonstrated:
    # - get_input(): Access task context fields (refund_amount, refund_reason)
    # - get_dependency_result(): Access upstream step results (validate_refund_request)
    # - get_dependency_field(): Extract nested fields from dependency results (customer_tier, original_purchase_date)
    class CheckRefundPolicyHandler < TaskerCore::StepHandler::Base
      # Refund policy rules (self-contained)
      REFUND_POLICIES = {
        standard: { window_days: 30, requires_approval: true, max_amount: 10_000 },
        gold: { window_days: 60, requires_approval: false, max_amount: 50_000 },
        premium: { window_days: 90, requires_approval: false, max_amount: 100_000 }
      }.freeze

      def call(context)
        # Extract and validate inputs
        inputs = extract_and_validate_inputs(context)

        logger.info "📋 CheckRefundPolicyHandler: Checking refund policy - task_uuid=#{context.task_uuid}, customer_tier=#{inputs[:customer_tier]}"

        # Check policy compliance
        policy_check_result = check_policy_compliance(inputs)

        # Ensure policy allows refund
        ensure_policy_compliant!(policy_check_result)

        logger.info "✅ CheckRefundPolicyHandler: Policy check passed - requires_approval=#{policy_check_result[:requires_approval]}"

        # Return standardized StepHandlerCallResult
        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            policy_checked: true,
            policy_compliant: true,
            customer_tier: inputs[:customer_tier],
            refund_window_days: policy_check_result[:refund_window_days],
            days_since_purchase: policy_check_result[:days_since_purchase],
            within_refund_window: policy_check_result[:within_window],
            requires_approval: policy_check_result[:requires_approval],
            max_allowed_amount: policy_check_result[:max_allowed_amount],
            policy_checked_at: Time.now.utc.iso8601,
            namespace: 'customer_success'
          },
          metadata: {
            operation: 'check_refund_policy',
            execution_hints: {
              customer_tier: inputs[:customer_tier],
              requires_approval: policy_check_result[:requires_approval],
              within_window: policy_check_result[:within_window]
            },
            http_headers: {
              'X-Policy-Engine' => 'RefundPolicyService',
              'X-Customer-Tier' => inputs[:customer_tier],
              'X-Requires-Approval' => policy_check_result[:requires_approval].to_s
            },
            input_refs: {
              refund_amount: 'context.get_input("refund_amount")',
              refund_reason: 'context.get_input("refund_reason")',
              customer_tier: 'context.get_dependency_field("validate_refund_request", "customer_tier")',
              original_purchase_date: 'context.get_dependency_field("validate_refund_request", "original_purchase_date")'
            }
          }
        )
      rescue StandardError => e
        logger.error "❌ CheckRefundPolicyHandler: Policy check failed - #{e.class.name}: #{e.message}"
        raise
      end

      private

      # TAS-137: Extract and validate inputs using StepContext API
      def extract_and_validate_inputs(context)
        # TAS-137: Validate dependency result exists
        validation_result = context.get_dependency_result('validate_refund_request')
        validation_result = validation_result.deep_symbolize_keys if validation_result

        unless validation_result&.dig(:request_validated)
          raise TaskerCore::Errors::PermanentError.new(
            'Request validation must be completed before policy check',
            error_code: 'MISSING_VALIDATION'
          )
        end

        {
          # TAS-137: Use get_dependency_field for nested dependency access
          customer_tier: context.get_dependency_field('validate_refund_request', 'customer_tier') || 'standard',
          # TAS-137: Use get_input for task context access
          refund_amount: context.get_input('refund_amount'),
          # TAS-137: Use get_dependency_field for nested dependency access
          purchase_date: context.get_dependency_field('validate_refund_request', 'original_purchase_date'),
          # TAS-137: Use get_input for task context access
          refund_reason: context.get_input('refund_reason')
        }
      end

      # Check if refund complies with policy (self-contained)
      def check_policy_compliance(inputs)
        customer_tier = inputs[:customer_tier].to_sym
        policy = REFUND_POLICIES[customer_tier] || REFUND_POLICIES[:standard]

        # Calculate days since purchase
        purchase_date = Time.parse(inputs[:purchase_date])
        days_since_purchase = ((Time.now - purchase_date) / 86_400).to_i

        # Check if within refund window
        within_window = days_since_purchase <= policy[:window_days]

        # Check if amount is within limits
        within_amount_limit = inputs[:refund_amount] <= policy[:max_amount]

        {
          customer_tier: customer_tier.to_s,
          refund_window_days: policy[:window_days],
          days_since_purchase: days_since_purchase,
          within_window: within_window,
          requires_approval: policy[:requires_approval],
          max_allowed_amount: policy[:max_amount],
          requested_amount: inputs[:refund_amount],
          within_amount_limit: within_amount_limit,
          policy_compliant: within_window && within_amount_limit
        }
      end

      # Ensure policy allows this refund
      def ensure_policy_compliant!(policy_check_result)
        unless policy_check_result[:within_window]
          raise TaskerCore::Errors::PermanentError.new(
            "Refund request outside policy window: #{policy_check_result[:days_since_purchase]} days " \
            "(max: #{policy_check_result[:refund_window_days]} days)",
            error_code: 'OUTSIDE_REFUND_WINDOW'
          )
        end

        return if policy_check_result[:within_amount_limit]

        raise TaskerCore::Errors::PermanentError.new(
          "Refund amount exceeds policy limit: $#{policy_check_result[:requested_amount] / 100.0} " \
          "(max: $#{policy_check_result[:max_allowed_amount] / 100.0})",
          error_code: 'EXCEEDS_AMOUNT_LIMIT'
        )
      end
    end
  end
end
