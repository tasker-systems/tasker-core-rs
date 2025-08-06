-- Create tasker_ready_tasks view for distributed orchestration
-- Phase 5.1: Replace imperative task readiness checking with declarative SQL view
--
-- This migration implements the architectural shift from batch-based to individual step-based
-- processing by creating a dedicated view for identifying tasks ready for orchestration.
--
-- Key Features:
-- 1. Uses existing SQL functions as reference for proven readiness logic
-- 2. Adds distributed safety with task claiming mechanism
-- 3. Optimized for orchestration polling performance
-- 4. Supports multiple orchestrator instances with stale claim recovery
--
-- ARCHITECTURAL NOTE:
-- This claiming mechanism ensures only ONE orchestrator processes a task's ready steps at a time.
-- Steps are still enqueued individually to pgmq for autonomous worker processing.
-- The claiming prevents duplicate step enqueueing while maintaining worker autonomy.

-- 1. Add task claiming columns to tasker_tasks table for distributed coordination
ALTER TABLE public.tasker_tasks 
ADD COLUMN IF NOT EXISTS claimed_at timestamp(6) without time zone,
ADD COLUMN IF NOT EXISTS claimed_by character varying,
ADD COLUMN IF NOT EXISTS priority integer DEFAULT 0 NOT NULL,
ADD COLUMN IF NOT EXISTS claim_timeout_seconds integer DEFAULT 60 NOT NULL;

-- 2. Create index for efficient claiming queries
CREATE INDEX IF NOT EXISTS idx_tasker_tasks_claiming 
ON public.tasker_tasks(claimed_at, claimed_by) 
WHERE claimed_at IS NOT NULL;

-- 3. Create index for unclaimed task queries (most common case)
CREATE INDEX IF NOT EXISTS idx_tasker_tasks_unclaimed 
ON public.tasker_tasks(complete, priority, created_at) 
WHERE complete = false AND claimed_at IS NULL;

-- 4. Create the tasker_ready_tasks view with priority fairness
-- This view combines task state with step readiness using existing SQL functions
-- PRIORITY FAIRNESS: Prevents task starvation with time-weighted priority calculation
CREATE OR REPLACE VIEW public.tasker_ready_tasks AS
SELECT 
    t.task_id,
    tn.name as namespace_name,
    t.priority,
    t.created_at,
    t.updated_at,
    t.claimed_at,
    t.claimed_by,
    -- Use existing get_task_execution_context() function for proven readiness logic
    tec.ready_steps as ready_steps_count,
    tec.execution_status,
    tec.total_steps,
    tec.completed_steps,
    tec.pending_steps,
    tec.failed_steps,
    tec.in_progress_steps,
    -- Calculate task age in hours for monitoring and debugging
    ROUND(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 3600.0, 2)::float8 as age_hours,
    -- Computed priority with time-weighted escalation to prevent starvation
    -- Aligned with Rust TaskPriority enum: Low=1, Normal=2, High=3, Urgent=4
    -- High-throughput timeframes: tasks should process in seconds, escalation in minutes
    (CASE 
        WHEN t.priority >= 4 THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 300, 2)      -- Urgent (4): +1 per 5min, max +2 (final: 6)
        WHEN t.priority = 3  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 180, 3)      -- High (3): +1 per 3min, max +3 (final: 6)
        WHEN t.priority = 2  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 120, 4)      -- Normal (2): +1 per 2min, max +4 (final: 6)
        WHEN t.priority = 1  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 60, 5)       -- Low (1): +1 per 1min, max +5 (final: 6)
        ELSE                      0 + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 30, 6)                -- Zero/Invalid (0): +1 per 30sec, max +6 (final: 6)
    END)::float8 as computed_priority,
    -- Calculate claim status for distributed coordination using configurable timeout
    CASE 
        WHEN t.claimed_at IS NULL THEN 'available'
        WHEN t.claimed_at < (NOW() - (t.claim_timeout_seconds || ' seconds')::interval) THEN 'stale_claim' 
        ELSE 'claimed'
    END as claim_status,
    -- Calculate how long task has been claimed (for monitoring)
    CASE 
        WHEN t.claimed_at IS NOT NULL 
        THEN EXTRACT(EPOCH FROM (NOW() - t.claimed_at))::integer
        ELSE NULL 
    END as claimed_duration_seconds
