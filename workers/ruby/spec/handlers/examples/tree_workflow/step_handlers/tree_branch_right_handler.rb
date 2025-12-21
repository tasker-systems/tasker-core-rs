# frozen_string_literal: true

module TreeWorkflow
  module StepHandlers
    # Tree Branch Right: Right main branch that squares the input
    class TreeBranchRightHandler < TaskerCore::StepHandler::Base
      def call(context)
        # Get result from tree_root
        root_result = context.get_dependency_result('tree_root')
        raise 'Tree root result not found' unless root_result

        # Square the root result (single parent operation)
        result = root_result * root_result

        logger.info "Tree Branch Right: #{root_result}² = #{result}"

        # Return result for right sub-branches
        TaskerCore::Types::StepHandlerCallResult.success(
          result: result,
          metadata: {
            operation: 'square',
            step_type: 'single_parent',
            input_refs: {
              root_result: 'sequence.tree_root.result'
            },
            branch: 'right_main',
            sub_branches: %w[tree_leaf_f tree_leaf_g]
          }
        )
      end
    end
  end
end
