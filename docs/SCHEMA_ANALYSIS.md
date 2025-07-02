# Tasker Schema Analysis: SQL vs Rust Models

## Overview

This document provides a comprehensive comparison between the tasker-related tables in the PostgreSQL database schema and the current Rust model implementations.

## Table-by-Table Analysis

### 1. tasker_tasks

**SQL Schema:**
```sql
CREATE TABLE public.tasker_tasks (
    task_id bigint NOT NULL,
    named_task_id integer NOT NULL,
    complete boolean DEFAULT false NOT NULL,
    requested_at timestamp without time zone NOT NULL,
    initiator character varying(128),
    source_system character varying(128),
    reason character varying(128),
    bypass_steps json,
    tags jsonb,
    context jsonb,
    identity_hash character varying(128) NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);
```

**Rust Model (Task):**
```rust
pub struct Task {
    pub task_id: i64,
    pub state: String,  // ⚠️ NOT IN SQL - SQL uses 'complete' boolean
    pub context: serde_json::Value,
    pub most_recent_error_message: Option<String>,  // ⚠️ NOT IN SQL
    pub most_recent_error_backtrace: Option<String>,  // ⚠️ NOT IN SQL
    pub named_task_id: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

**Missing in Rust Model:**
- `complete: bool`
- `requested_at: DateTime<Utc>`
- `initiator: Option<String>`
- `source_system: Option<String>`
- `reason: Option<String>`
- `bypass_steps: Option<serde_json::Value>`
- `tags: Option<serde_json::Value>`
- `identity_hash: String`

**Present in Rust but not SQL:**
- `state: String` (likely computed from transitions)
- `most_recent_error_message: Option<String>`
- `most_recent_error_backtrace: Option<String>`

### 2. tasker_workflow_steps

**SQL Schema:**
```sql
CREATE TABLE public.tasker_workflow_steps (
    workflow_step_id bigint NOT NULL,
    task_id bigint NOT NULL,
    named_step_id integer NOT NULL,
    retryable boolean DEFAULT true NOT NULL,
    retry_limit integer DEFAULT 3,
    in_process boolean DEFAULT false NOT NULL,
    processed boolean DEFAULT false NOT NULL,
    processed_at timestamp without time zone,
    attempts integer,
    last_attempted_at timestamp without time zone,
    backoff_request_seconds integer,
    inputs jsonb,
    results jsonb,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL,
    skippable boolean DEFAULT false NOT NULL
);
```

**Rust Model (WorkflowStep):**
```rust
pub struct WorkflowStep {
    pub workflow_step_id: i64,
    pub state: String,  // ⚠️ NOT IN SQL - likely derived from in_process/processed
    pub context: serde_json::Value,  // ⚠️ SQL has 'inputs' instead
    pub output: serde_json::Value,  // ⚠️ SQL has 'results' instead
    pub retry_count: i32,  // ⚠️ SQL has 'attempts' instead
    pub max_retries: i32,  // ⚠️ SQL has 'retry_limit' instead
    pub next_retry_at: Option<DateTime<Utc>>,  // ⚠️ NOT IN SQL
    pub most_recent_error_message: Option<String>,  // ⚠️ NOT IN SQL
    pub most_recent_error_backtrace: Option<String>,  // ⚠️ NOT IN SQL
    pub task_id: i64,
    pub named_step_id: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

**Missing in Rust Model:**
- `retryable: bool`
- `in_process: bool`
- `processed: bool`
- `processed_at: Option<DateTime<Utc>>`
- `last_attempted_at: Option<DateTime<Utc>>`
- `backoff_request_seconds: Option<i32>`
- `skippable: bool`

**Naming Differences:**
- Rust uses `context` vs SQL `inputs`
- Rust uses `output` vs SQL `results`
- Rust uses `retry_count` vs SQL `attempts`
- Rust uses `max_retries` vs SQL `retry_limit`

### 3. tasker_workflow_step_edges

**SQL Schema:**
```sql
CREATE TABLE public.tasker_workflow_step_edges (
    id bigint NOT NULL,  // ⚠️ Has primary key 'id'
    from_step_id bigint NOT NULL,
    to_step_id bigint NOT NULL,
    name character varying NOT NULL,  // ⚠️ Has 'name' field
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);
```

**Rust Model (WorkflowStepEdge):**
```rust
pub struct WorkflowStepEdge {
    // ⚠️ Missing 'id' field
    pub from_step_id: i64,
    pub to_step_id: i64,
    // ⚠️ Missing 'name' field
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

**Missing in Rust Model:**
- `id: i64` (primary key)
- `name: String` (edge name/label)

### 4. tasker_named_tasks

**SQL Schema:**
```sql
CREATE TABLE public.tasker_named_tasks (
    named_task_id integer NOT NULL,
    name character varying(64) NOT NULL,
    description character varying(255),
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL,
    task_namespace_id bigint DEFAULT 1 NOT NULL,
    version character varying(16) DEFAULT '0.1.0'::character varying NOT NULL,  // ⚠️ String type
    configuration jsonb DEFAULT '"{}"'::jsonb  // ⚠️ Has configuration field
);
```

**Rust Model (NamedTask):**
```rust
pub struct NamedTask {
    pub named_task_id: i32,
    pub name: String,
    pub version: i32,  // ⚠️ Integer vs SQL string(16)
    pub description: Option<String>,
    pub task_namespace_id: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

**Missing in Rust Model:**
- `configuration: Option<serde_json::Value>`

**Type Differences:**
- `version` is `i32` in Rust but `character varying(16)` in SQL (e.g., "0.1.0")

### 5. tasker_named_steps

**SQL Schema:**
```sql
CREATE TABLE public.tasker_named_steps (
    named_step_id integer NOT NULL,
    dependent_system_id integer NOT NULL,  // ⚠️ References dependent_systems
    name character varying(128) NOT NULL,
    description character varying(255),
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);
```

**Rust Model (NamedStep):**
```rust
pub struct NamedStep {
    pub named_step_id: i32,
    pub name: String,
    pub version: i32,  // ⚠️ NOT IN SQL
    pub description: Option<String>,
    pub handler_class: String,  // ⚠️ NOT IN SQL
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
```

**Missing in Rust Model:**
- `dependent_system_id: i32`

**Present in Rust but not SQL:**
- `version: i32`
- `handler_class: String`

### 6. tasker_named_tasks_named_steps (Join Table)

**SQL Schema:**
```sql
CREATE TABLE public.tasker_named_tasks_named_steps (
    id integer NOT NULL,  // ⚠️ Has primary key
    named_task_id integer NOT NULL,
    named_step_id integer NOT NULL,
    skippable boolean DEFAULT false NOT NULL,
    default_retryable boolean DEFAULT true NOT NULL,
    default_retry_limit integer DEFAULT 3 NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);
```

**Rust Model:** ⚠️ **NO DEDICATED MODEL** - Currently handled through methods in NamedTask

**Missing Entire Model for:**
- Join table with configuration for task-step relationships
- Per-task step configuration (skippable, retryable, retry_limit)

### 7. tasker_task_transitions

**SQL Schema & Rust Model:** ✅ **MATCH CORRECTLY**

The TaskTransition model correctly implements all fields from the SQL schema.

### 8. tasker_workflow_step_transitions

**SQL Schema & Rust Model:** ✅ **MATCH CORRECTLY**

The WorkflowStepTransition model correctly implements all fields from the SQL schema.

### 9. tasker_task_namespaces

**SQL Schema:**
```sql
CREATE TABLE public.tasker_task_namespaces (
    task_namespace_id integer NOT NULL,
    name character varying(64) NOT NULL,
    description character varying(255),
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);
```

**Rust Model (TaskNamespace):** ✅ **MATCH CORRECTLY**

### 10. Missing Tables (No Rust Models)

The following tables exist in SQL but have no corresponding Rust models:

**tasker_annotation_types:**
```sql
CREATE TABLE public.tasker_annotation_types (
    annotation_type_id integer NOT NULL,
    name character varying(64) NOT NULL,
    description character varying(255),
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);
```

**tasker_task_annotations:**
```sql
CREATE TABLE public.tasker_task_annotations (
    task_annotation_id bigint NOT NULL,
    task_id bigint NOT NULL,
    annotation_type_id integer NOT NULL,
    annotation jsonb,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);
```

**tasker_dependent_systems:**
```sql
CREATE TABLE public.tasker_dependent_systems (
    dependent_system_id integer NOT NULL,
    name character varying(64) NOT NULL,
    description character varying(255),
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);
```

**tasker_dependent_system_object_maps:**
```sql
CREATE TABLE public.tasker_dependent_system_object_maps (
    dependent_system_object_map_id bigint NOT NULL,
    dependent_system_one_id integer NOT NULL,
    dependent_system_two_id integer NOT NULL,
    remote_id_one character varying(128) NOT NULL,
    remote_id_two character varying(128) NOT NULL,
    created_at timestamp(6) without time zone NOT NULL,
    updated_at timestamp(6) without time zone NOT NULL
);
```

## Summary of Critical Issues

### 1. Major Field Mismatches
- **Task model** is missing many fields and has fields not in SQL
- **WorkflowStep model** has significant naming differences and missing fields
- **WorkflowStepEdge model** is missing id and name fields
- **NamedTask model** has version as integer instead of string
- **NamedStep model** is missing dependent_system_id and has extra fields

### 2. Missing Models
- No model for `tasker_named_tasks_named_steps` join table
- No models for annotation system (annotation_types, task_annotations)
- No models for dependent system tracking

### 3. Type Mismatches
- Version fields using different types (integer vs string)
- State representation differs between SQL and Rust

### 4. Naming Inconsistencies
- Different field names for same concepts (e.g., inputs/context, results/output)
- Some Rust fields don't exist in SQL (likely computed fields)

## Recommendations

1. **Align field names and types** with the SQL schema
2. **Add missing fields** to match the complete SQL schema
3. **Create missing models** for join tables and annotation system
4. **Consider computed fields** - clearly separate stored vs computed fields
5. **Version field type** - decide on string vs integer and be consistent
6. **State management** - clarify how state is stored vs computed