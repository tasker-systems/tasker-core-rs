# frozen_string_literal: true

module LinearWorkflow
  module StepHandlers
    # First step in linear workflow: square the initial even number
    class LinearStep1Handler < TaskerCore::StepHandler::Base
      def call(task, _sequence, _step)
        logger.info("Starting Linear Step 1, with task context: #{task.context.inspect}")
        # Get the even number from task context
        even_number = task.context['even_number']
        raise 'Task context must contain an even number' unless even_number&.even?

        # Square the even number (first step operation)
        result = even_number * even_number

        logger.info "Linear Step 1: #{even_number}² = #{result}"

        # Return standardized StepHandlerCallResult
        TaskerCore::Types::StepHandlerCallResult.success(
          result: result,
          metadata: {
            operation: 'square',
            step_type: 'initial',
            input_refs: {
              even_number: 'task.context.even_number'
            }
          }
        )
      end
    end
  end
end
