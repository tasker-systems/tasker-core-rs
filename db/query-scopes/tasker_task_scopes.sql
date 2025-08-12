-- SQL scopes for Tasker::Task
-- Generated on: 2025-07-05 23:51:32 UTC
-- Model file: app/models/tasker/task.rb

-- Scope: by_annotation
-- NOTE: This scope requires parameters - SQL generated with dummy values
SELECT "tasker_tasks".* FROM "tasker_tasks" INNER JOIN "tasker_task_annotations" ON "tasker_task_annotations"."task_uuid" = "tasker_tasks"."task_uuid" INNER JOIN "tasker_task_annotations" "task_annotations_tasker_tasks_join" ON "task_annotations_tasker_tasks_join"."task_uuid" = "tasker_tasks"."task_uuid" INNER JOIN "tasker_annotation_types" "annotation_types" ON "annotation_types"."annotation_type_id" = "task_annotations_tasker_tasks_join"."annotation_type_id" WHERE "annotation_types"."name" = 'test_annotation' AND (tasker_task_annotations.annotation->>'test_key' = 'test_value')

-- Scope: by_current_state
SELECT "tasker_tasks".* FROM "tasker_tasks" INNER JOIN ( SELECT DISTINCT ON (task_uuid) task_uuid, to_state FROM tasker_task_transitions ORDER BY task_uuid, sort_key DESC ) current_transitions ON current_transitions.task_uuid = tasker_tasks.task_uuid

-- Scope: with_all_associated
SELECT "tasker_tasks".* FROM "tasker_tasks"

-- Scope: created_since
-- NOTE: This scope requires parameters - SQL generated with dummy values
SELECT "tasker_tasks".* FROM "tasker_tasks" WHERE (tasker_tasks.created_at > '2025-07-04 23:51:32.827412')

-- Scope: completed_since
-- NOTE: This scope requires parameters - SQL generated with dummy values
SELECT DISTINCT "tasker_tasks".* FROM "tasker_tasks" INNER JOIN "tasker_workflow_steps" ON "tasker_workflow_steps"."task_uuid" = "tasker_tasks"."task_uuid" INNER JOIN "tasker_workflow_step_transitions" ON "tasker_workflow_step_transitions"."workflow_step_uuid" = "tasker_workflow_steps"."workflow_step_uuid" WHERE (tasker_workflow_step_transitions.to_state = 'complete' AND tasker_workflow_step_transitions.most_recent = TRUE) AND (tasker_workflow_step_transitions.created_at > '2025-07-04 23:51:32.828048')

-- Scope: failed_since
-- NOTE: This scope requires parameters - SQL generated with dummy values
SELECT "tasker_tasks".* FROM "tasker_tasks" INNER JOIN ( SELECT DISTINCT ON (task_uuid) task_uuid, to_state, created_at FROM tasker_task_transitions ORDER BY task_uuid, sort_key DESC ) current_transitions ON current_transitions.task_uuid = tasker_tasks.task_uuid WHERE "current_transitions"."to_state" = 'error' AND (current_transitions.created_at > '2025-07-04 23:51:32.839971')

-- Scope: active
SELECT "tasker_tasks".* FROM "tasker_tasks" WHERE (EXISTS ( SELECT 1 FROM tasker_workflow_steps ws INNER JOIN tasker_workflow_step_transitions wst ON wst.workflow_step_uuid = ws.workflow_step_uuid WHERE ws.task_uuid = tasker_tasks.task_uuid AND wst.most_recent = true AND wst.to_state NOT IN ('complete', 'error', 'skipped', 'resolved_manually') ))

-- Scope: in_namespace
-- NOTE: This scope requires parameters - SQL generated with dummy values
SELECT "tasker_tasks".* FROM "tasker_tasks" INNER JOIN "tasker_named_tasks" ON "tasker_named_tasks"."named_task_uuid" = "tasker_tasks"."named_task_uuid" INNER JOIN "tasker_task_namespaces" ON "tasker_task_namespaces"."task_namespace_id" = "tasker_named_tasks"."task_namespace_id" WHERE "tasker_task_namespaces"."name" = 'default'

-- Scope: with_task_name
-- NOTE: This scope requires parameters - SQL generated with dummy values
SELECT "tasker_tasks".* FROM "tasker_tasks" INNER JOIN "tasker_named_tasks" ON "tasker_named_tasks"."named_task_uuid" = "tasker_tasks"."named_task_uuid" WHERE "tasker_named_tasks"."name" = 'test_task'

-- Scope: with_version
-- NOTE: This scope requires parameters - SQL generated with dummy values
SELECT "tasker_tasks".* FROM "tasker_tasks" INNER JOIN "tasker_named_tasks" ON "tasker_named_tasks"."named_task_uuid" = "tasker_tasks"."named_task_uuid" WHERE "tasker_named_tasks"."version" = '1.0.0'
