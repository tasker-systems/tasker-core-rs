-- Create Optimized Worker Query Functions
-- Migration to create SQL functions for high-performance worker selection queries
-- with pre-computed query plans and optimized indexes

-- Create recommended indexes for optimal performance
-- Note: CONCURRENTLY removed since migrations run inside transactions
CREATE INDEX IF NOT EXISTS idx_worker_registrations_active 
ON tasker_worker_registrations (worker_id, status, unregistered_at, last_heartbeat_at) 
WHERE unregistered_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_worker_named_tasks_task_priority 
ON tasker_worker_named_tasks (named_task_id, priority DESC, worker_id);

CREATE INDEX IF NOT EXISTS idx_worker_registrations_heartbeat 
ON tasker_worker_registrations (last_heartbeat_at DESC, status) 
WHERE unregistered_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_workers_metadata_gin 
ON tasker_workers USING GIN (metadata);

-- Function to find active workers for a specific task with optimized query plan
CREATE OR REPLACE FUNCTION find_active_workers_for_task(p_named_task_id INTEGER)
RETURNS TABLE (
    id INTEGER,
    worker_id INTEGER,
    named_task_id INTEGER,
    configuration JSONB,
    priority INTEGER,
    created_at TIMESTAMP(6) WITHOUT TIME ZONE,
    updated_at TIMESTAMP(6) WITHOUT TIME ZONE,
    worker_name CHARACTER VARYING(255),
    task_name CHARACTER VARYING(255),
    task_version CHARACTER VARYING(255),
    namespace_name CHARACTER VARYING(255)
) AS $$
BEGIN
    RETURN QUERY
    WITH healthy_workers AS (
        SELECT 
            w.worker_id,
            w.worker_name,
            wr.status,
            wr.last_heartbeat_at,
            CASE 
                WHEN wr.last_heartbeat_at > NOW() - INTERVAL '1 minute' THEN 100
                WHEN wr.last_heartbeat_at > NOW() - INTERVAL '2 minutes' THEN 75
                WHEN wr.last_heartbeat_at IS NULL AND wr.registered_at > NOW() - INTERVAL '5 minutes' THEN 50
                ELSE 0
            END as health_score
        FROM tasker_workers w
        INNER JOIN tasker_worker_registrations wr ON w.worker_id = wr.worker_id
        WHERE wr.unregistered_at IS NULL
          AND wr.status IN ('registered', 'healthy')
          AND (
            wr.last_heartbeat_at > NOW() - INTERVAL '2 minutes'
            OR (wr.last_heartbeat_at IS NULL AND wr.registered_at > NOW() - INTERVAL '5 minutes')
          )
    )
    SELECT 
        wnt.id,
        wnt.worker_id,
        wnt.named_task_id,
        wnt.configuration,
        COALESCE(wnt.priority, 100) as priority,
        wnt.created_at,
        wnt.updated_at,
        hw.worker_name,
        nt.name as task_name,
        nt.version as task_version,
        tn.name as namespace_name
    FROM tasker_worker_named_tasks wnt
    INNER JOIN healthy_workers hw ON wnt.worker_id = hw.worker_id
    INNER JOIN tasker_named_tasks nt ON wnt.named_task_id = nt.named_task_id
    INNER JOIN tasker_task_namespaces tn ON nt.task_namespace_id = tn.task_namespace_id
    WHERE wnt.named_task_id = p_named_task_id
      AND hw.health_score > 0
    ORDER BY 
        hw.health_score DESC,
        wnt.priority DESC, 
        wnt.created_at;
END;
$$ LANGUAGE plpgsql STABLE;

-- Function to get worker health batch information
CREATE OR REPLACE FUNCTION get_worker_health_batch(p_worker_ids INTEGER[])
RETURNS TABLE (
    worker_id INTEGER,
    worker_name CHARACTER VARYING(255),
    status CHARACTER VARYING(50),
    last_heartbeat_at TIMESTAMP(6) WITHOUT TIME ZONE,
    connection_healthy BOOLEAN,
    current_load INTEGER
) AS $$
BEGIN
    RETURN QUERY
    SELECT 
        w.worker_id,
        w.worker_name,
        COALESCE(wr.status, 'unknown') as status,
        wr.last_heartbeat_at,
        CASE 
            WHEN wr.last_heartbeat_at > NOW() - INTERVAL '30 seconds' THEN true
            WHEN wr.last_heartbeat_at IS NULL AND wr.registered_at > NOW() - INTERVAL '2 minutes' THEN true
            ELSE false
        END as connection_healthy,
        0 as current_load  -- Placeholder - will be computed from actual load metrics
    FROM tasker_workers w
    LEFT JOIN tasker_worker_registrations wr ON w.worker_id = wr.worker_id
    WHERE w.worker_id = ANY(p_worker_ids)
      AND (wr.unregistered_at IS NULL OR wr.unregistered_at IS NULL)
    ORDER BY w.worker_id;
END;
$$ LANGUAGE plpgsql STABLE;

