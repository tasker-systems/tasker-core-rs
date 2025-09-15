# frozen_string_literal: true

require_relative 'function_wrapper'

module TaskerCore
  module Database
    module Functions
      # Function-based implementation of dependency level calculation
      # Uses SQL function for performance-optimized dependency level calculation
      class FunctionBasedDependencyLevels < FunctionWrapper
        # Define attributes to match the SQL function output
        attribute :workflow_step_uuid, :string
        attribute :dependency_level, :integer

        # Class methods that use SQL functions
        def self.for_task(task_uuid)
          sql = 'SELECT * FROM calculate_dependency_levels($1::UUID)'
          binds = [task_uuid]
          from_sql_function(sql, binds, 'DependencyLevels Load')
        end

        def self.levels_hash_for_task(task_uuid)
          for_task(task_uuid).each_with_object({}) do |level_data, hash|
            hash[level_data.workflow_step_uuid] = level_data.dependency_level
          end
        end

        def self.max_level_for_task(task_uuid)
          for_task(task_uuid).map(&:dependency_level).max || 0
        end

        def self.steps_at_level(task_uuid, level)
          for_task(task_uuid).select { |data| data.dependency_level == level }
                             .map(&:workflow_step_uuid)
        end

        def self.root_steps_for_task(task_uuid)
          steps_at_level(task_uuid, 0)
        end

        # Instance methods
        def to_h
          {
            workflow_step_uuid: workflow_step_uuid,
            dependency_level: dependency_level
          }
        end

        # Associations (lazy-loaded)
        def workflow_step
          @workflow_step ||= TaskerCore::Database::Models::WorkflowStep.find(workflow_step_uuid)
        end
      end
    end
  end
end
