# frozen_string_literal: true

require 'dry-struct'
require 'dry-types'

module TaskerCore
  module Types
    # Include Dry.Types for access to Types::String, etc.
    include Dry.Types()

    # UUID validation regex pattern (defined at module level for reuse)
    # Updated to accept UUID v7 (version nibble can be 7)
    UUID_REGEX = /\A[0-9a-f]{8}-[0-9a-f]{4}-[1-7][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}\z/i

    # Simple message structure for UUID-based step processing
    #
    # This replaces the complex nested StepMessage with a minimal 3-field structure
    # that leverages the shared database as the API layer. Ruby workers receive this
    # simple message and use the UUIDs to fetch ActiveRecord models directly.
    #
    # Benefits:
    # - 80%+ message size reduction (3 UUIDs vs complex nested JSON)
    # - Eliminates type conversion issues (no hash-to-object conversion)
    # - Prevents stale queue messages (UUIDs are globally unique)
    # - Real ActiveRecord models for handlers (full ORM functionality)
    # - Database as single source of truth
    #
    # @example Message structure
    #   {
    #     "task_uuid": "550e8400-e29b-41d4-a716-446655440001",
    #     "step_uuid": "550e8400-e29b-41d4-a716-446655440002",
    #     "ready_dependency_step_uuids": [
    #       "550e8400-e29b-41d4-a716-446655440003",
    #       "550e8400-e29b-41d4-a716-446655440004"
    #     ]
    #   }
    #
    # @example Ruby processing
    #   # 1. Receive simple message
    #   task = TaskerCore::Database::Models::Task.find_by!(task_uuid: message.task_uuid)
    #   step = TaskerCore::Database::Models::WorkflowStep.find_by!(step_uuid: message.step_uuid)
    #   dependencies = TaskerCore::Database::Models::WorkflowStep.where(
    #     step_uuid: message.ready_dependency_step_uuids
    #   ).includes(:results)
    #
    #   # 2. Create context and call handler
    #   context = TaskerCore::Types::StepContext.new(step_data)
    #   handler.call(context)
    class SimpleStepMessage < Dry::Struct
      # Make the struct flexible for additional attributes if needed
      transform_keys(&:to_sym)

      # Task UUID from tasker_tasks.task_uuid
      attribute :task_uuid, Types::String.constrained(format: UUID_REGEX)

      # Step UUID from tasker_workflow_steps.step_uuid
      attribute :step_uuid, Types::String.constrained(format: UUID_REGEX)

      # Array of dependency step UUIDs that are ready/completed
      # Empty array means no dependencies or root step
      attribute :ready_dependency_step_uuids, Types::Array.of(
        Types::String.constrained(format: UUID_REGEX)
      ).default([].freeze)

      # Convert to hash for JSON serialization
      # @return [Hash] hash representation suitable for JSON
      def to_h
        {
          task_uuid: task_uuid,
          step_uuid: step_uuid,
          ready_dependency_step_uuids: ready_dependency_step_uuids
        }
      end

      # Create from hash (for deserialization from JSON)
      # @param hash [Hash] hash representation
      # @return [SimpleStepMessage] new simple message instance
      def self.from_hash(hash)
        symbolized = hash.transform_keys(&:to_sym)
        new(symbolized)
      end

      # Validate that all UUIDs exist in the database
      # @return [Boolean] true if all referenced records exist
      def valid_references?
        task_exists? && step_exists? && all_dependencies_exist?
      end

      # Check if the task UUID exists in the database
      # @return [Boolean] true if task exists
      def task_exists?
        TaskerCore::Database::Models::Task.exists?(task_uuid: task_uuid)
      end

      # Check if the step UUID exists in the database
      # @return [Boolean] true if step exists
      def step_exists?
        TaskerCore::Database::Models::WorkflowStep.exists?(workflow_step_uuid: step_uuid)
      end

      # Check if all dependency UUIDs exist in the database
      # @return [Boolean] true if all dependencies exist
      def all_dependencies_exist?
        return true if ready_dependency_step_uuids.empty?

        existing_count = TaskerCore::Database::Models::WorkflowStep
                         .where(workflow_step_uuid: ready_dependency_step_uuids)
                         .count

        existing_count == ready_dependency_step_uuids.length
      end

      # Fetch the actual task record from the database
      # @return [TaskerCore::Database::Models::Task] the task record
      # @raise [ActiveRecord::RecordNotFound] if task doesn't exist
      def fetch_task
        TaskerCore::Database::Models::Task.find_by!(task_uuid: task_uuid)
      end

      # Fetch the actual step record from the database
      # @return [TaskerCore::Database::Models::WorkflowStep] the step record
      # @raise [ActiveRecord::RecordNotFound] if step doesn't exist
      def fetch_step
        TaskerCore::Database::Models::WorkflowStep.find_by!(workflow_step_uuid: step_uuid)
      end

      # Fetch the dependency step records from the database
      # @return [ActiveRecord::Relation<TaskerCore::Database::Models::WorkflowStep>] dependency steps
      def fetch_dependencies
        return TaskerCore::Database::Models::WorkflowStep.none if ready_dependency_step_uuids.empty?

        TaskerCore::Database::Models::WorkflowStep
          .where(workflow_step_uuid: ready_dependency_step_uuids)
          .includes(:results, :named_step) # Preload commonly needed associations
      end

      # Create a step message for testing with generated UUIDs
      # @param task_uuid [String] task UUID
      # @param step_uuid [String] step UUID
      # @param ready_dependency_step_uuids [Array<String>] dependency step UUIDs
      # @return [SimpleStepMessage] new simple message
      def self.build_test(task_uuid:, step_uuid:, ready_dependency_step_uuids: [])
        new(
          task_uuid: task_uuid,
          step_uuid: step_uuid,
          ready_dependency_step_uuids: ready_dependency_step_uuids
        )
      end
    end
  end
end
