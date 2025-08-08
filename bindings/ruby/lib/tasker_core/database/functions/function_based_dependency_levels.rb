# frozen_string_literal: true

require_relative 'function_wrapper'

module TaskerCore
  module Database
    module Functions
      # Function-based implementation of dependency level calculation
      # Uses SQL function for performance-optimized dependency level calculation
      class FunctionBasedDependencyLevels < FunctionWrapper
        # Define attributes to match the SQL function output
        attribute :workflow_step_id, :integer
        attribute :dependency_level, :integer

        # Class methods that use SQL functions
        def self.for_task(task_id)
          sql = 'SELECT * FROM calculate_dependency_levels($1::BIGINT)'
          binds = [task_id]
          from_sql_function(sql, binds, 'DependencyLevels Load')
        end

        def self.levels_hash_for_task(task_id)
          for_task(task_id).each_with_object({}) do |level_data, hash|
            hash[level_data.workflow_step_id] = level_data.dependency_level
          end
        end

        def self.max_level_for_task(task_id)
          for_task(task_id).map(&:dependency_level).max || 0
        end

        def self.steps_at_level(task_id, level)
          for_task(task_id).select { |data| data.dependency_level == level }
                           .map(&:workflow_step_id)
        end

        def self.root_steps_for_task(task_id)
          steps_at_level(task_id, 0)
        end

        # Instance methods
        def to_h
          {
            workflow_step_id: workflow_step_id,
            dependency_level: dependency_level
          }
        end

        # Associations (lazy-loaded)
        def workflow_step
          @workflow_step ||= TaskerCore::Database::Models::WorkflowStep.find(workflow_step_id)
        end
      end
    end
  end
end
