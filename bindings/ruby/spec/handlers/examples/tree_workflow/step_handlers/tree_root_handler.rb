# frozen_string_literal: true

module TreeWorkflow
  module StepHandlers
    # Tree Root: Initial step that squares the even number
    class TreeRootHandler < TaskerCore::StepHandler::Base
      def call(task, sequence, step)
        # Get the even number from task context
        even_number = task.context.dig("even_number")
        raise "Task context must contain an even number" unless even_number&.even?

        # Square the even number (first step operation)
        result = even_number * even_number

        logger.info "Tree Root: #{even_number}² = #{result}"

        # Return result for both main branches
        {
          status: "success",
          result: result,
          metadata: {
            operation: "square",
            input: even_number,
            output: result,
            step_type: "initial",
            branches: ["tree_branch_left", "tree_branch_right"]
          }
        }
      end
    end
  end
end