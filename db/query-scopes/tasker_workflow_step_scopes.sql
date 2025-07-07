-- SQL scopes for Tasker::WorkflowStep
-- Generated on: 2025-07-05 23:51:32 UTC
-- Model file: app/models/tasker/workflow_step.rb

-- Scope: completed
SELECT "tasker_workflow_steps".* FROM "tasker_workflow_steps" INNER JOIN "tasker_workflow_step_transitions" "workflow_step_transitions" ON "workflow_step_transitions"."workflow_step_id" = "tasker_workflow_steps"."workflow_step_id" WHERE "workflow_step_transitions"."most_recent" = TRUE AND "workflow_step_transitions"."to_state" IN ('complete', 'resolved_manually')

-- Scope: failed
SELECT "tasker_workflow_steps".* FROM "tasker_workflow_steps" INNER JOIN "tasker_workflow_step_transitions" "workflow_step_transitions" ON "workflow_step_transitions"."workflow_step_id" = "tasker_workflow_steps"."workflow_step_id" WHERE "workflow_step_transitions"."most_recent" = TRUE AND "workflow_step_transitions"."to_state" = 'error'

-- Scope: pending
-- NOTE: This scope requires parameters - SQL generated with dummy values
-- SKIPPED: Complex scope with .or() and incompatible joins

-- Scope: for_task
-- NOTE: This scope requires parameters - SQL generated with dummy values
SELECT "tasker_workflow_steps".* FROM "tasker_workflow_steps" WHERE "tasker_workflow_steps"."task_id" = 1

-- Scope: by_current_state
SELECT "tasker_workflow_steps".* FROM "tasker_workflow_steps" INNER JOIN ( SELECT DISTINCT ON (workflow_step_id) workflow_step_id, to_state FROM tasker_workflow_step_transitions WHERE most_recent = true ORDER BY workflow_step_id, sort_key DESC ) current_transitions ON current_transitions.workflow_step_id = tasker_workflow_steps.workflow_step_id

-- Scope: completed_since
-- NOTE: This scope requires parameters - SQL generated with dummy values
SELECT "tasker_workflow_steps".* FROM "tasker_workflow_steps" INNER JOIN "tasker_workflow_step_transitions" ON "tasker_workflow_step_transitions"."workflow_step_id" = "tasker_workflow_steps"."workflow_step_id" WHERE (tasker_workflow_step_transitions.most_recent = TRUE AND tasker_workflow_step_transitions.to_state = 'complete') AND (tasker_workflow_step_transitions.created_at > '2025-07-04 23:51:32.853710')

-- Scope: failed_since
-- NOTE: This scope requires parameters - SQL generated with dummy values
SELECT "tasker_workflow_steps".* FROM "tasker_workflow_steps" INNER JOIN "tasker_workflow_step_transitions" ON "tasker_workflow_step_transitions"."workflow_step_id" = "tasker_workflow_steps"."workflow_step_id" WHERE (tasker_workflow_step_transitions.most_recent = TRUE AND tasker_workflow_step_transitions.to_state = 'error') AND (tasker_workflow_step_transitions.created_at > '2025-07-04 23:51:32.854220')

-- Scope: for_tasks_since
-- NOTE: This scope requires parameters - SQL generated with dummy values
SELECT "tasker_workflow_steps".* FROM "tasker_workflow_steps" INNER JOIN "tasker_tasks" ON "tasker_tasks"."task_id" = "tasker_workflow_steps"."task_id" WHERE (tasker_tasks.created_at > '2025-07-04 23:51:32.854726')

