# frozen_string_literal: true

module TreeWorkflow
  module StepHandlers
    # Tree Branch Right: Right main branch that squares the input
    class TreeBranchRightHandler < TaskerCore::StepHandler::Base
      def call(task, sequence, step)
        # Get result from tree_root
        root_result = sequence.get("tree_root")&.dig("result")
        raise "Tree root result not found" unless root_result

        # Square the root result (single parent operation)
        result = root_result * root_result

        logger.info "Tree Branch Right: #{root_result}² = #{result}"

        # Return result for right sub-branches
        {
          status: "success",
          result: result,
          metadata: {
            operation: "square",
            input: root_result,
            output: result,
            step_type: "single_parent",
            branch: "right_main",
            sub_branches: ["tree_leaf_f", "tree_leaf_g"]
          }
        }
      end
    end
  end
end