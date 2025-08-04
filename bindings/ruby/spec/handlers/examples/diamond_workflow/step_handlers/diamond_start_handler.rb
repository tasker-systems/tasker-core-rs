# frozen_string_literal: true

module DiamondWorkflow
  module StepHandlers
    # Diamond Start: Initial step that squares the even number
    class DiamondStartHandler < TaskerCore::StepHandler::Base
      def call(task, sequence, step)
        # Get the even number from task context
        even_number = task.context.dig("even_number")
        raise "Task context must contain an even number" unless even_number&.even?

        # Square the even number (first step operation)
        result = even_number * even_number

        logger.info "Diamond Start: #{even_number}² = #{result}"

        # Return result for both parallel branches
        {
          status: "success",
          result: result,
          metadata: {
            operation: "square",
            input: even_number,
            output: result,
            step_type: "initial",
            branches: ["diamond_branch_b", "diamond_branch_c"]
          }
        }
      end
    end
  end
end