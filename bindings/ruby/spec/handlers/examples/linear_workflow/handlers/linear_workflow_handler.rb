# frozen_string_literal: true

module LinearWorkflow
  # Linear Workflow Handler
  # Implements A -> B -> C -> D pattern with mathematical operations
  class LinearWorkflowHandler < TaskerCore::BaseTaskHandler
    def initialize(task_config:)
      super(task_config: task_config)
    end

    def handle(task, sequence, step)
      {
        status: "success",
        message: "Linear workflow step #{step.step_name} completed",
        metadata: {
          workflow_type: "linear",
          step_name: step.step_name,
          task_id: task.task_id
        }
      }
    end

    private

    def workflow_description
      "Linear workflow that processes an even number through sequential mathematical operations"
    end
  end
end