-- SQL scopes for Tasker::WorkflowStepTransition
-- Generated on: 2025-07-05 23:51:32 UTC
-- Model file: app/models/tasker/workflow_step_transition.rb

-- Scope: recent
SELECT "tasker_workflow_step_transitions".* FROM "tasker_workflow_step_transitions" ORDER BY "tasker_workflow_step_transitions"."sort_key" DESC

-- Scope: to_state
-- NOTE: This scope requires parameters - SQL generated with dummy values
SELECT "tasker_workflow_step_transitions".* FROM "tasker_workflow_step_transitions" WHERE "tasker_workflow_step_transitions"."to_state" = 'pending'

-- Scope: with_metadata_key
-- NOTE: This scope requires parameters - SQL generated with dummy values
SELECT "tasker_workflow_step_transitions".* FROM "tasker_workflow_step_transitions" WHERE (metadata ? 'test_key')

-- Scope: for_task
-- NOTE: This scope requires parameters - SQL generated with dummy values
SELECT "tasker_workflow_step_transitions".* FROM "tasker_workflow_step_transitions" INNER JOIN "tasker_workflow_steps" ON "tasker_workflow_steps"."workflow_step_uuid" = "tasker_workflow_step_transitions"."workflow_step_uuid" WHERE "workflow_steps"."task_uuid" = 1
