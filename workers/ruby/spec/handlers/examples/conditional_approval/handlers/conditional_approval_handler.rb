# frozen_string_literal: true

require_relative '../../../../../lib/tasker_core/task_handler/base'

module ConditionalApproval
  # Conditional Approval Workflow Handler
  #
  # TAS-53: Demonstrates decision point functionality with dynamic workflow routing.
  #
  # This workflow shows how decision points enable runtime conditional branching:
  # 1. validate_request: Validates approval request data
  # 2. routing_decision: DECISION POINT - routes to appropriate approval path
  # 3. Dynamic branches (created by decision point):
  #    - auto_approve (< $1,000)
  #    - manager_approval ($1,000-$4,999 OR >= $5,000)
  #    - finance_review (>= $5,000, parallel with manager_approval)
  # 4. finalize_approval: Converges all paths and creates final approval record
  class ConditionalApprovalHandler < TaskerCore::TaskHandler::Base
    def handle(task, _sequence, step)
      {
        status: 'success',
        message: "Conditional approval workflow step #{step.step_name} completed",
        metadata: {
          workflow_type: 'conditional_approval',
          step_name: step.step_name,
          task_uuid: task.task_uuid,
          decision_point_enabled: true
        }
      }
    end

    private

    def workflow_description
      'Conditional approval workflow with decision point routing based on approval amount'
    end
  end
end
