-- =============================================================================
-- TAS-154: Add Identity Strategy to Named Tasks
-- =============================================================================
-- This migration adds support for configurable task identity strategies.
--
-- Identity strategies control how task deduplication works:
--   - strict (default): hash(named_task_uuid, context) - full idempotency
--   - caller_provided: caller provides idempotency_key
--   - always_unique: uuidv7() - no deduplication
--
-- See docs/ticket-specs/TAS-154/identity-hash-strategy.md for full specification.
-- =============================================================================

-- Set search_path for this session
SET search_path TO tasker, public;

-- =============================================================================
-- ENUM TYPE
-- =============================================================================

-- Create the identity strategy enum type
CREATE TYPE tasker.identity_strategy AS ENUM (
    'strict',
    'caller_provided',
    'always_unique'
);

COMMENT ON TYPE tasker.identity_strategy IS 'TAS-154: Task identity strategy for configurable deduplication behavior.
- strict: hash(named_task_uuid, context) - full idempotency (default)
- caller_provided: caller must provide idempotency_key
- always_unique: uuidv7() - no deduplication';

-- =============================================================================
-- COLUMN ADDITION
-- =============================================================================

-- Add identity_strategy column to named_tasks
-- Default is 'strict' to maintain backward compatibility with existing behavior
ALTER TABLE tasker.named_tasks
    ADD COLUMN identity_strategy tasker.identity_strategy NOT NULL DEFAULT 'strict';

COMMENT ON COLUMN tasker.named_tasks.identity_strategy IS 'TAS-154: Determines how task identity is computed for deduplication.
- strict: Same context = same task (full idempotency)
- caller_provided: Caller must provide idempotency_key
- always_unique: Every request creates new task';

-- =============================================================================
-- INDEX FOR FILTERING BY STRATEGY
-- =============================================================================

-- Index for queries filtering by identity strategy (e.g., finding all ALWAYS_UNIQUE tasks)
CREATE INDEX idx_named_tasks_identity_strategy ON tasker.named_tasks USING btree (identity_strategy);

COMMENT ON INDEX tasker.idx_named_tasks_identity_strategy IS 'TAS-154: Supports filtering named tasks by identity strategy.';
