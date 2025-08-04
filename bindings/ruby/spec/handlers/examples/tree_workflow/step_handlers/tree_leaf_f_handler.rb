# frozen_string_literal: true

module TreeWorkflow
  module StepHandlers
    # Tree Leaf F: Right-left leaf that squares the input from right branch
    class TreeLeafFHandler < TaskerCore::StepHandler::Base
      def call(task, sequence, step)
        # Get result from tree_branch_right
        branch_result = sequence.get("tree_branch_right")&.dig("result")
        raise "Tree branch right result not found" unless branch_result

        # Square the branch result (single parent operation)
        result = branch_result * branch_result

        logger.info "Tree Leaf F: #{branch_result}² = #{result}"

        # Return result for final convergence
        TaskerCore::Types::StepHandlerCallResult.success(
          result: result,
          metadata: {
            operation: "square",
            step_type: "single_parent",
            input_refs: {
              branch_result: "sequence.tree_branch_right.result"
            },
            branch: "right",
            leaf: "f"
          }
        )
      end
    end
  end
end