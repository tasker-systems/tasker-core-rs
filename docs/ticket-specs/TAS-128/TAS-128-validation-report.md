# TAS-128 Schema Validation Report

**Date**: 2026-01-10  
**Comparison**: pg17 pre-flattening → pg18 post-flattening

## Executive Summary

**✓ ALL SQL FUNCTIONS CORRECTLY TRANSLATED**

After stripping comments, all 26 core Tasker functions have **identical executable code** after applying the expected namespace transformations:
- `public.tasker_*` → `tasker.*` (table references)
- `public.function_name` → `tasker.function_name` (function references)

The differences flagged in the initial comparison were **comments only**, not logic changes.

---

## Function Validation

### Core Tasker Functions (26 total) - ALL MATCHED ✓

| Function | Status |
|----------|--------|
| calculate_dependency_levels | ✓ Identical |
| calculate_staleness_threshold | ✓ Identical |
| calculate_step_next_retry_time | ✓ Identical |
| create_dlq_entry | ✓ Identical |
| create_step_result_audit | ✓ Identical |
| detect_and_transition_stale_tasks | ✓ Identical |
| evaluate_step_state_readiness | ✓ Identical |
| find_stuck_tasks | ✓ Identical |
| get_analytics_metrics | ✓ Identical |
| get_current_task_state | ✓ Identical |
| get_next_ready_task | ✓ Identical |
| get_next_ready_tasks | ✓ Identical |
| get_slowest_steps | ✓ Identical |
| get_slowest_tasks | ✓ Identical |
| get_stale_tasks_for_dlq | ✓ Identical |
| get_step_readiness_status | ✓ Identical |
| get_step_readiness_status_batch | ✓ Identical |
| get_step_transitive_dependencies | ✓ Identical |
| get_system_health_counts | ✓ Identical |
| get_task_execution_context | ✓ Identical |
| get_task_execution_contexts_batch | ✓ Identical |
| get_task_ready_info | ✓ Identical |
| set_step_backoff_atomic | ✓ Identical |
| transition_stale_task_to_error | ✓ Identical |
| transition_task_state_atomic | ✓ Identical |
| update_updated_at_column | ✓ Identical |

### PGMQ Functions (8 functions)

All PGMQ helper functions exist in both schemas:

| Function | public schema | tasker schema |
|----------|--------------|---------------|
| pgmq_auto_add_headers_trigger | ✓ | ✓ (duplicate) |
| pgmq_delete_specific_message | ✓ | ✓ (duplicate) |
| pgmq_ensure_headers_column | ✓ | ✓ (duplicate) |
| pgmq_extend_vt_specific_message | ✓ | ✓ (duplicate) |
| pgmq_notify_queue_created | ✓ | ✓ (duplicate) |
| pgmq_read_specific_message | ✓ | ✓ (duplicate) |
| pgmq_send_batch_with_notify | ✓ | ✓ (duplicate) |
| pgmq_send_with_notify | ✓ | ✓ (duplicate) |

**⚠️ Note**: PGMQ functions are duplicated in both `public` and `tasker` schemas. Consider whether this is intentional for backward compatibility or should be consolidated.

---

## Table Validation

### Tables Matched (15 total) ✓

| Old Name | New Name |
|----------|----------|
| public.tasker_annotation_types | tasker.annotation_types |
| public.tasker_dependent_system_object_maps | tasker.dependent_system_object_maps |
| public.tasker_dependent_systems | tasker.dependent_systems |
| public.tasker_named_steps | tasker.named_steps |
| public.tasker_named_tasks | tasker.named_tasks |
| public.tasker_named_tasks_named_steps | tasker.named_tasks_named_steps |
| public.tasker_task_annotations | tasker.task_annotations |
| public.tasker_task_namespaces | tasker.task_namespaces |
| public.tasker_task_transitions | tasker.task_transitions |
| public.tasker_tasks | tasker.tasks |
| public.tasker_tasks_dlq | tasker.tasks_dlq |
| public.tasker_workflow_step_edges | tasker.workflow_step_edges |
| public.tasker_workflow_step_result_audit | tasker.workflow_step_result_audit |
| public.tasker_workflow_step_transitions | tasker.workflow_step_transitions |
| public.tasker_workflow_steps | tasker.workflow_steps |

**Note**: `_sqlx_migrations` table is SQLx infrastructure, intentionally excluded.

---

## Index Validation

**63 indexes matched** ✓

Index name transformation applied: `_tasker_` → `_`

Examples:
- `idx_tasker_tasks_state` → `idx_tasks_state`
- `idx_tasker_workflow_steps_task_id` → `idx_workflow_steps_task_id`

---

## Type Validation

| Old Type | New Type | Status |
|----------|----------|--------|
| public.dlq_reason | tasker.dlq_reason | ✓ Values identical |
| public.dlq_resolution_status | tasker.dlq_resolution_status | ✓ Values identical |

---

## UUID Compatibility

| Feature | Status |
|---------|--------|
| Old: pg_uuidv7 extension | ✓ Present in old schema |
| New: Native uuidv7() | ✓ Uses PostgreSQL 18 native function |
| Compatibility wrapper | ✓ `uuid_generate_v7()` in both `public` and `tasker` schemas |

---

## Recommendations

1. **PGMQ Function Duplication**: The PGMQ helper functions exist in both `public` and `tasker` schemas. Recommend:
   - If intentional for backward compatibility: Document the rationale
   - If unintentional: Clean up by keeping only in the appropriate schema

2. **Comment Changes**: Some functions had comments removed in the new schema (e.g., `get_next_ready_tasks`, `set_step_backoff_atomic`, `transition_task_state_atomic`). These were implementation notes - consider whether to preserve them for maintainability.

---

## Conclusion

The schema flattening transformation is **correct**. All SQL function logic has been preserved exactly, with only the expected namespace changes applied. The schema is ready for TAS-128 completion pending the PGMQ duplication decision.
