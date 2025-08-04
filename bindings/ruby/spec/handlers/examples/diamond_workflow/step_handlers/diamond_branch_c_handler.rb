# frozen_string_literal: true

module DiamondWorkflow
  module StepHandlers
    # Diamond Branch C: Right parallel branch that squares the input
    class DiamondBranchCHandler < TaskerCore::StepHandler::Base
      def call(task, sequence, step)
        # Get result from diamond_start
        start_result = sequence.get("diamond_start")&.dig("result")
        raise "Diamond start result not found" unless start_result

        # Square the start result (single parent operation)
        result = start_result * start_result

        logger.info "Diamond Branch C: #{start_result}² = #{result}"

        # Return result for convergence step
        TaskerCore::Types::StepHandlerCallResult.success(
          result: result,
          metadata: {
            operation: "square",
            step_type: "single_parent",
            input_refs: {
              start_result: "sequence.diamond_start.result"
            },
            branch: "right"
          }
        )
      end
    end
  end
end