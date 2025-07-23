# frozen_string_literal: true

module TaskerCore
  module Models
    class Step
      attr_accessor :workflow_step_id, :task_id, :named_step_id, :name, :retryable, :retry_limit, :in_process, :processed, :processed_at, :attempts, :last_attempted_at, :backoff_request_seconds, :inputs, :results, :skippable, :created_at, :updated_at

      def to_h
        {
          workflow_step_id: workflow_step_id,
          task_id: task_id,
          named_step_id: named_step_id,
          name: name,
          retryable: retryable,
          retry_limit: retry_limit,
          in_process: in_process,
          processed: processed,
          processed_at: processed_at,
          attempts: attempts,
          last_attempted_at: last_attempted_at,
          backoff_request_seconds: backoff_request_seconds,
          inputs: inputs,
          results: results,
          skippable: skippable,
          created_at: created_at,
          updated_at: updated_at
        }
      end
    end
  end
end