-- Function to select optimal worker for a task with comprehensive scoring
CREATE OR REPLACE FUNCTION select_optimal_worker_for_task(
    p_named_task_id INTEGER,
    p_required_capacity INTEGER DEFAULT 1
)
RETURNS TABLE (
    worker_id INTEGER,
    worker_name CHARACTER VARYING(255),
    priority INTEGER,
    configuration JSONB,
    health_score NUMERIC,
    current_load INTEGER,
    max_concurrent_steps INTEGER,
    available_capacity INTEGER,
    selection_score NUMERIC
) AS $$
BEGIN
    RETURN QUERY
    WITH worker_metrics AS (
        SELECT 
            w.worker_id,
            w.worker_name,
            wnt.priority,
            wnt.configuration,
            0 as current_load,
            10 as max_concurrent_steps,
            CASE 
                WHEN wr.last_heartbeat_at > NOW() - INTERVAL '30 seconds' THEN 100
                WHEN wr.last_heartbeat_at > NOW() - INTERVAL '1 minute' THEN 90
                WHEN wr.last_heartbeat_at > NOW() - INTERVAL '2 minutes' THEN 75
                WHEN wr.last_heartbeat_at IS NULL AND wr.registered_at > NOW() - INTERVAL '5 minutes' THEN 60
                ELSE 0
            END::NUMERIC as health_score,
            10 as available_capacity
        FROM tasker_workers w
        INNER JOIN tasker_worker_named_tasks wnt ON w.worker_id = wnt.worker_id
        INNER JOIN tasker_worker_registrations wr ON w.worker_id = wr.worker_id
        WHERE wnt.named_task_id = p_named_task_id
          AND wr.unregistered_at IS NULL
          AND wr.status IN ('registered', 'healthy')
          AND 10 >= p_required_capacity
    ),
    scored_workers AS (
        SELECT *,
            -- Comprehensive scoring algorithm
            (
                worker_metrics.health_score * 0.4 +
                (COALESCE(worker_metrics.priority, 100) / 100.0) * 0.3 +
                (worker_metrics.available_capacity / 10.0) * 100 * 0.3
            ) as selection_score
        FROM worker_metrics
        WHERE worker_metrics.health_score > 50
    )
    SELECT 
        sw.worker_id,
        sw.worker_name,
        COALESCE(sw.priority, 100) as priority,
        sw.configuration,
        sw.health_score,
        sw.current_load,
        sw.max_concurrent_steps,
        sw.available_capacity,
        sw.selection_score
    FROM scored_workers sw
    ORDER BY sw.selection_score DESC, sw.available_capacity DESC
    LIMIT 1;
END;
$$ LANGUAGE plpgsql STABLE;

-- Function to get worker pool statistics for monitoring
CREATE OR REPLACE FUNCTION get_worker_pool_statistics()
RETURNS TABLE (
    total_workers INTEGER,
    healthy_workers INTEGER,
    registered_workers INTEGER,
    active_workers INTEGER,
    workers_with_recent_heartbeat INTEGER,
    avg_health_score NUMERIC
) AS $$
BEGIN
    RETURN QUERY
    SELECT 
        COUNT(*)::INTEGER as total_workers,
        COUNT(CASE WHEN wr.status = 'healthy' THEN 1 END)::INTEGER as healthy_workers,
        COUNT(CASE WHEN wr.status IN ('registered', 'healthy') THEN 1 END)::INTEGER as registered_workers,
        COUNT(CASE WHEN wr.unregistered_at IS NULL THEN 1 END)::INTEGER as active_workers,
        COUNT(CASE WHEN wr.last_heartbeat_at > NOW() - INTERVAL '1 minute' THEN 1 END)::INTEGER as workers_with_recent_heartbeat,
        AVG(
            CASE 
                WHEN wr.last_heartbeat_at > NOW() - INTERVAL '1 minute' THEN 100
                WHEN wr.last_heartbeat_at > NOW() - INTERVAL '2 minutes' THEN 75
                WHEN wr.last_heartbeat_at IS NULL AND wr.registered_at > NOW() - INTERVAL '5 minutes' THEN 50
                ELSE 0
            END
        ) as avg_health_score
    FROM tasker_workers w
    LEFT JOIN tasker_worker_registrations wr ON w.worker_id = wr.worker_id;
END;
$$ LANGUAGE plpgsql STABLE;

-- Create a function to invalidate specific worker caches (placeholder for future cache invalidation)
CREATE OR REPLACE FUNCTION invalidate_worker_cache(p_named_task_id INTEGER DEFAULT NULL)
RETURNS BOOLEAN AS $$
BEGIN
    -- This function is a placeholder for cache invalidation logic
    -- In the future, this could trigger cache clearing in application layers
    -- For now, it simply returns true to indicate successful cache invalidation request
    
    -- Log the cache invalidation request
    RAISE NOTICE 'Cache invalidation requested for task_id: %', 
        COALESCE(p_named_task_id::TEXT, 'ALL');
    
    RETURN TRUE;
END;
$$ LANGUAGE plpgsql VOLATILE;