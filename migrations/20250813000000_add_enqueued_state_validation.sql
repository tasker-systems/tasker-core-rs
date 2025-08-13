-- TAS-32: Add Enqueued State Validation Migration
--
-- This migration adds proper state validation for the new 'enqueued' state
-- introduced in TAS-32 for queue state management improvements.
--
-- GOALS:
-- 1. Add CHECK constraints to validate workflow step state values
-- 2. Include 'enqueued' as a valid state in the validation
-- 3. Ensure both to_state and from_state columns have proper validation
-- 4. Maintain backward compatibility with existing data
--
-- ARCHITECTURE CHANGE:
-- - Steps are now marked 'enqueued' when placed in queue (not 'in_progress')
-- - Workers transition steps from 'enqueued' to 'in_progress' when claiming
-- - Database becomes single source of truth for step processing state
-- - Queue used only for messaging, not state management

-- =============================================================================
-- ADD STATE VALIDATION CONSTRAINTS
-- =============================================================================

-- Valid workflow step states including the new 'enqueued' state
-- These match the constants defined in both Rust and Ruby state machines
DO $$
BEGIN
    -- Add constraint for to_state column (required field)
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'chk_workflow_step_transitions_to_state'
        AND table_name = 'tasker_workflow_step_transitions'
        AND table_schema = 'public'
    ) THEN
        ALTER TABLE public.tasker_workflow_step_transitions
        ADD CONSTRAINT chk_workflow_step_transitions_to_state
        CHECK (to_state IN (
            'pending',
            'enqueued',
            'in_progress', 
            'complete',
            'error',
            'cancelled',
            'resolved_manually'
        ));
        
        RAISE NOTICE 'Added to_state validation constraint with enqueued state';
    ELSE
        RAISE NOTICE 'to_state constraint already exists, skipping';
    END IF;

    -- Add constraint for from_state column (nullable field)
    -- NULL values are allowed for initial transitions
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'chk_workflow_step_transitions_from_state'
        AND table_name = 'tasker_workflow_step_transitions'
        AND table_schema = 'public'
    ) THEN
        ALTER TABLE public.tasker_workflow_step_transitions
        ADD CONSTRAINT chk_workflow_step_transitions_from_state
        CHECK (from_state IS NULL OR from_state IN (
            'pending',
            'enqueued',
            'in_progress',
            'complete', 
            'error',
            'cancelled',
            'resolved_manually'
        ));
        
        RAISE NOTICE 'Added from_state validation constraint with enqueued state';
    ELSE
        RAISE NOTICE 'from_state constraint already exists, skipping';
    END IF;
END
$$;

-- =============================================================================
-- TASK TRANSITIONS STATE VALIDATION (FOR CONSISTENCY)
-- =============================================================================

-- Also add state validation for task transitions to maintain consistency
-- Task states do not include 'enqueued' as that is specific to workflow steps
DO $$
BEGIN
    -- Add constraint for task to_state column
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'chk_task_transitions_to_state'
        AND table_name = 'tasker_task_transitions'
        AND table_schema = 'public'
    ) THEN
        ALTER TABLE public.tasker_task_transitions
        ADD CONSTRAINT chk_task_transitions_to_state
        CHECK (to_state IN (
            'pending',
            'in_progress',
            'complete',
            'error', 
            'cancelled',
            'resolved_manually'
        ));
        
        RAISE NOTICE 'Added task to_state validation constraint';
    ELSE
        RAISE NOTICE 'Task to_state constraint already exists, skipping';
    END IF;

    -- Add constraint for task from_state column
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.table_constraints 
        WHERE constraint_name = 'chk_task_transitions_from_state'
        AND table_name = 'tasker_task_transitions'
        AND table_schema = 'public'
    ) THEN
        ALTER TABLE public.tasker_task_transitions
        ADD CONSTRAINT chk_task_transitions_from_state
        CHECK (from_state IS NULL OR from_state IN (
            'pending',
            'in_progress',
            'complete',
            'error',
            'cancelled', 
            'resolved_manually'
        ));
        
        RAISE NOTICE 'Added task from_state validation constraint';
    ELSE
        RAISE NOTICE 'Task from_state constraint already exists, skipping';
    END IF;
