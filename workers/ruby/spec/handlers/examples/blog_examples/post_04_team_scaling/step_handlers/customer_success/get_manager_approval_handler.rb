# frozen_string_literal: true

module CustomerSuccess
  module StepHandlers
    class GetManagerApprovalHandler < TaskerCore::StepHandler::Base
      def call(task, sequence, _step)
        # Extract and validate inputs
        inputs = extract_and_validate_inputs(task, sequence, _step)

        logger.info "👨‍💼 GetManagerApprovalHandler: Getting manager approval - task_uuid=#{task.task_uuid}, requires_approval=#{inputs[:requires_approval]}"

        # Check if approval is required
        if inputs[:requires_approval]
          # Simulate approval process
          approval_result = simulate_manager_approval(inputs)

          # Ensure approval was obtained
          ensure_approval_obtained!(approval_result)

          logger.info "✅ GetManagerApprovalHandler: Approval obtained - approval_id=#{approval_result[:approval_id]}, manager=#{approval_result[:manager_id]}"
        else
          logger.info "⏭️  GetManagerApprovalHandler: Approval not required - auto-approved for customer_tier=#{inputs[:customer_tier]}"

          approval_result = {
            approval_required: false,
            auto_approved: true,
            reason: "Customer tier #{inputs[:customer_tier]} does not require approval"
          }
        end

        # Return standardized StepHandlerCallResult
        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            approval_obtained: true,
            approval_required: inputs[:requires_approval],
            auto_approved: !inputs[:requires_approval],
            approval_id: approval_result[:approval_id],
            manager_id: approval_result[:manager_id],
            manager_notes: approval_result[:manager_notes],
            approved_at: approval_result[:approved_at] || Time.now.utc.iso8601,
            namespace: 'customer_success'
          },
          metadata: {
            operation: 'get_manager_approval',
            execution_hints: {
              approval_required: inputs[:requires_approval],
              approval_id: approval_result[:approval_id],
              manager_id: approval_result[:manager_id]
            },
            http_headers: {
              'X-Approval-System' => 'ManagerApprovalPortal',
              'X-Approval-ID' => approval_result[:approval_id] || 'N/A',
              'X-Auto-Approved' => (!inputs[:requires_approval]).to_s
            },
            input_refs: {
              policy_check: 'sequence.check_refund_policy.result',
              requires_approval: 'sequence.check_refund_policy.result.requires_approval'
            }
          }
        )
      rescue StandardError => e
        logger.error "❌ GetManagerApprovalHandler: Approval failed - #{e.class.name}: #{e.message}"
        raise
      end

      private

      # Extract and validate inputs from task and previous steps
      def extract_and_validate_inputs(task, sequence, _step)
        context = task.context.deep_symbolize_keys

        # Get policy check results
        policy_result = sequence.get_results('check_refund_policy')
        policy_result = policy_result.deep_symbolize_keys if policy_result

        unless policy_result&.dig(:policy_checked)
          raise TaskerCore::Errors::PermanentError.new(
            'Policy check must be completed before approval',
            error_code: 'MISSING_POLICY_CHECK'
          )
        end

        # Get validation results for ticket info
        validation_result = sequence.get_results('validate_refund_request')
        validation_result = validation_result.deep_symbolize_keys if validation_result

        {
          requires_approval: policy_result[:requires_approval],
          customer_tier: policy_result[:customer_tier],
          refund_amount: context[:refund_amount],
          refund_reason: context[:refund_reason],
          ticket_id: validation_result[:ticket_id],
          customer_id: validation_result[:customer_id]
        }
      end

      # Simulate manager approval process (self-contained)
      def simulate_manager_approval(inputs)
        ticket_id = inputs[:ticket_id]

        # Simulate different approval scenarios
        case ticket_id
        when /ticket_denied/
          {
            status: 'denied',
            reason: 'Manager denied refund request',
            manager_id: "mgr_#{rand(1..5)}"
          }
        when /ticket_pending/
          {
            status: 'pending',
            reason: 'Waiting for manager response'
          }
        else
          # Success case - approval granted
          approval_id = "appr_#{SecureRandom.hex(8)}"
          manager_id = "mgr_#{rand(1..5)}"

          {
            status: 'approved',
            approval_required: true,
            approval_id: approval_id,
            manager_id: manager_id,
            manager_notes: "Approved refund request for customer #{inputs[:customer_id]}",
            approved_at: Time.now.utc.iso8601,
            approval_timestamp: Time.now.utc.iso8601
          }
        end
      end

      # Ensure approval was obtained (if required)
      def ensure_approval_obtained!(approval_result)
        status = approval_result[:status]

        case status
        when 'approved'
          # Approval granted
          nil
        when 'denied'
          # Permanent error - manager denied
          raise TaskerCore::Errors::PermanentError.new(
            "Manager denied refund request: #{approval_result[:reason]}",
            error_code: 'APPROVAL_DENIED'
          )
        when 'pending'
          # Temporary state - waiting for approval
          raise TaskerCore::Errors::RetryableError.new(
            'Waiting for manager approval',
            retry_after: 60 # Check every minute
          )
        when 'timeout'
          # Temporary error - approval timed out
          raise TaskerCore::Errors::RetryableError.new(
            'Manager approval timeout, will retry',
            retry_after: 120
          )
        else
          # Unknown status - treat as temporary
          raise TaskerCore::Errors::RetryableError.new(
            "Unknown approval status: #{status}",
            retry_after: 60
          )
        end
      end
    end
  end
end
