# frozen_string_literal: true

module ErrorScenarios
  # ErrorTestingHandler - Task handler for error scenario testing
  #
  # Coordinates error testing workflows to validate:
  # - Permanent error handling (no retries)
  # - Retryable error handling (with retry exhaustion)
  # - Intermittent failure recovery (eventual success)
  # - Simple retry patterns (fail-once-then-succeed)
  #
  # This handler is minimal because step orchestration is handled by
  # the Rust foundation. It primarily exists to satisfy the task handler
  # requirement and provide a place for task-level logging.
  class ErrorTestingHandler < TaskerCore::TaskHandler::Base
    # Called when task is initialized
    #
    # @param task [TaskerCore::Types::TaskSequenceStep] The task being initialized
    # @return [void]
    def on_initialize(task)
      TaskerCore::Logger.instance.log_task(
        :info,
        'error_testing_workflow_initialized',
        task_uuid: task.task_uuid,
        namespace: task.namespace,
        name: task.name,
        message: 'Error testing workflow initialized - testing failure scenarios'
      )
    end

    # Called when task completes successfully
    #
    # @param task [TaskerCore::Types::TaskSequenceStep] The completed task
    # @return [void]
    def on_complete(task)
      TaskerCore::Logger.instance.log_task(
        :info,
        'error_testing_workflow_complete',
        task_uuid: task.task_uuid,
        message: 'Error testing workflow completed - all error scenarios handled correctly'
      )
    end

    # Called when task fails
    #
    # @param task [TaskerCore::Types::TaskSequenceStep] The failed task
    # @param error [Exception] The error that caused failure
    # @return [void]
    def on_error(task, error)
      TaskerCore::Logger.instance.log_task(
        :error,
        'error_testing_workflow_failed',
        task_uuid: task.task_uuid,
        error_class: error.class.name,
        error_message: error.message,
        message: 'Error testing workflow failed - error scenario validation detected failure'
      )
    end
  end
end
