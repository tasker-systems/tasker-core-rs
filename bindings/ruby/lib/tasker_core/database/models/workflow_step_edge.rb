# frozen_string_literal: true

module TaskerCore
  module Database
    module Models
      class WorkflowStepEdge < ApplicationRecord
        belongs_to :from_step, class_name: 'WorkflowStep'
        belongs_to :to_step, class_name: 'WorkflowStep'

        validates :name, presence: true

        before_create :ensure_no_cycles!

        scope :children_of, ->(step) { where(from_step: step) }
        scope :parents_of, ->(step) { where(to_step: step) }
        scope :siblings_of, ->(step) { find_by_sql(sibling_sql(step.workflow_step_id)) }
        scope :provides_edges, -> { where(name: WorkflowStep::PROVIDES_EDGE_NAME) }
        scope :provides_to_children, ->(step) { where(name: WorkflowStep::PROVIDES_EDGE_NAME, to_step: children_of(step)) }

        def self.create_edge!(from_step, to_step, name)
          create!(from_step: from_step, to_step: to_step, name: name)
        end

        def self.sibling_sql(step_id)
          sanitized_id = connection.quote(step_id)
          <<~SQL.squish
            WITH step_parents AS (
              SELECT from_step_id
              FROM tasker_workflow_step_edges
              WHERE to_step_id = #{sanitized_id}
            ),
            potential_siblings AS (
              SELECT to_step_id
              FROM tasker_workflow_step_edges
              WHERE from_step_id IN (SELECT from_step_id FROM step_parents)
              AND to_step_id != #{sanitized_id}
            ),
            siblings AS (
              SELECT to_step_id
              FROM tasker_workflow_step_edges
              WHERE to_step_id IN (SELECT to_step_id FROM potential_siblings)
              GROUP BY to_step_id
              HAVING ARRAY_AGG(from_step_id ORDER BY from_step_id) =
                    (SELECT ARRAY_AGG(from_step_id ORDER BY from_step_id) FROM step_parents)
            )
            SELECT e.*
            FROM tasker_workflow_step_edges e
            JOIN siblings ON e.to_step_id = siblings.to_step_id
          SQL
        end

        private

        def ensure_no_cycles!
          return unless from_step && to_step

          # Check for direct cycles first (A->B, B->A)
          if self.class.exists?(from_step: to_step, to_step: from_step)
            raise ActiveRecord::RecordInvalid.new(self), 'Adding this edge would create a cycle in the workflow'
          end

          # Check for indirect cycles (A->B->C->A)
          # Use a recursive CTE that includes our new edge
          cycle_sql = <<~SQL
            WITH RECURSIVE all_edges AS (
              -- Combine existing edges with our new edge
              SELECT from_step_id, to_step_id
              FROM tasker_workflow_step_edges
              UNION ALL
              SELECT #{from_step.id}::bigint, #{to_step.id}::bigint
            ),
            path AS (
              -- Start with edges from to_step
              SELECT from_step_id, to_step_id, ARRAY[from_step_id] as path
              FROM all_edges
              WHERE from_step_id = #{to_step.id}::bigint

              UNION ALL

              -- Follow edges recursively
              SELECT e.from_step_id, e.to_step_id, p.path || e.from_step_id
              FROM all_edges e
              JOIN path p ON e.from_step_id = p.to_step_id
              WHERE NOT e.from_step_id = ANY(p.path) -- Avoid cycles in traversal
            )
            SELECT COUNT(*) as cycle_count
            FROM path
            WHERE to_step_id = #{from_step.id}::bigint
          SQL

          result = self.class.connection.execute(cycle_sql).first
          if result['cycle_count'].to_i > 0 # rubocop:disable Style/NumericPredicate,Style/GuardClause
            raise ActiveRecord::RecordInvalid.new(self), 'Adding this edge would create a cycle in the workflow'
          end
        end
      end
    end
  end
end
