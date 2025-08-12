-- SQL scopes for Tasker::WorkflowStepEdge
-- Generated on: 2025-07-05 23:51:32 UTC
-- Model file: app/models/tasker/workflow_step_edge.rb

-- Scope: children_of
-- NOTE: This scope requires parameters - SQL generated with dummy values
SELECT "tasker_workflow_step_edges".* FROM "tasker_workflow_step_edges" WHERE "tasker_workflow_step_edges"."from_step_id" = 1

-- Scope: parents_of
-- NOTE: This scope requires parameters - SQL generated with dummy values
SELECT "tasker_workflow_step_edges".* FROM "tasker_workflow_step_edges" WHERE "tasker_workflow_step_edges"."to_step_id" = 1

-- Scope: siblings_of
-- NOTE: This scope requires parameters - SQL generated with dummy values
-- ERROR: Could not generate SQL with dummy parameters: wrong number of arguments (given 0, expected 1)
-- Could not generate SQL for this scope

-- Scope: provides_edges
SELECT "tasker_workflow_step_edges".* FROM "tasker_workflow_step_edges" WHERE "tasker_workflow_step_edges"."name" = 'provides'

-- Scope: provides_to_children
-- NOTE: This scope requires parameters - SQL generated with dummy values
SELECT "tasker_workflow_step_edges".* FROM "tasker_workflow_step_edges" WHERE "tasker_workflow_step_edges"."name" = 'provides' AND "tasker_workflow_step_edges"."to_step_id" IN (SELECT workflow_step_uuid FROM "tasker_workflow_step_edges" WHERE "tasker_workflow_step_edges"."from_step_id" = NULL)