FROM public.tasker_tasks t
-- Join with named_tasks to get task metadata
INNER JOIN public.tasker_named_tasks nt ON t.named_task_id = nt.named_task_id
-- Join with task_namespaces to get namespace name
INNER JOIN public.tasker_task_namespaces tn ON nt.task_namespace_id = tn.task_namespace_id
-- Use LATERAL JOIN with get_task_execution_context() for efficiency
JOIN LATERAL (
    SELECT * FROM get_task_execution_context(t.task_id)
) tec ON true
WHERE 
    -- Only include incomplete tasks
    t.complete = false 
    -- Only include tasks with ready steps (from existing SQL function logic)
    AND tec.ready_steps > 0 
    AND tec.execution_status IN ('in_progress', 'pending', 'has_ready_steps')
    -- Include unclaimed tasks OR stale claims (configurable timeout per task)
    AND (t.claimed_at IS NULL OR t.claimed_at < (NOW() - (t.claim_timeout_seconds || ' seconds')::interval))
ORDER BY 
    -- Use computed_priority to ensure fairness with time-weighted escalation
    -- This prevents starvation while respecting base priority levels
    -- Aligned with Rust TaskPriority enum: Low=1, Normal=2, High=3, Urgent=4
    -- High-throughput timeframes: tasks should process in seconds, escalation in minutes
    (CASE 
        WHEN t.priority >= 4 THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 300, 2)      -- Urgent (4): +1 per 5min, max +2 (final: 6)
        WHEN t.priority = 3  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 180, 3)      -- High (3): +1 per 3min, max +3 (final: 6)
        WHEN t.priority = 2  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 120, 4)      -- Normal (2): +1 per 2min, max +4 (final: 6)
        WHEN t.priority = 1  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 60, 5)       -- Low (1): +1 per 1min, max +5 (final: 6)
        ELSE                      0 + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 30, 6)                -- Zero/Invalid (0): +1 per 30sec, max +6 (final: 6)
    END)::float8 DESC,
    -- Break ties with creation order (oldest first)
    t.created_at ASC;

-- Performance indexes for the view
-- Index for efficient priority-based ordering
CREATE INDEX IF NOT EXISTS idx_tasker_tasks_priority_created 
ON public.tasker_tasks(priority DESC, created_at ASC) 
WHERE complete = false;

-- Index for namespace-based filtering (when orchestrators are namespace-specific)
CREATE INDEX IF NOT EXISTS idx_tasker_tasks_named_task_created 
ON public.tasker_tasks(named_task_id, created_at ASC) 
WHERE complete = false;

-- 5. Create function for atomic task claiming with priority fairness
-- This function encapsulates the complex claiming logic to prevent race conditions
-- PRIORITY FAIRNESS: Uses computed priority ordering to prevent task starvation
CREATE OR REPLACE FUNCTION public.claim_ready_tasks(
    p_orchestrator_id character varying,
    p_limit integer DEFAULT 1,
    p_namespace_filter character varying DEFAULT NULL
) 
RETURNS TABLE(
    task_id bigint,
    namespace_name character varying,
    priority integer,
    computed_priority float8,
    age_hours float8,
    ready_steps_count bigint,
    claim_timeout_seconds integer
)
LANGUAGE plpgsql
AS $$
BEGIN
    -- Atomically claim tasks using proper locking on base table with computed priority ordering
    RETURN QUERY
    UPDATE tasker_tasks t
    SET claimed_at = NOW(), 
        claimed_by = p_orchestrator_id,
        updated_at = NOW()
    FROM (
        SELECT 
            t.task_id,
            -- Include computed priority and age for debugging and monitoring
            -- Aligned with Rust TaskPriority enum: Low=1, Normal=2, High=3, Urgent=4
            -- High-throughput timeframes: tasks should process in seconds, escalation in minutes
            (CASE 
                WHEN t.priority >= 4 THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 300, 2)
                WHEN t.priority = 3  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 180, 3)
                WHEN t.priority = 2  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 120, 4)
                WHEN t.priority = 1  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 60, 5)
                ELSE                      0 + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 30, 6)
            END)::float8 as computed_priority_calc,
            ROUND(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 3600.0, 2)::float8 as age_hours_calc
        FROM tasker_tasks t
        JOIN tasker_named_tasks nt ON t.named_task_id = nt.named_task_id
        JOIN tasker_task_namespaces tn ON nt.task_namespace_id = tn.task_namespace_id
        JOIN LATERAL (SELECT * FROM get_task_execution_context(t.task_id)) tec ON true
        WHERE t.complete = false 
            AND tec.ready_steps > 0 
            AND (t.claimed_at IS NULL OR t.claimed_at < (NOW() - (t.claim_timeout_seconds || ' seconds')::interval))
            AND (p_namespace_filter IS NULL OR tn.name = p_namespace_filter)
        ORDER BY 
            -- Use computed priority for fair ordering
            -- Aligned with Rust TaskPriority enum: Low=1, Normal=2, High=3, Urgent=4
            -- High-throughput timeframes: tasks should process in seconds, escalation in minutes
            (CASE 
                WHEN t.priority >= 4 THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 300, 2)
                WHEN t.priority = 3  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 180, 3)
                WHEN t.priority = 2  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 120, 4)
                WHEN t.priority = 1  THEN t.priority + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 60, 5)
                ELSE                      0 + LEAST(EXTRACT(EPOCH FROM (NOW() - t.created_at)) / 30, 6)
            END)::float8 DESC,
            t.created_at ASC
        LIMIT p_limit
        FOR UPDATE OF t SKIP LOCKED
    ) ready_tasks
    WHERE t.task_id = ready_tasks.task_id
    RETURNING 
        t.task_id,
        (SELECT name FROM tasker_task_namespaces WHERE task_namespace_id = 
            (SELECT task_namespace_id FROM tasker_named_tasks WHERE named_task_id = t.named_task_id)
        ) as namespace_name,
        t.priority,
        ready_tasks.computed_priority_calc::float8 as computed_priority,
        ready_tasks.age_hours_calc::float8 as age_hours,
        (SELECT ready_steps FROM get_task_execution_context(t.task_id)) as ready_steps_count,
        t.claim_timeout_seconds;
