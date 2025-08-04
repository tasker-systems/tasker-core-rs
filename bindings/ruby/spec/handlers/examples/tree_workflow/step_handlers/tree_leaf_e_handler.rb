# frozen_string_literal: true

module TreeWorkflow
  module StepHandlers
    # Tree Leaf E: Left-right leaf that squares the input from left branch
    class TreeLeafEHandler < TaskerCore::StepHandler::Base
      def call(task, sequence, step)
        # Get result from tree_branch_left
        branch_result = sequence.get("tree_branch_left")&.dig("result")
        raise "Tree branch left result not found" unless branch_result

        # Square the branch result (single parent operation)
        result = branch_result * branch_result

        logger.info "Tree Leaf E: #{branch_result}² = #{result}"

        # Return result for final convergence
        {
          status: "success",
          result: result,
          metadata: {
            operation: "square",
            input: branch_result,
            output: result,
            step_type: "single_parent",
            branch: "left",
            leaf: "e"
          }
        }
      end
    end
  end
end