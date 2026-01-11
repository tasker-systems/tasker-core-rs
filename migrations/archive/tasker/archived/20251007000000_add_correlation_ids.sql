-- ============================================================================
-- TAS-29 Phase 1: Add Correlation ID Support
-- ============================================================================
-- This migration adds correlation ID tracking to support distributed tracing
-- and observability throughout the workflow lifecycle.
--
-- Background:
-- Correlation IDs enable end-to-end request tracing across the entire system:
-- - Track a task's lifecycle from creation through completion
-- - Link parent/child workflow relationships
-- - Enable distributed tracing with OpenTelemetry
-- - Provide cross-cutting observability in logs, metrics, and traces
--
-- Design Decisions:
-- 1. correlation_id is NOT NULL - every task must have a correlation ID
--    - Simplifies all downstream logic (no Option<Uuid> handling)
--    - Auto-generated at insertion if not provided
--    - Enforces observability by default
--
-- 2. Using UUID v7 (time-ordered) for correlation IDs provides:
--    - Chronological ordering for better query performance
--    - Distributed generation without coordination
--    - Compatibility with existing UUID infrastructure
--    - Matches task_uuid ordering (both are UUIDv7)
--
-- 3. Backfill Strategy:
--    - Uses uuid_generate_v7() for existing tasks (current timestamp)
--    - For backfilled tasks, correlation_id won't match task creation time
--    - New tasks will have correlation_id generated at creation time
-- ============================================================================

-- ============================================================================
-- Add correlation_id and parent_correlation_id columns
-- ============================================================================

-- Add columns as nullable first
ALTER TABLE public.tasker_tasks
ADD COLUMN IF NOT EXISTS correlation_id UUID,
ADD COLUMN IF NOT EXISTS parent_correlation_id UUID;

-- Generate correlation_id for any existing tasks
-- Uses current timestamp since uuid_generate_v7() doesn't accept parameters
UPDATE public.tasker_tasks
SET correlation_id = uuid_generate_v7()
WHERE correlation_id IS NULL;

-- Now make correlation_id NOT NULL (safe because we just filled all NULLs)
ALTER TABLE public.tasker_tasks
ALTER COLUMN correlation_id SET NOT NULL;

-- ============================================================================
-- Add indexes for correlation ID queries
-- ============================================================================

-- Index for correlation_id lookups (primary tracing queries)
CREATE INDEX IF NOT EXISTS idx_tasker_tasks_correlation_id
ON public.tasker_tasks(correlation_id);

-- Index for parent_correlation_id lookups (workflow hierarchy queries)
CREATE INDEX IF NOT EXISTS idx_tasker_tasks_parent_correlation_id
ON public.tasker_tasks(parent_correlation_id)
WHERE parent_correlation_id IS NOT NULL;

-- Composite index for efficient hierarchy traversal
CREATE INDEX IF NOT EXISTS idx_tasker_tasks_correlation_hierarchy
ON public.tasker_tasks(parent_correlation_id, correlation_id)
WHERE parent_correlation_id IS NOT NULL;

-- ============================================================================
-- Comments for documentation
-- ============================================================================

COMMENT ON COLUMN public.tasker_tasks.correlation_id IS
'UUID v7 correlation ID for distributed tracing (NOT NULL). Auto-generated if not provided at task creation. Enables end-to-end request tracking across orchestration and workers.';

COMMENT ON COLUMN public.tasker_tasks.parent_correlation_id IS
'Optional correlation ID of parent workflow. Enables tracking of nested/chained workflow relationships.';

-- ============================================================================
-- Rollback instructions
-- ============================================================================
-- To rollback:
-- DROP INDEX IF EXISTS idx_tasker_tasks_correlation_hierarchy;
-- DROP INDEX IF EXISTS idx_tasker_tasks_parent_correlation_id;
-- DROP INDEX IF EXISTS idx_tasker_tasks_correlation_id;
-- ALTER TABLE public.tasker_tasks DROP COLUMN IF EXISTS parent_correlation_id;
-- ALTER TABLE public.tasker_tasks DROP COLUMN IF EXISTS correlation_id;
