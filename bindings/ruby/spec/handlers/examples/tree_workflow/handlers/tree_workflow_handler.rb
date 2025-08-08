# frozen_string_literal: true

require_relative '../../../../../lib/tasker_core/task_handler/base'

module TreeWorkflow
  # Tree Workflow Handler
  # Implements A -> (B -> (D, E), C -> (F, G)) -> H pattern
  class TreeWorkflowHandler < TaskerCore::TaskHandler::Base
    def handle(task, _sequence, step)
      {
        status: 'success',
        message: "Tree workflow step #{step.step_name} completed",
        metadata: {
          workflow_type: 'tree',
          step_name: step.step_name,
          task_id: task.task_id
        }
      }
    end

    private

    def workflow_description
      'Complex tree workflow demonstrating hierarchical processing with multiple convergence points'
    end
  end
end
