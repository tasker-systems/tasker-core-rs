# frozen_string_literal: true

module DiamondWorkflow
  module StepHandlers
    # Diamond Branch B: Left parallel branch that squares the input
    class DiamondBranchBHandler < TaskerCore::StepHandler::Base
      def call(_task, sequence, _step)
        # Get result from diamond_start
        start_result = sequence.get('diamond_start')&.dig('result')
        raise 'Diamond start result not found' unless start_result

        # Square the start result (single parent operation)
        result = start_result * start_result

        logger.info "Diamond Branch B: #{start_result}² = #{result}"

        # Return result for convergence step
        TaskerCore::Types::StepHandlerCallResult.success(
          result: result,
          metadata: {
            operation: 'square',
            step_type: 'single_parent',
            input_refs: {
              start_result: 'sequence.diamond_start.result'
            },
            branch: 'left'
          }
        )
      end
    end
  end
end
