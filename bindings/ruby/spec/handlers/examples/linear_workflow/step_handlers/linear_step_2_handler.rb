# frozen_string_literal: true

module LinearWorkflow
  module StepHandlers
    # Second step in linear workflow: square the result from step 1
    class LinearStep2Handler < TaskerCore::StepHandler::Base
      def call(task, sequence, step)
        # Get result from previous step (linear_step_1)
        previous_result = sequence.get("linear_step_1")&.dig("result")
        raise "Previous step result not found" unless previous_result

        # Square the previous result (single parent operation)
        result = previous_result * previous_result

        logger.info "Linear Step 2: #{previous_result}² = #{result}"

        # Return result for next step
        {
          status: "success",
          result: result,
          metadata: {
            operation: "square",
            input: previous_result,
            output: result,
            step_type: "single_parent"
          }
        }
      end
    end
  end
end