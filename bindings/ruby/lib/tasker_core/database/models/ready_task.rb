# frozen_string_literal: true

module TaskerCore
  module Database
    module Models
      # Model backed by the tasker_ready_tasks view
      # Provides a convenient ActiveRecord interface to query tasks that are ready for execution
      class ReadyTask < ApplicationRecord
        self.table_name = 'tasker_ready_tasks'
        self.primary_key = 'task_uuid'

        # Read-only model backed by database view
        def readonly?
          true
        end

        # Associations to actual models for additional data
        belongs_to :task, foreign_key: 'task_uuid', primary_key: 'task_uuid'

        # Scopes for common ready task queries
        scope :available, -> { where(claim_status: 'available') }
        scope :claimed, -> { where(claim_status: 'claimed') }
        scope :for_namespace, ->(namespace) { where(namespace_name: namespace) }
        scope :with_ready_steps, -> { where('ready_steps_count > 0') }
        scope :by_priority, -> { order(:priority) }
        scope :by_created_at, -> { order(:created_at) }

        # Convenience methods
        def available?
          claim_status == 'available'
        end

        def claimed?
          claim_status == 'claimed'
        end

        def has_ready_steps?
          ready_steps_count.to_i.positive?
        end

        # Status checks
        def ready_for_execution?
          available? && has_ready_steps?
        end

        def processing?
          execution_status == 'in_progress'
        end

        def needs_steps?
          execution_status == 'has_ready_steps'
        end
      end
    end
  end
end
