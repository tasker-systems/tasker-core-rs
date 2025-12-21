# frozen_string_literal: true

module DiamondWorkflow
  module StepHandlers
    # Diamond Start: Initial step that squares the even number
    class DiamondStartHandler < TaskerCore::StepHandler::Base
      def call(context)
        # Get the even number from task context
        even_number = context.task.context['even_number']
        raise 'Task context must contain an even number' unless even_number&.even?

        # Square the even number (first step operation)
        result = even_number * even_number

        logger.info "Diamond Start: #{even_number}² = #{result}"

        # Return result for both parallel branches
        TaskerCore::Types::StepHandlerCallResult.success(
          result: result,
          metadata: {
            operation: 'square',
            step_type: 'initial',
            input_refs: {
              even_number: 'context.task.context.even_number'
            },
            branches: %w[diamond_branch_b diamond_branch_c]
          }
        )
      end
    end
  end
end
