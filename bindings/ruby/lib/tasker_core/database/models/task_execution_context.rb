# frozen_string_literal: true

module TaskerCore
  module Database
    module Models
      # TaskExecutionContext now uses SQL functions for high-performance queries
      # This class explicitly delegates to the function-based implementation for better maintainability
      class TaskExecutionContext
        # Explicit delegation of class methods to function-based implementation
        def self.find(task_id)
          TaskerCore::Database::Functions::FunctionBasedTaskExecutionContext.find(task_id)
        end

        def self.for_tasks(task_ids)
          TaskerCore::Database::Functions::FunctionBasedTaskExecutionContext.for_tasks(task_ids)
        end

        # For backward compatibility, maintain the active method but point to function-based implementation
        def self.active
          TaskerCore::Database::Functions::FunctionBasedTaskExecutionContext
        end

        def initialize(task_id)
          @task_id = task_id
        end

        def workflow_summary
          @workflow_summary ||= TaskerCore::Database::Functions::FunctionBasedTaskExecutionContext.find(@task_id).workflow_summary
        end
      end
    end
  end
end
