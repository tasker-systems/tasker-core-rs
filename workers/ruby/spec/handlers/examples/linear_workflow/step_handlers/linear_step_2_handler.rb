# frozen_string_literal: true

module LinearWorkflow
  module StepHandlers
    # Second step in linear workflow: square the result from step 1
    class LinearStep2Handler < TaskerCore::StepHandler::Base
      def call(context)
        # Get result from previous step (linear_step_1)
        previous_result = context.get_dependency_result('linear_step_1')
        raise 'Previous step result not found' unless previous_result

        result = previous_result * previous_result

        logger.info "Linear Step 2: #{previous_result} * #{previous_result} = #{result}"

        # Return standardized StepHandlerCallResult
        TaskerCore::Types::StepHandlerCallResult.success(
          result: result,
          metadata: {
            operation: 'square',
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
