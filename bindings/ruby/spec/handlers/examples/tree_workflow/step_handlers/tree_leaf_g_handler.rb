# frozen_string_literal: true

module TreeWorkflow
  module StepHandlers
    # Tree Leaf G: Right-right leaf that squares the input from right branch
    class TreeLeafGHandler < TaskerCore::StepHandler::Base
      def call(_task, sequence, _step)
        # Get result from tree_branch_right
        branch_result = sequence.get_results('tree_branch_right')
        raise 'Tree branch right result not found' unless branch_result

        # Square the branch result (single parent operation)
        result = branch_result * branch_result

        logger.info "Tree Leaf G: #{branch_result}² = #{result}"

        # Return result for final convergence
        TaskerCore::Types::StepHandlerCallResult.success(
          result: result,
          metadata: {
            operation: 'square',
            step_type: 'single_parent',
            input_refs: {
              branch_result: 'sequence.tree_branch_right.result'
            },
            branch: 'right',
            leaf: 'g'
          }
        )
      end
    end
  end
end
