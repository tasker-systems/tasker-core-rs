# Summary of Changes to Fix Hardcoded Test Names

## Overview
Fixed all hardcoded test names in test files to use unique timestamp-based names, preventing unique constraint violations during test runs.

## Pattern Used
Replace hardcoded names like:
- `name: "test_namespace".to_string()` 
- `name: "test_task".to_string()`
- `name: "test_system".to_string()`
- `name: "test_step".to_string()`

With timestamp-based unique names:
- `name: format!("test_namespace_{}", chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0))`

## Files Modified

### Model Test Files
1. **src/models/task_diagram.rs**
   - Fixed hardcoded test_namespace and test_task names
   - Fixed null context issue in Task creation

2. **src/models/task_execution_context.rs**
   - Fixed hardcoded test_namespace and test_task names
   - Fixed null context issue in Task creation

3. **src/models/step_readiness_status.rs**
   - Fixed hardcoded test_namespace, test_task, and test_step names
   - Fixed null context issue in Task creation

4. **src/models/named_task.rs**
   - Fixed hardcoded test_task name
   - Added namespace creation instead of using hardcoded namespace ID
   - Updated assertions to use dynamic names
   - Added namespace cleanup in test teardown

5. **src/models/dependent_system_object_map.rs**
   - Fixed hardcoded test_system_one and test_system_two names

6. **src/models/dependent_system.rs**
   - Fixed hardcoded test_system name
   - Updated find_or_create_by_name calls to use dynamic names
   - Updated assertions to check for prefix instead of exact match

7. **src/models/named_step.rs**
   - Fixed hardcoded test_step name
   - Updated find_by_name and is_name_unique calls to use dynamic names

8. **src/models/named_tasks_named_step.rs**
   - Fixed hardcoded test_step and test_system names in unit test
   - Updated assertions to use dynamic names

9. **src/models/task_annotation.rs**
   - Fixed hardcoded test_task and test_annotation names
   - Fixed null context issue in Task creation
   - Updated assertion to check for prefix

10. **src/models/task_transition.rs**
    - Fixed hardcoded test_task name

11. **src/models/task.rs**
    - Fixed hardcoded test_task and test_system names

12. **src/models/workflow_step_transition.rs**
    - Fixed hardcoded test_task, test_step, and test_system names
    - Updated find_or_create_by_name call to use dynamic name

### Test Helper Files
1. **tests/common/builders.rs**
   - Fixed hardcoded test_system in NamedStepBuilder (2 occurrences)
   - Now uses unique_name() function consistently

2. **tests/test_helpers/mod.rs**
   - Fixed hardcoded test_seed_namespace, test_seed_system, and test_seed_annotation names
   - Now uses unique_name() function for all seed data

## Additional Fixes
- Fixed null context issues where tests were creating Tasks with `context: None` but the database requires NOT NULL
- Changed to `context: Some(serde_json::json!({}))` for all Task creations
- Fixed foreign key constraint issues by creating required parent entities (namespaces) before dependent entities

## Results
- Eliminated all unique constraint violations in tests
- Tests can now run multiple times without conflicts
- Each test run uses unique identifiers preventing database conflicts