# frozen_string_literal: true

module MixedDagWorkflow
  module StepHandlers
    # DAG Transform: Squares the left process result (step E in mixed DAG)
    class DagTransformHandler < TaskerCore::StepHandler::Base
      def call(_task, sequence, _step)
        # Get result from dag_process_left (single parent)
        left_result = sequence.get('dag_process_left')&.dig('result')
        raise 'Process left result not found' unless left_result

        # Square the left result (single parent operation)
        result = left_result * left_result

        logger.info "DAG Transform: #{left_result}² = #{result}"

        # Return result for final convergence
        TaskerCore::Types::StepHandlerCallResult.success(
          result: result,
          metadata: {
            operation: 'square',
            step_type: 'single_parent',
            input_refs: {
              left_result: 'sequence.dag_process_left.result'
            },
            transform_type: 'left_branch_square'
          }
        )
      end
    end
  end
end
