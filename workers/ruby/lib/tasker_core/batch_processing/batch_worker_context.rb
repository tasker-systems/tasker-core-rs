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
    #   start = context.start_cursor  # => 0
    #   end_pos = context.end_cursor  # => 1000
    #   batch_id = context.batch_id     # => "001"
    #
    # == Cursor Flexibility
    #
    # Cursors are intentionally flexible to support diverse business logic scenarios.
    # The system validates numeric cursors when both values are integers, but cursors
    # can also be alphanumeric strings, timestamps, UUIDs, or any comparable type that
    # makes sense for your data partitioning strategy.
    #
    # With great power comes great responsibility: ensure your cursor implementation
    # matches your data source's capabilities and that your handler logic correctly
    # interprets the cursor boundaries.
    #
    # @example Numeric cursors (validated for ordering)
    #   cursor: {
    #     batch_id: '001',
    #     start_cursor: 0,      # Integer cursors are validated
    #     end_cursor: 1000
    #   }
    #
    # @example Alphanumeric cursors (developer responsibility)
    #   cursor: {
    #     batch_id: '002',
    #     start_cursor: 'A',    # String cursors for alphabetical ranges
    #     end_cursor: 'M'
    #   }
    #
    # @example UUID-based cursors (developer responsibility)
    #   cursor: {
    #     batch_id: '003',
    #     start_cursor: '00000000-0000-0000-0000-000000000000',
    #     end_cursor: '88888888-8888-8888-8888-888888888888'
    #   }
    #
    # @example Timestamp cursors (developer responsibility)
    #   cursor: {
    #     batch_id: '004',
    #     start_cursor: '2024-01-01T00:00:00Z',
    #     end_cursor: '2024-01-31T23:59:59Z'
    #   }
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

      # Get the starting cursor in the dataset (inclusive)
      #
      # @return [Integer] Starting cursor position
      def start_cursor
        cursor[:start_cursor].to_i
      end

      # Get the ending cursor in the dataset (exclusive)
      #
      # @return [Integer] Ending cursor position
      def end_cursor
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

        # Extract cursor values for validation
        start_val = cursor[:start_cursor]
        end_val = cursor[:end_cursor]

        # Validate numeric cursors if they're integers
        # (supports both integer and other types like timestamps/UUIDs)
        return unless start_val.is_a?(Integer) || end_val.is_a?(Integer)

        # If one is integer, both should be integers
        unless start_val.is_a?(Integer) && end_val.is_a?(Integer)
          raise ArgumentError,
                "Mixed cursor types not allowed: start=#{start_val.class}, end=#{end_val.class}"
        end

        # Validate non-negative values
        if start_val.negative?
          raise ArgumentError,
                "start_cursor must be non-negative, got #{start_val}"
        end
        if end_val.negative?
          raise ArgumentError,
                "end_cursor must be non-negative, got #{end_val}"
        end

        # Validate logical ordering
        if start_val > end_val
          raise ArgumentError,
                "start_cursor (#{start_val}) must be <= end_cursor (#{end_val})"
        end

        # Warn about zero-length ranges (likely a bug)
        return unless start_val == end_val

        warn "WARNING: Zero-length cursor range (#{start_val} == #{end_val}) " \
             "- worker will process no data. batch_id=#{cursor[:batch_id]}"
      end
    end
  end
end