END
$$;

-- =============================================================================
-- VALIDATION QUERIES FOR EXISTING DATA
-- =============================================================================

-- Verify that existing data is compatible with new constraints
-- This query will help identify any invalid states in existing data
DO $$
DECLARE
    invalid_step_to_states INTEGER;
    invalid_step_from_states INTEGER;
    invalid_task_to_states INTEGER;
    invalid_task_from_states INTEGER;
BEGIN
    -- Check for invalid workflow step to_states
    SELECT COUNT(*) INTO invalid_step_to_states
    FROM public.tasker_workflow_step_transitions
    WHERE to_state NOT IN ('pending', 'enqueued', 'in_progress', 'complete', 'error', 'cancelled', 'resolved_manually');
    
    -- Check for invalid workflow step from_states
    SELECT COUNT(*) INTO invalid_step_from_states  
    FROM public.tasker_workflow_step_transitions
    WHERE from_state IS NOT NULL 
    AND from_state NOT IN ('pending', 'enqueued', 'in_progress', 'complete', 'error', 'cancelled', 'resolved_manually');

    -- Check for invalid task to_states
    SELECT COUNT(*) INTO invalid_task_to_states
    FROM public.tasker_task_transitions  
    WHERE to_state NOT IN ('pending', 'in_progress', 'complete', 'error', 'cancelled', 'resolved_manually');

    -- Check for invalid task from_states
    SELECT COUNT(*) INTO invalid_task_from_states
    FROM public.tasker_task_transitions
    WHERE from_state IS NOT NULL
    AND from_state NOT IN ('pending', 'in_progress', 'complete', 'error', 'cancelled', 'resolved_manually');

    -- Report validation results
    RAISE NOTICE 'DATA VALIDATION RESULTS:';
    RAISE NOTICE '  Invalid workflow step to_states: %', invalid_step_to_states;
    RAISE NOTICE '  Invalid workflow step from_states: %', invalid_step_from_states;
    RAISE NOTICE '  Invalid task to_states: %', invalid_task_to_states;
    RAISE NOTICE '  Invalid task from_states: %', invalid_task_from_states;
    
    IF invalid_step_to_states > 0 OR invalid_step_from_states > 0 OR 
       invalid_task_to_states > 0 OR invalid_task_from_states > 0 THEN
        RAISE WARNING 'Found invalid state values in existing data. Review before applying constraints.';
    ELSE
        RAISE NOTICE 'All existing data is compatible with new state constraints.';
    END IF;
END
$$;

-- =============================================================================
-- MIGRATION COMPLETION CONFIRMATION
-- =============================================================================

-- Confirm that the migration completed successfully
DO $$
BEGIN
    RAISE NOTICE '';
    RAISE NOTICE '=============================================================================';
    RAISE NOTICE 'TAS-32 ENQUEUED STATE VALIDATION MIGRATION COMPLETED';
    RAISE NOTICE '=============================================================================';
    RAISE NOTICE '';
    RAISE NOTICE 'CHANGES APPLIED:';
    RAISE NOTICE '  ✅ Added workflow step state validation (including "enqueued")';
    RAISE NOTICE '  ✅ Added task state validation (standard states only)';
    RAISE NOTICE '  ✅ Validated existing data compatibility';
    RAISE NOTICE '';
    RAISE NOTICE 'NEXT STEPS:';
    RAISE NOTICE '  1. Update SQL functions to handle "enqueued" state correctly';
    RAISE NOTICE '  2. Update orchestration logic to use "enqueued" instead of "in_progress"';
    RAISE NOTICE '  3. Update worker claiming logic to transition "enqueued" → "in_progress"';
    RAISE NOTICE '';
    RAISE NOTICE 'ARCHITECTURE BENEFITS:';
    RAISE NOTICE '  • Database is now single source of truth for step processing state';
    RAISE NOTICE '  • Queue used only for messaging, not state management';
    RAISE NOTICE '  • Eliminates race conditions in step claiming';
    RAISE NOTICE '  • Improves idempotency guarantees';
    RAISE NOTICE '';
    RAISE NOTICE '=============================================================================';
END
$$;