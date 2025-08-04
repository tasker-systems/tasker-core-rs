# frozen_string_literal: true

require_relative '../../../../../lib/tasker_core/task_handler/base'

module DiamondWorkflow
  # Diamond Workflow Handler
  # Implements A -> (B, C) -> D pattern with mathematical operations
  class DiamondWorkflowHandler < TaskerCore::TaskHandler::Base
    def initialize(task_config:)
      super(task_config: task_config)
    end

    def handle(task, sequence, step)
      {
        status: "success",
        message: "Diamond workflow step #{step.step_name} completed",
        metadata: {
          workflow_type: "diamond",
          step_name: step.step_name,
          task_id: task.task_id
        }
      }
    end

    private

    def workflow_description
      "Diamond workflow demonstrating parallel processing with convergence and multiple parent logic"
    end
  end
end