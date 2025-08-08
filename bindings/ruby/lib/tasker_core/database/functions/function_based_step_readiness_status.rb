# frozen_string_literal: true

require_relative 'function_wrapper'

module TaskerCore
  module Database
    module Functions
      # Function-based implementation of StepReadinessStatus
      # Maintains the same interface as the view-based model but uses SQL functions for performance
      class FunctionBasedStepReadinessStatus < FunctionWrapper
        # Define attributes to match the SQL function output
        attribute :workflow_step_id, :integer
        attribute :task_id, :integer
        attribute :named_step_id, :integer
        attribute :name, :string
        attribute :current_state, :string
        attribute :dependencies_satisfied, :boolean
        attribute :retry_eligible, :boolean
        attribute :ready_for_execution, :boolean
        attribute :last_failure_at, :datetime
        attribute :next_retry_at, :datetime
        attribute :total_parents, :integer
        attribute :completed_parents, :integer
        attribute :attempts, :integer
        attribute :retry_limit, :integer
        attribute :backoff_request_seconds, :integer
        attribute :last_attempted_at, :datetime

        # Class methods that use SQL functions
        def self.for_task(task_id, step_ids = nil)
          sql = 'SELECT * FROM get_step_readiness_status($1::BIGINT, $2::BIGINT[])'
          binds = [task_id, step_ids]
          from_sql_function(sql, binds, 'StepReadinessStatus Load')
        end

        def self.for_steps(task_id, step_ids)
          for_task(task_id, step_ids)
        end

        def self.for_tasks(task_ids)
          return [] if task_ids.empty?

          # Use the batch function to get steps for multiple tasks efficiently
          sql = 'SELECT * FROM get_step_readiness_status_batch($1::BIGINT[])'
          binds = [task_ids]
          from_sql_function(sql, binds, 'StepReadinessStatus Batch Load')
        end

        def self.ready_for_task(task_id)
          for_task(task_id).select(&:ready_for_execution)
        end

        def self.blocked_by_dependencies_for_task(task_id)
          for_task(task_id).reject(&:dependencies_satisfied)
        end

        def self.blocked_by_retry_for_task(task_id)
          for_task(task_id).reject(&:retry_eligible)
        end

        def self.pending_for_task(task_id)
          for_task(task_id).select { |s| s.current_state == 'pending' }
        end

        def self.failed_for_task(task_id)
          for_task(task_id).select { |s| s.current_state == 'error' }
        end

        def self.in_progress_for_task(task_id)
          for_task(task_id).select { |s| s.current_state == 'in_progress' }
        end

        def self.complete_for_task(task_id)
          for_task(task_id).select { |s| s.current_state.in?(%w[complete resolved_manually]) }
        end

        # Instance methods (same as original model)
        def can_execute_now?
          ready_for_execution
        end

        def blocking_reason
          return nil if ready_for_execution
          return 'dependencies_not_satisfied' unless dependencies_satisfied
          return 'retry_not_eligible' unless retry_eligible
          return 'invalid_state' unless %w[pending error].include?(current_state)

          'unknown'
        end

        def time_until_ready
          return 0 if ready_for_execution
          return nil unless next_retry_at

          [(next_retry_at - Time.current).to_i, 0].max
        end

        def dependency_status
          if total_parents.zero?
            'no_dependencies'
          elsif dependencies_satisfied
            'all_satisfied'
          else
            "#{completed_parents || 0}/#{total_parents || 0}_satisfied"
          end
        end

        def retry_status
          if attempts >= retry_limit
            'max_retries_reached'
          elsif retry_eligible
            'retry_eligible'
          else
            'waiting_for_backoff'
          end
        end

        def backoff_type
          if backoff_request_seconds.present? && last_attempted_at.present?
            'explicit_backoff'
          elsif last_failure_at.present?
            'exponential_backoff'
          else
            'no_backoff'
          end
        end

        def effective_backoff_seconds
          if backoff_request_seconds.present?
            backoff_request_seconds
          elsif attempts.present? && attempts.positive?
            # Calculate default exponential backoff (base_delay * 2^attempts, capped at 30)
            [2**attempts, 30].min
          else
            0
          end
        end

        def to_h
          {
            workflow_step_id: workflow_step_id,
            task_id: task_id,
            named_step_id: named_step_id,
            name: name,
            current_state: current_state,
            dependencies_satisfied: dependencies_satisfied,
            retry_eligible: retry_eligible,
            ready_for_execution: ready_for_execution,
            last_failure_at: last_failure_at,
            next_retry_at: next_retry_at,
            total_parents: total_parents,
            completed_parents: completed_parents,
            attempts: attempts,
            retry_limit: retry_limit,
            backoff_request_seconds: backoff_request_seconds,
            last_attempted_at: last_attempted_at,
            detailed_status: detailed_status
          }
        end

        def detailed_status
          {
            ready: ready_for_execution,
            current_state: current_state,
            dependencies: dependency_status,
            retry: retry_status,
            blocking_reason: blocking_reason,
            time_until_ready: time_until_ready,
            backoff_type: backoff_type,
            effective_backoff_seconds: effective_backoff_seconds
          }
        end

        # Associations (lazy-loaded)
        def workflow_step
          @workflow_step ||= TaskerCore::Database::Models::WorkflowStep.find(workflow_step_id)
        end

        def task
          @task ||= TaskerCore::Database::Models::Task.find(task_id)
        end
      end
    end
  end
end
