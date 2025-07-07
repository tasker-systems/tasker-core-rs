-- SQL scopes for Tasker::TaskTransition
-- Generated on: 2025-07-05 23:51:32 UTC
-- Model file: app/models/tasker/task_transition.rb

-- Scope: recent
SELECT "tasker_task_transitions".* FROM "tasker_task_transitions" ORDER BY "tasker_task_transitions"."sort_key" DESC

-- Scope: to_state
-- NOTE: This scope requires parameters - SQL generated with dummy values
SELECT "tasker_task_transitions".* FROM "tasker_task_transitions" WHERE "tasker_task_transitions"."to_state" = 'pending'

-- Scope: with_metadata_key
-- NOTE: This scope requires parameters - SQL generated with dummy values
SELECT "tasker_task_transitions".* FROM "tasker_task_transitions" WHERE (metadata ? 'test_key')

