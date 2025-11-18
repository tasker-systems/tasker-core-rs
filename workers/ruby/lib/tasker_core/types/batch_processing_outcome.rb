# frozen_string_literal: true

require 'dry-struct'
require 'dry-types'

module TaskerCore
  module Types
    # BatchProcessingOutcome - TAS-59 Batch Processing Implementation
    #
    # Represents the outcome of a batch processing handler execution.
    # Batch processing handlers determine whether to create worker batches
    # based on business logic and data volume.
    #
    # ## Usage Patterns
    #
    # ### No Batches (No worker batches needed)
    # ```ruby
    # BatchProcessingOutcome.no_batches
    # ```
    #
    # ### Create Worker Batches
    # ```ruby
    # # Create batches for processing large datasets
    # cursor_configs = [
    #   { 'batch_id' => '001', 'start_cursor' => 1, 'end_cursor' => 200 },
    #   { 'batch_id' => '002', 'start_cursor' => 201, 'end_cursor' => 400 },
    #   { 'batch_id' => '003', 'start_cursor' => 401, 'end_cursor' => 600 }
    # ]
    #
    # BatchProcessingOutcome.create_batches(
    #   worker_template_name: 'process_batch_worker',
    #   worker_count: 3,
    #   cursor_configs: cursor_configs,
    #   total_items: 250
    # )
    # ```
    #
    # ## Integration with Step Handlers
    #
    # Batch processing handlers should return this outcome in their result:
    #
    # ```ruby
    # def call(task, sequence, step)
    #   # Business logic to determine if batching is needed
    #   total_records = task.context['total_records']
    #
    #   outcome = if total_records > 1000
    #     # Create batches for parallel processing
    #     cursor_configs = calculate_cursor_configs(total_records)
    #     BatchProcessingOutcome.create_batches(
    #       worker_template_name: 'batch_processor',
    #       worker_count: cursor_configs.size,
    #       cursor_configs: cursor_configs,
    #       total_items: total_records
    #     )
    #   else
    #     # Process inline, no batches needed
    #     BatchProcessingOutcome.no_batches
    #   end
    #
    #   StepHandlerCallResult.success(
    #     result: {
    #       batch_processing_outcome: outcome.to_h
    #     },
    #     metadata: {
    #       operation: 'batch_decision',
    #       batches_created: outcome.requires_batch_creation? ? cursor_configs.size : 0
    #     }
    #   )
    # end
    # ```
    module BatchProcessingOutcome
      module Types
        include Dry.Types()
      end

      # NoBatches outcome - no worker batches needed
      class NoBatches < Dry::Struct
        # Type discriminator (matches Rust serde tag field)
        attribute :type, Types::String.default('no_batches')

        # Convert to hash for serialization to Rust
        # Note: Rust expects lowercase snake_case due to #[serde(rename_all = "snake_case")]
        def to_h
          { type: 'no_batches' }
        end

        # Check if this outcome requires batch creation
        def requires_batch_creation?
          false
        end
      end

      # CreateBatches outcome - create worker batches for parallel processing
      class CreateBatches < Dry::Struct
        # Type discriminator (matches Rust serde tag field)
        attribute :type, Types::String.default('create_batches')

        # Name of the worker template to use for batch processing
        attribute :worker_template_name, Types::Strict::String

        # Number of worker batches to create
        attribute :worker_count, Types::Strict::Integer.constrained(gteq: 0)

        # Array of cursor configurations for batch processing
        # Each config is a flexible hash (e.g., batch_id, start_cursor, end_cursor, batch_size)
        attribute :cursor_configs, Types::Array.of(Types::Hash).constrained(min_size: 1)

        # Total number of items to be processed across all batches
        attribute :total_items, Types::Strict::Integer.constrained(gteq: 0)

        # Convert to hash for serialization to Rust
        # Note: Rust expects lowercase snake_case due to #[serde(rename_all = "snake_case")]
        def to_h
          {
            'type' => 'create_batches',
            'worker_template_name' => worker_template_name,
            'worker_count' => worker_count,
            'cursor_configs' => cursor_configs,
            'total_items' => total_items
          }
        end

        # Check if this outcome requires batch creation
        def requires_batch_creation?
          true
        end
      end

      # Factory methods for creating outcomes
      class << self
        # Create a NoBatches outcome
        #
        # Use when the batch processing handler determines that no worker
        # batches are needed (e.g., small dataset, inline processing).
        #
        # @return [NoBatches] A no-batches outcome instance
        #
        # @example
        #   BatchProcessingOutcome.no_batches
        def no_batches
          NoBatches.new
        end

        # Create a CreateBatches outcome with specified batch configuration
        #
        # Use when the batch processing handler determines that worker batches
        # should be created for parallel processing.
        #
        # @param worker_template_name [String] Name of worker template to use
        # @param worker_count [Integer] Number of worker batches to create (must be >= 0)
        # @param cursor_configs [Array<Hash>] Array of cursor configurations
        # @param total_items [Integer] Total number of items to process (must be >= 0)
        # @return [CreateBatches] A create-batches outcome instance
        # @raise [ArgumentError] if required parameters are invalid
        #
        # @example Create batches for CSV row processing
        #   cursor_configs = [
        #     { 'batch_id' => '001', 'start_cursor' => 1, 'end_cursor' => 200, 'batch_size' => 200 },
        #     { 'batch_id' => '002', 'start_cursor' => 201, 'end_cursor' => 400, 'batch_size' => 200 }
        #   ]
        #   BatchProcessingOutcome.create_batches(
        #     worker_template_name: 'process_csv_batch',
        #     worker_count: 2,
        #     cursor_configs: cursor_configs,
        #     total_items: 400
        #   )
        def create_batches(worker_template_name:, worker_count:, cursor_configs:, total_items:)
          if worker_template_name.nil? || worker_template_name.empty?
            raise ArgumentError,
                  'worker_template_name cannot be empty'
          end
          raise ArgumentError, 'cursor_configs cannot be empty' if cursor_configs.nil? || cursor_configs.empty?

          CreateBatches.new(
            worker_template_name: worker_template_name,
            worker_count: worker_count,
            cursor_configs: cursor_configs,
            total_items: total_items
          )
        end

        # Parse a BatchProcessingOutcome from a hash
        #
        # Used to deserialize outcomes from step handler results.
        # Supports both symbol and string keys for maximum flexibility.
        #
        # @param hash [Hash] The hash representation of an outcome
        # @return [NoBatches, CreateBatches, nil] The parsed outcome or nil if invalid
        #
        # @example Parse no-batches outcome
        #   outcome = BatchProcessingOutcome.from_hash({
        #     type: 'no_batches'
        #   })
        #
        # @example Parse create-batches outcome
        #   outcome = BatchProcessingOutcome.from_hash({
        #     'type' => 'create_batches',
        #     'worker_template_name' => 'batch_worker',
        #     'worker_count' => 3,
        #     'cursor_configs' => [
        #       { 'cursor_key' => 'offset', 'cursor_value' => 0 }
        #     ],
        #     'total_items' => 300
        #   })
        def from_hash(hash)
          return nil unless hash.is_a?(Hash)

          # Support both symbol and string keys
          # Note: Rust sends lowercase snake_case due to #[serde(rename_all = "snake_case")]
          hash = deep_symbolize_keys(hash)
          outcome_type = hash[:type]

          case outcome_type
          when 'no_batches'
            NoBatches.new
          when 'create_batches'
            worker_template_name = hash[:worker_template_name]
            worker_count = hash[:worker_count]
            cursor_configs = hash[:cursor_configs]
            total_items = hash[:total_items]

            return nil if worker_template_name.nil? || worker_template_name.empty?
            return nil if worker_count.nil? || worker_count.negative?
            return nil unless cursor_configs.is_a?(Array) && !cursor_configs.empty?
            return nil if total_items.nil? || total_items.negative?

            CreateBatches.new(
              worker_template_name: worker_template_name,
              worker_count: worker_count,
              cursor_configs: cursor_configs,
              total_items: total_items
            )
          end
        end

        private

        # Deep symbolize hash keys for consistent access
        # Handles nested hashes and arrays of hashes
        def deep_symbolize_keys(hash)
          return hash unless hash.is_a?(Hash)

          hash.each_with_object({}) do |(key, value), result|
            new_key = key.to_sym
            new_value = case value
                        when Hash
                          deep_symbolize_keys(value)
                        when Array
                          value.map { |v| v.is_a?(Hash) ? deep_symbolize_keys(v) : v }
                        else
                          value
                        end
            result[new_key] = new_value
          end
        end
      end
    end
  end
end
