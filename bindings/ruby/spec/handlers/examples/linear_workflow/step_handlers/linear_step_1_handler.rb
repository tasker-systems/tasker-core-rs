# frozen_string_literal: true

module LinearWorkflow
  module StepHandlers
    # First step in linear workflow: square the initial even number
    class LinearStep1Handler < TaskerCore::StepHandler::Base
      def call(task, sequence, step)
        # Get the even number from task context
        even_number = task.context.dig("even_number")
        raise "Task context must contain an even number" unless even_number&.even?

        # Square the even number (first step operation)
        result = even_number * even_number

        logger.info "Linear Step 1: #{even_number}² = #{result}"

        # Return result for next step
        {
          status: "success",
          result: result,
          metadata: {
            operation: "square",
            input: even_number,
            output: result,
            step_type: "initial"
          }
        }
      end
    end
  end
end