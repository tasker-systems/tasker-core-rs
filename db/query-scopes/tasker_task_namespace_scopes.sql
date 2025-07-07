-- SQL scopes for Tasker::TaskNamespace
-- Generated on: 2025-07-05 23:51:32 UTC
-- Model file: app/models/tasker/task_namespace.rb

-- Scope: custom
SELECT "tasker_task_namespaces".* FROM "tasker_task_namespaces" WHERE "tasker_task_namespaces"."name" != 'default'

