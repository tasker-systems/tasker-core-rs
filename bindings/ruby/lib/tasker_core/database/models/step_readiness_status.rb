# frozen_string_literal: true

module TaskerCore
  module Database
    module Models
      # StepReadinessStatus now uses SQL functions for high-performance queries
      # This class explicitly delegates to the function-based implementation for better maintainability
      class StepReadinessStatus
        # Explicit delegation of class methods to function-based implementation
        def self.for_task(task_uuid, step_uuids = nil)
          TaskerCore::Database::Functions::FunctionBasedStepReadinessStatus.for_task(task_uuid, step_uuids)
        end

        def self.for_steps(task_uuid, step_uuids)
          TaskerCore::Database::Functions::FunctionBasedStepReadinessStatus.for_steps(task_uuid, step_uuids)
        end

        def self.for_tasks(task_uuids)
          TaskerCore::Database::Functions::FunctionBasedStepReadinessStatus.for_tasks(task_uuids)
        end

        # Task-scoped methods that require task_uuid parameter
        def self.ready_for_task(task_uuid)
          TaskerCore::Database::Functions::FunctionBasedStepReadinessStatus.ready_for_task(task_uuid)
        end

        def self.blocked_by_dependencies_for_task(task_uuid)
          TaskerCore::Database::Functions::FunctionBasedStepReadinessStatus.blocked_by_dependencies_for_task(task_uuid)
        end

        def self.blocked_by_retry_for_task(task_uuid)
          TaskerCore::Database::Functions::FunctionBasedStepReadinessStatus.blocked_by_retry_for_task(task_uuid)
        end

        def self.pending_for_task(task_uuid)
          TaskerCore::Database::Functions::FunctionBasedStepReadinessStatus.pending_for_task(task_uuid)
        end

        def self.failed_for_task(task_uuid)
          TaskerCore::Database::Functions::FunctionBasedStepReadinessStatus.failed_for_task(task_uuid)
        end

        def self.in_progress_for_task(task_uuid)
          TaskerCore::Database::Functions::FunctionBasedStepReadinessStatus.in_progress_for_task(task_uuid)
        end

        def self.complete_for_task(task_uuid)
          TaskerCore::Database::Functions::FunctionBasedStepReadinessStatus.complete_for_task(task_uuid)
        end

        # For backward compatibility, maintain the active method but point to function-based implementation
        def self.active
          TaskerCore::Database::Functions::FunctionBasedStepReadinessStatus
        end

        def self.all_steps_complete_for_task?(task)
          complete = TaskerCore::Database::Functions::FunctionBasedStepReadinessStatus.complete_for_task(task.task_uuid)
          complete.length == task.workflow_steps.count
        end
      end
    end
  end
end
