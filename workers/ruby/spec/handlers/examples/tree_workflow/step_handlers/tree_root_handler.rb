# frozen_string_literal: true

module TreeWorkflow
  module StepHandlers
    # Tree Root: Initial step that squares the even number
    class TreeRootHandler < TaskerCore::StepHandler::Base
      def call(context)
        # Get the even number from task context
        even_number = context.task.context['even_number']
        raise 'Task context must contain an even number' unless even_number&.even?

        # Square the even number (first step operation)
        result = even_number * even_number

        logger.info "Tree Root: #{even_number}² = #{result}"

        # Return result for both main branches
        TaskerCore::Types::StepHandlerCallResult.success(
          result: result,
          metadata: {
            operation: 'square',
            step_type: 'initial',
            input_refs: {
              even_number: 'context.task.context.even_number'
            },
            branches: %w[tree_branch_left tree_branch_right]
          }
        )
      end
    end
  end
end