END;
$$;

-- 6. Create function to release task claims
-- Use this when done processing or on error to make task available again
CREATE OR REPLACE FUNCTION public.release_task_claim(
    p_task_id bigint,
    p_orchestrator_id character varying
)
RETURNS boolean
LANGUAGE plpgsql
AS $$
DECLARE
    v_rows_updated integer;
BEGIN
    UPDATE tasker_tasks
    SET claimed_at = NULL,
        claimed_by = NULL,
        updated_at = NOW()
    WHERE task_id = p_task_id
        AND claimed_by = p_orchestrator_id;  -- Only release if we own the claim
    
    GET DIAGNOSTICS v_rows_updated = ROW_COUNT;
    
    -- Return true if claim was released, false if not found or not owned
    RETURN v_rows_updated > 0;
END;
$$;

-- 7. Create function to extend task claim (heartbeat)
-- Call this periodically while processing to prevent stale claim timeout
CREATE OR REPLACE FUNCTION public.extend_task_claim(
    p_task_id bigint,
    p_orchestrator_id character varying
)
RETURNS boolean
LANGUAGE plpgsql
AS $$
DECLARE
    v_rows_updated integer;
BEGIN
    UPDATE tasker_tasks
    SET claimed_at = NOW(),  -- Reset claim time
        updated_at = NOW()
    WHERE task_id = p_task_id
        AND claimed_by = p_orchestrator_id  -- Only extend if we own the claim
        AND claimed_at IS NOT NULL;         -- And claim exists
    
    GET DIAGNOSTICS v_rows_updated = ROW_COUNT;
    
    -- Return true if claim was extended, false if not found or not owned
    RETURN v_rows_updated > 0;
END;
$$;

-- Comments for documentation
COMMENT ON VIEW public.tasker_ready_tasks IS 'Declarative view for distributed orchestration with priority fairness - prevents task starvation using time-weighted priority calculation';

COMMENT ON FUNCTION public.claim_ready_tasks IS 'Atomically claim tasks with priority fairness ordering. Includes computed_priority and age_hours for monitoring starvation prevention.';
COMMENT ON FUNCTION public.release_task_claim IS 'Release a task claim when done processing or on error. Only releases if caller owns the claim.';
COMMENT ON FUNCTION public.extend_task_claim IS 'Extend claim timeout while processing (heartbeat). Prevents stale claim detection during long operations.';

COMMENT ON COLUMN public.tasker_tasks.claimed_at IS 'Timestamp when task was claimed by an orchestrator instance for distributed coordination';
COMMENT ON COLUMN public.tasker_tasks.claimed_by IS 'Identifier of orchestrator instance that claimed this task (hostname + uuid)';
COMMENT ON COLUMN public.tasker_tasks.priority IS 'Base task execution priority (1=Low, 2=Normal, 3=High, 4=Urgent, 0=Invalid). Combined with task age to compute fair execution order and prevent starvation.';
COMMENT ON COLUMN public.tasker_tasks.claim_timeout_seconds IS 'Configurable timeout for task claims (default 60s). Prevents indefinite blocking from crashed orchestrators.';

