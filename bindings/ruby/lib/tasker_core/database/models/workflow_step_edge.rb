# frozen_string_literal: true

module TaskerCore
  module Database
    module Models
      class WorkflowStepEdge < ApplicationRecord
        self.primary_key = :workflow_step_edge_uuid
        belongs_to :from_step, class_name: 'WorkflowStep', foreign_key: :from_step_uuid,
                               primary_key: :workflow_step_uuid
        belongs_to :to_step, class_name: 'WorkflowStep', foreign_key: :to_step_uuid, primary_key: :workflow_step_uuid

        validates :name, presence: true

        before_create :ensure_no_cycles!

        scope :children_of, ->(step) { where(from_step: step) }
        scope :parents_of, ->(step) { where(to_step: step) }
        scope :siblings_of, ->(step) { find_by_sql(sibling_sql(step.workflow_step_uuid)) }
        scope :provides_edges, -> { where(name: WorkflowStep::PROVIDES_EDGE_NAME) }
        scope :provides_to_children, ->(step) { where(name: WorkflowStep::PROVIDES_EDGE_NAME, to_step: children_of(step)) }

        def self.create_edge!(from_step, to_step, name)
          create!(from_step: from_step, to_step: to_step, name: name)
        end

        def self.sibling_sql(step_uuid)
          sanitized_uuid = connection.quote(step_uuid)
          <<~SQL.squish
            WITH step_parents AS (
              SELECT from_step_uuid
              FROM tasker_workflow_step_edges
              WHERE to_step_uuid = #{sanitized_uuid}
            ),
            potential_siblings AS (
              SELECT to_step_uuid
              FROM tasker_workflow_step_edges
              WHERE from_step_uuid IN (SELECT from_step_uuid FROM step_parents)
              AND to_step_uuid != #{sanitized_uuid}
            ),
            siblings AS (
              SELECT to_step_uuid
              FROM tasker_workflow_step_edges
              WHERE to_step_uuid IN (SELECT to_step_uuid FROM potential_siblings)
              GROUP BY to_step_uuid
              HAVING ARRAY_AGG(from_step_uuid ORDER BY from_step_uuid) =
                    (SELECT ARRAY_AGG(from_step_uuid ORDER BY from_step_uuid) FROM step_parents)
            )
            SELECT e.*
            FROM tasker_workflow_step_edges e
            JOIN siblings ON e.to_step_uuid = siblings.to_step_uuid
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
              SELECT from_step_uuid, to_step_uuid
              FROM tasker_workflow_step_edges
              UNION ALL
              SELECT '#{from_step.workflow_step_uuid}'::uuid, '#{to_step.workflow_step_uuid}'::uuid
            ),
            path AS (
              -- Start with edges from to_step
              SELECT from_step_uuid, to_step_uuid, ARRAY[from_step_uuid] as path
              FROM all_edges
              WHERE from_step_uuid = '#{to_step.workflow_step_uuid}'::uuid

              UNION ALL

              -- Follow edges recursively
              SELECT e.from_step_uuid, e.to_step_uuid, p.path || e.from_step_uuid
              FROM all_edges e
              JOIN path p ON e.from_step_uuid = p.to_step_uuid
              WHERE NOT e.from_step_uuid = ANY(p.path) -- Avoid cycles in traversal
            )
            SELECT COUNT(*) as cycle_count
            FROM path
            WHERE to_step_uuid = '#{from_step.workflow_step_uuid}'::uuid
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
