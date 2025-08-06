# frozen_string_literal: true

module MixedDagWorkflow
  module StepHandlers
    # DAG Analyze: Squares the right process result (step F in mixed DAG)
    class DagAnalyzeHandler < TaskerCore::StepHandler::Base
      def call(_task, sequence, _step)
        # Get result from dag_process_right (single parent)
        right_result = sequence.get('dag_process_right')&.dig('result')
        raise 'Process right result not found' unless right_result

        # Square the right result (single parent operation)
        result = right_result * right_result

        logger.info "DAG Analyze: #{right_result}² = #{result}"

        # Return result for final convergence
        TaskerCore::Types::StepHandlerCallResult.success(
          result: result,
          metadata: {
            operation: 'square',
            step_type: 'single_parent',
            input_refs: {
              right_result: 'sequence.dag_process_right.result'
            },
            analysis_type: 'right_branch_square'
          }
        )
      end
    end
  end
end
