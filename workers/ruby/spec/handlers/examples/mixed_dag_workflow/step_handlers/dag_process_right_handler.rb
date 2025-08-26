# frozen_string_literal: true

module MixedDagWorkflow
  module StepHandlers
    # DAG Process Right: Squares the init result (step C in mixed DAG)
    class DagProcessRightHandler < TaskerCore::StepHandler::Base
      def call(_task, sequence, _step)
        # Get result from dag_init
        init_result = sequence.get_results('dag_init')
        raise 'Init result not found' unless init_result

        # Square the init result (single parent operation)
        result = init_result * init_result

        logger.info "DAG Process Right: #{init_result}² = #{result}"

        # Return result for both validation (D) and analysis (F)
        TaskerCore::Types::StepHandlerCallResult.success(
          result: result,
          metadata: {
            operation: 'square',
            step_type: 'single_parent',
            input_refs: {
              init_result: 'sequence.dag_init.result'
            },
            branch: 'right',
            feeds_to: %w[dag_validate dag_analyze]
          }
        )
      end
    end
  end
end
