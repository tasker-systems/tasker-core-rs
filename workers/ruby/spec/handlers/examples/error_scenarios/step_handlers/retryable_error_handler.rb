# frozen_string_literal: true

module ErrorScenarios
  # RetryableErrorHandler - Demonstrates retryable transient failures
  #
  # This handler always raises a RetryableError, simulating scenarios like:
  # - Network timeouts
  # - Temporary service unavailability
  # - Rate limiting
  #
  # Expected behavior:
  # - Step should retry up to configured max_attempts
  # - Exponential backoff should be applied
  # - Eventually should transition to 'error' state after retry exhaustion
  class RetryableErrorHandler < TaskerCore::StepHandler::Base
    # @param task [TaskerCore::Types::TaskSequenceStep] The task context
    # @param sequence [TaskerCore::Types::TaskSequenceStep] The sequence context
    # @param step [TaskerCore::Types::TaskSequenceStep] The current step
    # @raise [TaskerCore::Errors::RetryableError] Always raises retryable error
    def call(context)
      retry_count = context.workflow_step.results&.dig('retry_count') || 0

      TaskerCore::Logger.instance.log_step(
        :warn,
        'retryable_failure_injection',
        step_uuid: context.step_uuid,
        step_name: context.workflow_step.name,
        retry_count: retry_count,
        message: 'Simulating retryable transient failure'
      )

      # Simulate network timeout or service unavailability
      raise TaskerCore::Errors::RetryableError.new(
        "Payment service timeout after 30s (attempt #{retry_count + 1})",
        retry_after: 5,
        context: {
          error_code: 'SERVICE_TIMEOUT',
          reason: 'Test failure scenario',
          service: 'payment_gateway',
          timeout_seconds: 30,
          retry_count: retry_count,
          expected_behavior: 'Retry with exponential backoff'
        }
      )
    end
  end
end
