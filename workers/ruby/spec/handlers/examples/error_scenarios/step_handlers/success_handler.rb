# frozen_string_literal: true

module ErrorScenarios
  # SuccessHandler - Always succeeds (for mixed workflow testing)
  #
  # This handler always succeeds, useful for testing workflows that mix
  # success and failure scenarios.
  #
  # Expected behavior:
  # - Always completes successfully
  # - No errors raised
  # - Can be used before/after error handlers to test workflow composition
  class SuccessHandler < TaskerCore::StepHandler::Base
    # @param task [TaskerCore::Types::TaskSequenceStep] The task context
    # @param sequence [TaskerCore::Types::TaskSequenceStep] The sequence context
    # @param step [TaskerCore::Types::TaskSequenceStep] The current step
    # @return [Hash] Success result
    def call(_task, _sequence, step)
      TaskerCore::Logger.instance.log_step(
        :info,
        'success_handler_execution',
        step_uuid: step.workflow_step_uuid,
        step_name: step.name,
        message: 'Success handler executed successfully'
      )

      {
        status: 'success',
        message: 'Step completed successfully',
        timestamp: Time.now.utc.iso8601
      }
    end
  end
end
