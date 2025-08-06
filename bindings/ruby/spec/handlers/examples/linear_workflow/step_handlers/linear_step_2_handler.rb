# frozen_string_literal: true

module LinearWorkflow
  module StepHandlers
    # Second step in linear workflow: square the result from step 1
    class LinearStep2Handler < TaskerCore::StepHandler::Base
      def call(_task, sequence, _step)
        # Get result from previous step (linear_step_1)
        previous_result = sequence.get('linear_step_1')&.dig('result')
        raise 'Previous step result not found' unless previous_result

        # Add constant to previous result (based on config)
        constant = 10 # From YAML config
        result = previous_result + constant

        logger.info "Linear Step 2: #{previous_result} + #{constant} = #{result}"

        # Return standardized StepHandlerCallResult
        TaskerCore::Types::StepHandlerCallResult.success(
          result: result,
          metadata: {
            operation: 'add',
            constant: constant,
            step_type: 'intermediate',
            input_refs: {
              previous_result: 'sequence.linear_step_1.result'
            }
          }
        )
      end
    end
  end
end
