-- Add unique constraints for better data integrity and concurrent access

-- Add unique constraint on dependent_systems.name
ALTER TABLE tasker_dependent_systems 
ADD CONSTRAINT unique_dependent_system_name UNIQUE (name);

-- NOTE: Named steps should NOT be unique per dependent_system - multiple tasks 
-- can have steps with the same name like "verify_order_status"
-- Uniqueness should be enforced at the named_task level if needed