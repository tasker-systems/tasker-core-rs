# frozen_string_literal: true

module TreeWorkflow
  module StepHandlers
    # Tree Branch Left: Left main branch that squares the input
    class TreeBranchLeftHandler < TaskerCore::StepHandler::Base
      def call(_task, sequence, _step)
        # Get result from tree_root
        root_result = sequence.get_results('tree_root')
        raise 'Tree root result not found' unless root_result

        # Square the root result (single parent operation)
        result = root_result * root_result

        logger.info "Tree Branch Left: #{root_result}² = #{result}"

        # Return result for left sub-branches
        TaskerCore::Types::StepHandlerCallResult.success(
          result: result,
          metadata: {
            operation: 'square',
            step_type: 'single_parent',
            input_refs: {
              root_result: 'sequence.tree_root.result'
            },
            branch: 'left_main',
            sub_branches: %w[tree_leaf_d tree_leaf_e]
          }
        )
      end
    end
  end
end
