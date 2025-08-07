# frozen_string_literal: true

module TaskerCore
  module Database
    module Models
      class StepDagRelationship < ApplicationRecord
        self.table_name = 'tasker_step_dag_relationships'
        self.primary_key = 'workflow_step_id'

        # Read-only model backed by database view
        def readonly?
          true
        end

        # Associations to actual models for additional data
        belongs_to :workflow_step
        belongs_to :task

        # Scopes for common DAG queries
        scope :for_task, ->(task_id) { where(task_id: task_id) }
        scope :root_steps, -> { where(is_root_step: true) }
        scope :leaf_steps, -> { where(is_leaf_step: true) }
        scope :with_parents, -> { where('parent_count > 0') }
        scope :with_children, -> { where('child_count > 0') }

        # Efficient siblings scope leveraging existing WorkflowStepEdge logic
        # Uses the sophisticated sibling_sql that finds steps with exactly the same parent set
        scope :siblings_of, lambda { |workflow_step_id|
          # Get sibling step IDs using the existing, well-tested WorkflowStepEdge logic
          sibling_edges = WorkflowStepEdge.siblings_of(WorkflowStep.find(workflow_step_id))
          sibling_step_ids = sibling_edges.pluck(:to_step_id).uniq

          # Return StepDagRelationship records for these siblings
          where(workflow_step_id: sibling_step_ids)
        }

        # Helper methods for DAG navigation
        def root_step?
          is_root_step
        end

        def leaf_step?
          is_leaf_step
        end

        def has_parents?
          parent_count.positive?
        end

        def has_children?
          child_count.positive?
        end

        # Parse JSONB arrays for relationship access
        def parent_step_ids_array
          return [] if parent_step_ids.blank?

          parent_step_ids.is_a?(Array) ? parent_step_ids : JSON.parse(parent_step_ids)
        end

        def child_step_ids_array
          return [] if child_step_ids.blank?

          child_step_ids.is_a?(Array) ? child_step_ids : JSON.parse(child_step_ids)
        end
      end
    end
  end
end
