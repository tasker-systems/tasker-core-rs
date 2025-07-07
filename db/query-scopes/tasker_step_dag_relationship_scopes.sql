-- SQL scopes for Tasker::StepDagRelationship
-- Generated on: 2025-07-05 23:51:32 UTC
-- Model file: app/models/tasker/step_dag_relationship.rb

-- Scope: for_task
-- NOTE: This scope requires parameters - SQL generated with dummy values
SELECT "tasker_step_dag_relationships".* FROM "tasker_step_dag_relationships" WHERE "tasker_step_dag_relationships"."task_id" = 1

-- Scope: root_steps
SELECT "tasker_step_dag_relationships".* FROM "tasker_step_dag_relationships" WHERE "tasker_step_dag_relationships"."is_root_step" = TRUE

-- Scope: leaf_steps
SELECT "tasker_step_dag_relationships".* FROM "tasker_step_dag_relationships" WHERE "tasker_step_dag_relationships"."is_leaf_step" = TRUE

-- Scope: with_parents
SELECT "tasker_step_dag_relationships".* FROM "tasker_step_dag_relationships" WHERE (parent_count > 0)

-- Scope: with_children
SELECT "tasker_step_dag_relationships".* FROM "tasker_step_dag_relationships" WHERE (child_count > 0)

-- Scope: siblings_of
-- NOTE: This scope requires parameters - SQL generated with dummy values
-- ERROR: Could not generate SQL with dummy parameters: wrong number of arguments (given 0, expected 1)
-- Could not generate SQL for this scope

