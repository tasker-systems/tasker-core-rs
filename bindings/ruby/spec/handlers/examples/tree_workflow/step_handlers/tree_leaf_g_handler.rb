# frozen_string_literal: true

module TreeWorkflow
  module StepHandlers
    # Tree Leaf G: Right-right leaf that squares the input from right branch
    class TreeLeafGHandler < TaskerCore::StepHandler::Base
      def call(task, sequence, step)
        # Get result from tree_branch_right
        branch_result = sequence.get("tree_branch_right")&.dig("result")
        raise "Tree branch right result not found" unless branch_result

        # Square the branch result (single parent operation)
        result = branch_result * branch_result

        logger.info "Tree Leaf G: #{branch_result}² = #{result}"

        # Return result for final convergence
        {
          status: "success",
          result: result,
          metadata: {
            operation: "square",
            input: branch_result,
            output: result,
            step_type: "single_parent",
            branch: "right",
            leaf: "g"
          }
        }
      end
    end
  end
end