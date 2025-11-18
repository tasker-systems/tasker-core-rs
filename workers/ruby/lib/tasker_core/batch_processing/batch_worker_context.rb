# frozen_string_literal: true

module TaskerCore
  module BatchProcessing
    # Ruby equivalent of Rust's BatchWorkerContext
    #
    # Extracts cursor config from workflow_step.inputs via WorkflowStepWrapper.
    # This mirrors the Rust implementation which deserializes BatchWorkerInputs
    # from the workflow_step.inputs field.
    #
    # @example Checking if worker is no-op
    #   context = BatchWorkerContext.from_step_data(sequence_step)
    #   if context.no_op?
    #     # Handle placeholder worker for NoBatches scenario
    #     return success_result(message: 'No-op worker completed')
    #   end
    #
    # @example Accessing cursor configuration
    #   context = BatchWorkerContext.from_step_data(sequence_step)
    #   start = context.start_position  # => 0
    #   end_pos = context.end_position  # => 1000
    #   batch_id = context.batch_id     # => "001"
    class BatchWorkerContext
      attr_reader :cursor, :batch_metadata, :is_no_op

      # Extract context from WorkflowStepWrapper
      #
      # @param workflow_step [WorkflowStepWrapper] Workflow step wrapper
      # @return [BatchWorkerContext] Extracted context
      def self.from_step_data(workflow_step)
        new(workflow_step)
      end

      # Initialize batch worker context from workflow step
      #
      # Reads BatchWorkerInputs from workflow_step.inputs (instance data)
      # not from step_definition.handler.initialization (template data).
      #
      # @param workflow_step [WorkflowStepWrapper] Workflow step wrapper
      def initialize(workflow_step)
        # Access inputs via WorkflowStepWrapper
        # Use ActiveSupport deep_symbolize_keys for clean key access
        inputs = (workflow_step.inputs || {}).deep_symbolize_keys

        @is_no_op = inputs[:is_no_op] == true

        if @is_no_op
          # Placeholder worker - minimal context
          @cursor = {}
          @batch_metadata = {}
        else
          @cursor = inputs[:cursor] || {}
          @batch_metadata = inputs[:batch_metadata] || {}

          validate_cursor!
        end
      end

      # Get the starting position in the dataset (inclusive)
      #
      # @return [Integer] Starting cursor position
      def start_position
        cursor[:start_cursor].to_i
      end

      # Get the ending position in the dataset (exclusive)
      #
      # @return [Integer] Ending cursor position
      def end_position
        cursor[:end_cursor].to_i
      end

      # Get the batch identifier
      #
      # @return [String] Batch ID (e.g., "001", "002", "000" for no-op)
      def batch_id
        cursor[:batch_id] || 'unknown'
      end

      # Get the checkpoint interval
      #
      # Workers should update checkpoint progress after processing this many items.
      #
      # @return [Integer] Checkpoint interval
      def checkpoint_interval
        batch_metadata[:checkpoint_interval]&.to_i || 100
      end

      # Check if this is a no-op/placeholder worker
      #
      # @return [Boolean] True if this worker should skip processing
      def no_op?
        @is_no_op
      end

      private

      # Validate cursor configuration for real workers
      #
      # @raise [ArgumentError] If cursor configuration is invalid
      def validate_cursor!
        return if @is_no_op

        raise ArgumentError, 'Missing cursor configuration' if cursor.empty?
        raise ArgumentError, 'Missing batch_id' unless cursor[:batch_id]
        raise ArgumentError, 'Missing start_cursor' unless cursor.key?(:start_cursor)
        raise ArgumentError, 'Missing end_cursor' unless cursor.key?(:end_cursor)
      end
    end
  end
end
