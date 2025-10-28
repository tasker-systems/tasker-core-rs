# frozen_string_literal: true

module ConditionalApproval
  module StepHandlers
    # Finalize Approval: Final convergence step that processes all approvals
    #
    # This step receives results from whichever approval path was taken:
    # - auto_approve (for small amounts)
    # - manager_approval (for medium amounts)
    # - manager_approval + finance_review (for large amounts)
    #
    # It consolidates the approval decisions and creates the final approval record.
    class FinalizeApprovalHandler < TaskerCore::StepHandler::Base
      def call(task, sequence, _step)
        amount = task.context['amount']
        requester = task.context['requester']
        purpose = task.context['purpose']

        # Collect all approval results from dependencies
        approvals = collect_approval_results(sequence)

        logger.info "Finalizing approval for #{requester}: $#{amount} - #{approvals.size} approval(s) received"

        # Verify all required approvals are present and approved
        all_approved = approvals.all? { |approval| approval[:approved] }

        unless all_approved
          raise TaskerCore::Errors::PermanentError.new(
            'Not all required approvals were granted',
            error_code: 'APPROVAL_DENIED',
            context: { approvals: approvals }
          )
        end

        # Create final approval record
        approval_record = {
          approved: true,
          final_amount: amount,
          requester: requester,
          purpose: purpose,
          approval_chain: approvals.map { |a| a[:approval_type] },
          approved_by: approvals.map { |a| a[:approved_by] },
          finalized_at: Time.now.iso8601,
          approval_path: determine_approval_path(approvals)
        }

        logger.info "Approval finalized: #{requester} for $#{amount} via #{approval_record[:approval_path]} path"

        TaskerCore::Types::StepHandlerCallResult.success(
          result: approval_record,
          metadata: {
            operation: 'finalize_approval',
            step_type: 'convergence',
            approval_count: approvals.size,
            total_processing_steps: sequence.keys.size
          }
        )
      end

      private

      def collect_approval_results(sequence)
        # Get results from all parent steps
        # In a real system, we'd parse the step results to extract approval data
        approval_steps = %w[auto_approve manager_approval finance_review]

        # Iterate through all dependency keys and filter for approval steps
        sequence.keys.select do |step_name|
          approval_steps.include?(step_name)
        end.map do |step_name|
          # Get the result data for this step
          result_data = sequence.get_results(step_name)
          result_data if result_data.is_a?(Hash) && result_data['approved'] == true
        end.compact
      end

      def determine_approval_path(approvals)
        types = approvals.map { |a| a[:approval_type] }.sort

        case types
        when ['automatic']
          'auto'
        when ['manager']
          'manager_only'
        when ['finance', 'manager']
          'dual_approval'
        else
          'unknown'
        end
      end
    end
  end
end