-- Usage examples and notes
/*
Priority Fairness Examples:

1. UNDERSTANDING THE ESCALATION RATES:
   
   Aligned with Rust TaskPriority enum: Low=1, Normal=2, High=3, Urgent=4
   High-throughput system: tasks should process in seconds, escalation in minutes
   
   Urgent Priority Tasks (4):
   - Start with urgent base priority (4)
   - Escalate slowly: +1 per 5 minutes, max +2 boost
   - Example: Priority 4 task after 10 minutes = 4 + 2 = 6 effective priority
   
   High Priority Tasks (3):
   - Start with high base priority (3)
   - Escalate moderately: +1 per 3 minutes, max +3 boost
   - Example: Priority 3 task after 9 minutes = 3 + 3 = 6 effective priority
   
   Normal Priority Tasks (2):
   - Start with normal base priority (2)
   - Escalate faster: +1 per 2 minutes, max +4 boost
   - Example: Priority 2 task after 8 minutes = 2 + 4 = 6 effective priority
   
   Low Priority Tasks (1):
   - Start with low base priority (1)
   - Escalate quickly: +1 per 1 minute, max +5 boost
   - Example: Priority 1 task after 5 minutes = 1 + 5 = 6 effective priority
   
   Zero/Invalid Priority Tasks (0):
   - Start with zero base priority (invalid/legacy)
   - Escalate fastest: +1 per 30 seconds, max +6 boost
   - Example: Priority 0 task after 3 minutes = 0 + 6 = 6 effective priority

2. STARVATION PREVENTION DEMONSTRATION:
   
   Scenario: Urgent tasks (priority 4) keep arriving continuously  
   - New priority 4 (Urgent) task: computed_priority = 4 + 0 = 4
   - Priority 1 (Low) task waiting 5 minutes: computed_priority = 1 + 5 = 6
   - Result: The waiting priority 1 task gets processed first!
   
   All priorities converge to maximum 6 within minutes, ensuring fairness in high-throughput systems.

3. MONITORING STARVATION PREVENTION:
   
   -- See all tasks with their computed priorities
   SELECT task_id, priority, age_hours, computed_priority, namespace_name
   FROM tasker_ready_tasks 
   WHERE claim_status = 'available'
   ORDER BY computed_priority DESC;
   
   -- Find tasks that have been escalated beyond their base priority
   SELECT task_id, priority, computed_priority, 
          (computed_priority - priority) as priority_boost,
          age_hours
   FROM tasker_ready_tasks 
   WHERE computed_priority > priority;
   
   -- Monitor for potential starvation (tasks waiting too long)
   SELECT task_id, priority, age_hours, computed_priority
   FROM tasker_ready_tasks 
   WHERE age_hours > 1  -- Tasks waiting more than 1 hour
   ORDER BY age_hours DESC;

4. CLAIMING WITH PRIORITY FAIRNESS:
   
   -- Claim tasks with fairness ordering (includes computed_priority for monitoring)
   SELECT task_id, namespace_name, priority, computed_priority, age_hours, ready_steps_count
   FROM claim_ready_tasks('orchestrator-host123-uuid', 5);
   
   -- Claim from specific namespace with fairness
   SELECT task_id, priority, computed_priority, age_hours
   FROM claim_ready_tasks('orchestrator-host123-uuid', 3, 'fulfillment');

5. RELEASE AND EXTEND CLAIMS:

   -- Release a task claim when done or on error
   SELECT release_task_claim(12345, 'orchestrator-host123-uuid');

   -- Extend claim while processing (heartbeat)
   SELECT extend_task_claim(12345, 'orchestrator-host123-uuid');

6. TYPICAL ORCHESTRATOR WORKFLOW WITH PRIORITY FAIRNESS:
   
   -- Step 1: Claim tasks with fairness
   WITH claimed AS (
     SELECT * FROM claim_ready_tasks('my-orchestrator-id', 10)
   )
   -- Step 2: Process each claimed task and monitor priority distribution
   SELECT task_id, namespace_name, priority, computed_priority, 
          CASE WHEN computed_priority > priority 
               THEN 'escalated' 
               ELSE 'base_priority' END as priority_status,
          ready_steps_count 
   FROM claimed;
   
   -- Step 3: For each task, discover and enqueue ready steps
   -- Step 4: Release claim when done
   SELECT release_task_claim(task_id, 'my-orchestrator-id');

Performance Impact:
- Computed priority calculation adds minimal overhead (simple arithmetic)
- Same indexes used for ordering (priority + created_at)
- LATERAL JOIN efficiency preserved from original implementation
- Additional columns (age_hours, computed_priority) available for monitoring without performance cost

Architecture Benefits:
- No additional queues needed (vs. namespace_priority_queue approach)
- Preserves existing orchestration patterns
- Maintains backward compatibility with existing priority values
- Provides rich monitoring data for queue health analysis
- Ensures all tasks eventually get processed regardless of base priority
*/