# frozen_string_literal: true

module ErrorScenarios
  # PermanentErrorHandler - Demonstrates permanent business logic failures
  #
  # This handler always raises a PermanentError, simulating scenarios like:
  # - Invalid payment method
  # - Invalid business rules
  # - Validation failures that cannot be retried
  #
  # Expected behavior:
  # - Step should transition to 'error' state
  # - No retries should be attempted
  # - Task should fail immediately
  class PermanentErrorHandler < TaskerCore::StepHandler::Base
    # @param task [TaskerCore::Types::TaskSequenceStep] The task context
    # @param sequence [TaskerCore::Types::TaskSequenceStep] The sequence context
    # @param step [TaskerCore::Types::TaskSequenceStep] The current step
    # @raise [TaskerCore::Errors::PermanentError] Always raises permanent error
    def call(task, sequence, step)
      TaskerCore::Logger.instance.log_step(
        :error,
        'permanent_failure_injection',
        step_uuid: step.workflow_step_uuid,
        step_name: step.name,
        message: 'Simulating permanent business logic failure'
      )

      # Simulate permanent business logic failure
      raise TaskerCore::Errors::PermanentError.new(
        "Invalid payment method: test_invalid_card",
        context: {
          error_code: 'INVALID_PAYMENT_METHOD',
          reason: 'Test failure scenario',
          card_type: 'test_invalid_card',
          expected_behavior: 'No retries, immediate failure'
        }
      )
    end
  end
end
