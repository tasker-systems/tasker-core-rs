# frozen_string_literal: true

module MixedDagWorkflow
  module StepHandlers
    # DAG Init: Initial step that squares the even number for mixed DAG
    class DagInitHandler < TaskerCore::StepHandler::Base
      def call(task, sequence, step)
        # Get the even number from task context
        even_number = task.context.dig("even_number")
        raise "Task context must contain an even number" unless even_number&.even?

        # Square the even number (first step operation)
        result = even_number * even_number

        logger.info "DAG Init: #{even_number}² = #{result}"

        # Return result for both process branches
        {
          status: "success",
          result: result,
          metadata: {
            operation: "square",
            input: even_number,
            output: result,
            step_type: "initial",
            branches: ["dag_process_left", "dag_process_right"]
          }
        }
      end
    end
  end
end