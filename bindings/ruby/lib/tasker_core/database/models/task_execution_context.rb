# frozen_string_literal: true

module TaskerCore
  module Database
    module Models
      # TaskExecutionContext now uses SQL functions for high-performance queries
      # This class explicitly delegates to the function-based implementation for better maintainability
      class TaskExecutionContext
        # Explicit delegation of class methods to function-based implementation
        def self.find(task_uuid)
          TaskerCore::Database::Functions::FunctionBasedTaskExecutionContext.find(task_uuid)
        end

        def self.for_tasks(task_uuids)
          TaskerCore::Database::Functions::FunctionBasedTaskExecutionContext.for_tasks(task_uuids)
        end

        # For backward compatibility, maintain the active method but point to function-based implementation
        def self.active
          TaskerCore::Database::Functions::FunctionBasedTaskExecutionContext
        end

        def initialize(task_uuid)
          @task_uuid = task_uuid
        end

        def workflow_summary
          @workflow_summary ||= TaskerCore::Database::Functions::FunctionBasedTaskExecutionContext.find(@task_uuid).workflow_summary
        end
      end
    end
  end
end
