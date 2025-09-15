# frozen_string_literal: true

require_relative 'function_wrapper'

module TaskerCore
  module Database
    module Functions
      # Function-based implementation of TaskExecutionContext
      # Maintains the same interface as the view-based model but uses SQL functions for performance
      class FunctionBasedTaskExecutionContext < FunctionWrapper
        # Define attributes to match the SQL function output
        attribute :task_uuid, :string
        attribute :named_task_uuid, :string
        attribute :status, :string
        attribute :total_steps, :integer
        attribute :pending_steps, :integer
        attribute :in_progress_steps, :integer
        attribute :completed_steps, :integer
        attribute :failed_steps, :integer
        attribute :ready_steps, :integer
        attribute :enqueued_steps, :integer
        attribute :execution_status, :string
        attribute :recommended_action, :string
        attribute :completion_percentage, :decimal
        attribute :health_status, :string

        # Class methods that use SQL functions
        def self.find(task_uuid)
          sql = 'SELECT * FROM get_task_execution_context($1::UUID)'
          binds = [task_uuid]
          single_from_sql_function(sql, binds, 'TaskExecutionContext Load')
        end

        def self.for_tasks(task_uuids)
          return [] if task_uuids.empty?

          # Use the batch function to avoid N+1 queries
          sql = 'SELECT * FROM get_task_execution_contexts_batch($1::UUID[])'
          binds = [task_uuids]
          from_sql_function(sql, binds, 'TaskExecutionContext Batch Load')
        end

        # Helper methods for workflow decision making (same as original model)
        def has_work_to_do?
          Constants::ACTIONABLE_TASK_EXECUTION_STATUSES.include?(execution_status) ||
            execution_status == Constants::TaskExecution::ExecutionStatus::PROCESSING
        end

        def is_blocked?
          execution_status == Constants::TaskExecution::ExecutionStatus::BLOCKED_BY_FAILURES
        end

        def is_complete?
          execution_status == Constants::TaskExecution::ExecutionStatus::ALL_COMPLETE
        end

        def is_healthy?
          health_status == Constants::TaskExecution::HealthStatus::HEALTHY
        end

        def needs_intervention?
          health_status == Constants::TaskExecution::HealthStatus::BLOCKED
        end

        def can_make_progress?
          ready_steps.positive?
        end

        def should_reenqueue?
          Constants::REENQUEUE_TASK_EXECUTION_STATUSES.include?(execution_status)
        end

        def needs_immediate_action?
          Constants::ACTIONABLE_TASK_EXECUTION_STATUSES.include?(execution_status)
        end

        # Status summary methods (same as original model)
        def workflow_summary
          {
            total_steps: total_steps,
            completed: completed_steps,
            pending: pending_steps,
            in_progress: in_progress_steps,
            failed: failed_steps,
            ready: ready_steps,
            completion_percentage: completion_percentage,
            status: execution_status,
            health: health_status,
            recommended_action: recommended_action,
            progress_details: progress_details
          }
        end

        def progress_details
          {
            completed_ratio: "#{completed_steps}/#{total_steps}",
            completion_percentage: "#{completion_percentage}%",
            remaining_steps: total_steps - completed_steps,
            failed_steps: failed_steps,
            ready_steps: ready_steps
          }
        end

        def next_action_details
          case recommended_action
          when Constants::TaskExecution::RecommendedAction::EXECUTE_READY_STEPS
            {
              action: Constants::TaskExecution::RecommendedAction::EXECUTE_READY_STEPS,
              description: "#{ready_steps} steps ready for execution",
              urgency: 'high',
              can_proceed: true
            }
          when Constants::TaskExecution::RecommendedAction::WAIT_FOR_COMPLETION
            {
              action: Constants::TaskExecution::RecommendedAction::WAIT_FOR_COMPLETION,
              description: "#{in_progress_steps} steps currently processing",
              urgency: 'low',
              can_proceed: false
            }
          when Constants::TaskExecution::RecommendedAction::HANDLE_FAILURES
            {
              action: Constants::TaskExecution::RecommendedAction::HANDLE_FAILURES,
              description: "#{failed_steps} failed steps blocking progress",
              urgency: 'critical',
              can_proceed: false
            }
          when Constants::TaskExecution::RecommendedAction::FINALIZE_TASK
            {
              action: Constants::TaskExecution::RecommendedAction::FINALIZE_TASK,
              description: 'All steps completed successfully',
              urgency: 'medium',
              can_proceed: true
            }
          else
            {
              action: Constants::TaskExecution::RecommendedAction::WAIT_FOR_DEPENDENCIES,
              description: 'Waiting for dependencies to be satisfied',
              urgency: 'low',
              can_proceed: false
            }
          end
        end

        def to_h
          {
            task_uuid: task_uuid,
            named_task_uuid: named_task_uuid,
            status: status,
            total_steps: total_steps,
            pending_steps: pending_steps,
            in_progress_steps: in_progress_steps,
            completed_steps: completed_steps,
            failed_steps: failed_steps,
            ready_steps: ready_steps,
            enqueued_steps: enqueued_steps,
            execution_status: execution_status,
            recommended_action: recommended_action,
            completion_percentage: completion_percentage,
            health_status: health_status,
            progress_details: progress_details,
            next_action_details: next_action_details
          }.deep_symbolize_keys
        end

        # Association (lazy-loaded)
        def task
          @task ||= TaskerCore::Database::Models::Task.find(task_uuid)
        end
      end
    end
  end
end
