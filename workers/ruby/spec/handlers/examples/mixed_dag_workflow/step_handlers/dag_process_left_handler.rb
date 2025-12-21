# frozen_string_literal: true

module MixedDagWorkflow
  module StepHandlers
    # DAG Process Left: Squares the init result (step B in mixed DAG)
    class DagProcessLeftHandler < TaskerCore::StepHandler::Base
      def call(context)
        # Get result from dag_init
        init_result = context.get_dependency_result('dag_init')
        raise 'Init result not found' unless init_result

        # Square the init result (single parent operation)
        result = init_result * init_result

        logger.info "DAG Process Left: #{init_result}² = #{result}"

        # Return result for both validation (D) and transformation (E)
        TaskerCore::Types::StepHandlerCallResult.success(
          result: result,
          metadata: {
            operation: 'square',
            step_type: 'single_parent',
            input_refs: {
              init_result: 'sequence.dag_init.result'
            },
            branch: 'left',
            feeds_to: %w[dag_validate dag_transform]
          }
        )
      end
    end
  end
end
