-- SQL scopes for Tasker::NamedTask
-- Generated on: 2025-07-05 23:51:32 UTC
-- Model file: app/models/tasker/named_task.rb

-- Scope: in_namespace
-- NOTE: This scope requires parameters - SQL generated with dummy values
SELECT "tasker_named_tasks".* FROM "tasker_named_tasks" INNER JOIN "tasker_task_namespaces" "task_namespace" ON "task_namespace"."task_namespace_id" = "tasker_named_tasks"."task_namespace_id" WHERE "task_namespace"."name" = 'default'

-- Scope: with_version
-- NOTE: This scope requires parameters - SQL generated with dummy values
SELECT "tasker_named_tasks".* FROM "tasker_named_tasks" WHERE "tasker_named_tasks"."version" = '1.0.0'

-- Scope: latest_versions
SELECT DISTINCT ON (task_namespace_id, name) * FROM "tasker_named_tasks" ORDER BY "tasker_named_tasks"."task_namespace_id" ASC, "tasker_named_tasks"."name" ASC, "tasker_named_tasks"."version" DESC

