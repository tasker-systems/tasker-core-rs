# frozen_string_literal: true

module MixedDagWorkflow
  module StepHandlers
    # DAG Validate: Convergence step that multiplies results from both process branches
    class DagValidateHandler < TaskerCore::StepHandler::Base
      def call(task, sequence, step)
        # Get results from both process branches (multiple parents)
        left_result = sequence.get("dag_process_left")&.dig("result")
        right_result = sequence.get("dag_process_right")&.dig("result")

        raise "Process left result not found" unless left_result
        raise "Process right result not found" unless right_result

        # Multiple parent logic: multiply the results together, then square
        multiplied = left_result * right_result
        result = multiplied * multiplied

        logger.info "DAG Validate: (#{left_result} × #{right_result})² = #{multiplied}² = #{result}"

        # Return result for final convergence
        {
          status: "success",
          result: result,
          metadata: {
            operation: "multiply_and_square",
            inputs: {
              process_left: left_result,
              process_right: right_result
            },
            multiplied: multiplied,
            output: result,
            step_type: "multiple_parent",
            convergence_type: "dual_branch"
          }
        }
      end
    end
  end
end