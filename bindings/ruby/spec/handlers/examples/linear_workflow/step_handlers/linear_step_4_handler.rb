# frozen_string_literal: true

module LinearWorkflow
  module StepHandlers
    # Fourth step in linear workflow: square the result from step 3 (final step)
    class LinearStep4Handler < TaskerCore::StepHandler::Base
      def call(task, sequence, step)
        # Get result from previous step (linear_step_3)
        previous_result = sequence.get("linear_step_3")&.dig("result")
        raise "Previous step result not found" unless previous_result

        # Square the previous result (single parent operation)
        result = previous_result * previous_result

        logger.info "Linear Step 4 (Final): #{previous_result}² = #{result}"

        # Calculate the full chain for verification
        original_number = task.context.dig("even_number")
        expected = original_number ** 8  # 2^4 = 8 (squaring 4 times)

        logger.info "Linear Workflow Complete: #{original_number} -> #{result}"
        logger.info "Verification: #{original_number}^8 = #{expected} (match: #{result == expected})"

        # Return final result
        TaskerCore::Types::StepHandlerCallResult.success(
          result: result,
          metadata: {
            operation: "square",
            step_type: "final",
            input_refs: {
              previous_result: "sequence.linear_step_3.result"
            },
            verification: {
              original_number: original_number,
              expected_result: expected,
              actual_result: result,
              matches: result == expected
            }
          }
        )
      end
    end
  end
end