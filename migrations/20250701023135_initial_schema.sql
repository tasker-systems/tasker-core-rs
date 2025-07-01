-- Initial schema migration for Tasker Core Rust
-- Based on the Rails Tasker engine schema

-- Task Namespaces - Organizational hierarchy
CREATE TABLE tasker_task_namespaces (
    task_namespace_id SERIAL PRIMARY KEY,
    name VARCHAR(64) NOT NULL UNIQUE,
    description VARCHAR(255),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Named Tasks - Task templates/definitions
CREATE TABLE tasker_named_tasks (
    named_task_id SERIAL PRIMARY KEY,
    name VARCHAR(64) NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    description VARCHAR(255),
    task_namespace_id INTEGER NOT NULL REFERENCES tasker_task_namespaces(task_namespace_id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(name, version, task_namespace_id)
);

-- Named Steps - Step definitions
CREATE TABLE tasker_named_steps (
    named_step_id SERIAL PRIMARY KEY,
    name VARCHAR(64) NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    description VARCHAR(255),
    handler_class VARCHAR(255) NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(name, version)
);

-- Junction table for task-step relationships
CREATE TABLE tasker_named_tasks_named_steps (
    named_task_id INTEGER NOT NULL REFERENCES tasker_named_tasks(named_task_id),
    named_step_id INTEGER NOT NULL REFERENCES tasker_named_steps(named_step_id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (named_task_id, named_step_id)
);

-- Tasks - Actual task instances
CREATE TABLE tasker_tasks (
    task_id BIGSERIAL PRIMARY KEY,
    state VARCHAR(32) NOT NULL DEFAULT 'created',
    context JSONB NOT NULL DEFAULT '{}',
    most_recent_error_message TEXT,
    most_recent_error_backtrace TEXT,
    named_task_id INTEGER NOT NULL REFERENCES tasker_named_tasks(named_task_id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Workflow Steps - Individual step instances
CREATE TABLE tasker_workflow_steps (
    workflow_step_id BIGSERIAL PRIMARY KEY,
    state VARCHAR(32) NOT NULL DEFAULT 'created',
    context JSONB NOT NULL DEFAULT '{}',
    output JSONB NOT NULL DEFAULT '{}',
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    next_retry_at TIMESTAMPTZ,
    most_recent_error_message TEXT,
    most_recent_error_backtrace TEXT,
    task_id BIGINT NOT NULL REFERENCES tasker_tasks(task_id),
    named_step_id INTEGER NOT NULL REFERENCES tasker_named_steps(named_step_id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Workflow Step Edges - Dependency relationships (DAG)
CREATE TABLE tasker_workflow_step_edges (
    from_step_id BIGINT NOT NULL REFERENCES tasker_workflow_steps(workflow_step_id),
    to_step_id BIGINT NOT NULL REFERENCES tasker_workflow_steps(workflow_step_id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (from_step_id, to_step_id)
);

-- Task Transitions - Task state change audit trail
CREATE TABLE tasker_task_transitions (
    id BIGSERIAL PRIMARY KEY,
    to_state VARCHAR(32) NOT NULL,
    from_state VARCHAR(32),
    metadata JSONB NOT NULL DEFAULT '{}',
    sort_key INTEGER NOT NULL,
    most_recent BOOLEAN NOT NULL DEFAULT true,
    task_id BIGINT NOT NULL REFERENCES tasker_tasks(task_id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Workflow Step Transitions - Step state change audit trail
CREATE TABLE tasker_workflow_step_transitions (
    id BIGSERIAL PRIMARY KEY,
    to_state VARCHAR(32) NOT NULL,
    from_state VARCHAR(32),
    metadata JSONB NOT NULL DEFAULT '{}',
    sort_key INTEGER NOT NULL,
    most_recent BOOLEAN NOT NULL DEFAULT true,
    workflow_step_id BIGINT NOT NULL REFERENCES tasker_workflow_steps(workflow_step_id),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for performance
CREATE INDEX idx_tasker_tasks_state ON tasker_tasks(state);
CREATE INDEX idx_tasker_tasks_named_task_id ON tasker_tasks(named_task_id);
CREATE INDEX idx_tasker_tasks_created_at ON tasker_tasks(created_at);

CREATE INDEX idx_tasker_workflow_steps_state ON tasker_workflow_steps(state);
CREATE INDEX idx_tasker_workflow_steps_task_id ON tasker_workflow_steps(task_id);
CREATE INDEX idx_tasker_workflow_steps_named_step_id ON tasker_workflow_steps(named_step_id);
CREATE INDEX idx_tasker_workflow_steps_next_retry_at ON tasker_workflow_steps(next_retry_at) WHERE next_retry_at IS NOT NULL;

CREATE INDEX idx_tasker_task_transitions_task_id ON tasker_task_transitions(task_id);
CREATE INDEX idx_tasker_task_transitions_most_recent ON tasker_task_transitions(task_id, most_recent) WHERE most_recent = true;

CREATE INDEX idx_tasker_workflow_step_transitions_step_id ON tasker_workflow_step_transitions(workflow_step_id);
CREATE INDEX idx_tasker_workflow_step_transitions_most_recent ON tasker_workflow_step_transitions(workflow_step_id, most_recent) WHERE most_recent = true;

-- Function to update updated_at timestamp
CREATE OR REPLACE FUNCTION update_updated_at_column()
RETURNS TRIGGER AS $$
BEGIN
    NEW.updated_at = NOW();
    RETURN NEW;
END;
$$ language 'plpgsql';

-- Triggers to automatically update updated_at
CREATE TRIGGER update_tasker_task_namespaces_updated_at BEFORE UPDATE ON tasker_task_namespaces FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
CREATE TRIGGER update_tasker_named_tasks_updated_at BEFORE UPDATE ON tasker_named_tasks FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
CREATE TRIGGER update_tasker_named_steps_updated_at BEFORE UPDATE ON tasker_named_steps FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
CREATE TRIGGER update_tasker_tasks_updated_at BEFORE UPDATE ON tasker_tasks FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
CREATE TRIGGER update_tasker_workflow_steps_updated_at BEFORE UPDATE ON tasker_workflow_steps FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
CREATE TRIGGER update_tasker_task_transitions_updated_at BEFORE UPDATE ON tasker_task_transitions FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();
CREATE TRIGGER update_tasker_workflow_step_transitions_updated_at BEFORE UPDATE ON tasker_workflow_step_transitions FOR EACH ROW EXECUTE FUNCTION update_updated_at_column();