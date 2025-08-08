# frozen_string_literal: true

require_relative '../../../../../lib/tasker_core/task_handler/base'

module MixedDagWorkflow
  # Mixed DAG Workflow Handler
  # Implements complex DAG: A -> B, A -> C, B -> D, C -> D, B -> E, C -> F, (D,E,F) -> G
  class MixedDagWorkflowHandler < TaskerCore::TaskHandler::Base
    def handle(task, _sequence, step)
      {
        status: 'success',
        message: "Mixed DAG workflow step #{step.step_name} completed",
        metadata: {
          workflow_type: 'mixed_dag',
          step_name: step.step_name,
          task_id: task.task_id
        }
      }
    end

    private

    def workflow_description
      'Complex mixed DAG workflow demonstrating multiple convergence patterns and dependency types'
    end
  end
end
