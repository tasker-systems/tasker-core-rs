# frozen_string_literal: true

module LinearWorkflow
  module StepHandlers
    # Third step in linear workflow: square the result from step 2
    class LinearStep3Handler < TaskerCore::StepHandler::Base
      def call(task, sequence, step)
        # Get result from previous step (linear_step_2)
        previous_result = sequence.get("linear_step_2")&.dig("result")
        raise "Previous step result not found" unless previous_result

        # Square the previous result (single parent operation)
        result = previous_result * previous_result

        logger.info "Linear Step 3: #{previous_result}² = #{result}"

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