# frozen_string_literal: true

module TaskerCore
  module Models
    class Task
      attr_accessor :task_id, :named_task_id, :complete, :requested_at, :initiator, :source_system, :reason, :bypass_steps, :tags, :context, :identity_hash, :created_at, :updated_at

      def to_h
        {
          task_id: task_id,
          named_task_id: named_task_id,
          complete: complete,
          requested_at: requested_at,
          initiator: initiator,
          source_system: source_system,
          reason: reason,
          bypass_steps: bypass_steps,
          tags: tags,
          context: context,
          identity_hash: identity_hash,
          created_at: created_at,
          updated_at: updated_at
        }
      end
    end
  end
end
