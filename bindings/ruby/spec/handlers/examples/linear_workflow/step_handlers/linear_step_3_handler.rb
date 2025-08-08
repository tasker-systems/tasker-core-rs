# frozen_string_literal: true

module LinearWorkflow
  module StepHandlers
    # Third step in linear workflow: square the result from step 2
    class LinearStep3Handler < TaskerCore::StepHandler::Base
      def call(_task, sequence, _step)
        # Get result from previous step (linear_step_2)
        previous_result = sequence.get_results('linear_step_2')
        raise 'Previous step result not found' unless previous_result

        # Square the previous result (single parent operation)
        result = previous_result * previous_result

        logger.info "Linear Step 3: #{previous_result}² = #{result}"

        # Return standardized StepHandlerCallResult
        TaskerCore::Types::StepHandlerCallResult.success(
          result: result,
          metadata: {
            operation: 'square',
            step_type: 'single_parent',
            input_refs: {
              previous_result: 'sequence.linear_step_2.result'
            }
          }
        )
      end
    end
  end
end
