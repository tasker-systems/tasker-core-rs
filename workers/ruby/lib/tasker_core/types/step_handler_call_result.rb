# frozen_string_literal: true

require 'dry-struct'
require 'dry-types'

module TaskerCore
  module Types
    # Standardized result structure for step handler call methods
    #
    # This provides a consistent interface for all step handlers to return
    # structured results with proper metadata for observability.
    #
    # Success example:
    #   StepHandlerCallResult.success(
    #     result: { validated_items: [...], total: 100.00 },
    #     metadata: {
    #       operation: "validate_order",
    #       item_count: 3,
    #       processing_time_ms: 125
    #     }
    #   )
    #
    # Error example (typically raised as exception):
    #   raise TaskerCore::Errors::PermanentError.new(
    #     "Invalid order total",
    #     error_code: "INVALID_TOTAL",
    #     context: { total: -50, min_allowed: 0 }
    #   )
    module StepHandlerCallResult
      class Success < Dry::Struct
        # Always true for success results
        attribute :success, Types::Bool.default(true)

        # The actual result data from the step handler
        # Can be any JSON-serializable value (Hash, Array, String, Number, etc.)
        attribute :result, Types::Any

        # Optional metadata for observability
        attribute(:metadata, Types::Hash.default { {} })

        # Predicate method for checking if result is successful
        def success?
          success
        end

        # TAS-125: Not a checkpoint - this is a final success result
        def checkpoint?
          false
        end
      end

      class Error < Dry::Struct
        # Always false for error results
        attribute :success, Types::Bool.default(false)

        # Error type (matches our error class names)
        attribute :error_type,
                  Types::String.enum('PermanentError', 'RetryableError', 'ValidationError', 'UnexpectedError',
                                     'StepCompletionError')

        # Human-readable error message
        attribute :message, Types::String

        # Optional error code for categorization
        attribute :error_code, Types::String.optional

        # Whether this error should trigger a retry
        attribute :retryable, Types::Bool.default(false)

        # Additional error context and metadata
        attribute(:metadata, Types::Hash.default { {} })

        # Predicate method for checking if result is successful
        def success?
          success
        end

        # TAS-125: Not a checkpoint - this is a final error result
        def checkpoint?
          false
        end
      end

      # TAS-125: Checkpoint yield result for batch processing
      #
      # This result type signals that a batch processing handler wants to
      # persist its progress (checkpoint) and be re-dispatched for continued
      # processing. Unlike Success or Error, this is an intermediate state
      # that doesn't complete the step.
      #
      # Example:
      #   StepHandlerCallResult.checkpoint_yield(
      #     cursor: 1000,
      #     items_processed: 1000,
      #     accumulated_results: { total_amount: 50000.00 }
      #   )
      class CheckpointYield < Dry::Struct
        # Position to resume from (Integer, String, or Hash)
        # For offset-based pagination: Integer (row number)
        # For cursor-based pagination: String or Hash (opaque cursor)
        attribute :cursor, Types::Any

        # Total number of items processed so far (cumulative across all yields)
        attribute :items_processed, Types::Integer.constrained(gteq: 0)

        # Optional partial aggregations to carry forward to next iteration
        # Useful for running totals, counters, or intermediate calculations
        attribute :accumulated_results, Types::Any.optional

        # This is a checkpoint, not a final result
        def checkpoint?
          true
        end

        # Not successful yet - still in progress
        def success?
          false
        end

        # Convert to hash for FFI transport
        def to_checkpoint_data(event_id:, step_uuid:)
          {
            event_id: event_id,
            step_uuid: step_uuid,
            cursor: cursor,
            items_processed: items_processed,
            accumulated_results: accumulated_results
          }
        end
      end

      # Factory methods for creating results
      class << self
        # Create a success result
        #
        # @param result [Object] The result data (must be JSON-serializable)
        # @param metadata [Hash] Optional metadata for observability
        # @return [Success] A success result instance
        def success(result:, metadata: {})
          Success.new(
            success: true,
            result: result,
            metadata: metadata
          )
        end

        # Create an error result
        #
        # @param error_type [String] Type of error (PermanentError, RetryableError, etc.)
        # @param message [String] Human-readable error message
        # @param error_code [String, nil] Optional error code
        # @param retryable [Boolean] Whether to retry this error
        # @param metadata [Hash] Additional error context
        # @return [Error] An error result instance
        def error(error_type:, message:, error_code: nil, retryable: false, metadata: {})
          Error.new(
            success: false,
            error_type: error_type,
            message: message,
            error_code: error_code,
            retryable: retryable,
            metadata: metadata
          )
        end

        # TAS-125: Create a checkpoint yield result for batch processing
        #
        # Use this when your batch processing handler wants to persist progress
        # and be re-dispatched for continued processing.
        #
        # @param cursor [Integer, String, Hash] Position to resume from
        # @param items_processed [Integer] Total items processed so far (cumulative)
        # @param accumulated_results [Hash, nil] Optional partial aggregations
        # @return [CheckpointYield] A checkpoint yield result instance
        #
        # @example Yield checkpoint with offset cursor
        #   StepHandlerCallResult.checkpoint_yield(
        #     cursor: 1000,
        #     items_processed: 1000,
        #     accumulated_results: { total_amount: 50000.00, row_count: 1000 }
        #   )
        #
        # @example Yield checkpoint with opaque cursor
        #   StepHandlerCallResult.checkpoint_yield(
        #     cursor: { page_token: "eyJsYXN0X2lkIjo5OTl9" },
        #     items_processed: 500
        #   )
        def checkpoint_yield(cursor:, items_processed:, accumulated_results: nil)
          CheckpointYield.new(
            cursor: cursor,
            items_processed: items_processed,
            accumulated_results: accumulated_results
          )
        end

        # Convert arbitrary handler output to a StepHandlerCallResult
        #
        # @param output [Object] The output from a handler's call method
        # @return [Success, Error, CheckpointYield] A properly structured result
        def from_handler_output(output)
          case output
          when Success, Error, CheckpointYield
            # Already a proper result
            output
          when Hash
            # Check if it looks like a result structure
            if output[:success] == true || output['success'] == true
              # Looks like a success result
              Success.new(
                success: true,
                result: output[:result] || output['result'] || output,
                metadata: output[:metadata] || output['metadata'] || {}
              )
            elsif output[:success] == false || output['success'] == false
              # Looks like an error result
              Error.new(
                success: false,
                error_type: output[:error_type] || output['error_type'] || 'UnexpectedError',
                message: output[:message] || output['message'] || 'Unknown error',
                error_code: output[:error_code] || output['error_code'],
                retryable: output[:retryable] || output['retryable'] || false,
                metadata: output[:metadata] || output['metadata'] || {}
              )
            else
              # Just a hash of results - wrap it as success
              Success.new(
                success: true,
                result: output,
                metadata: {
                  wrapped: true,
                  original_type: 'Hash'
                }
              )
            end
          else
            # Any other type - wrap as success
            Success.new(
              success: true,
              result: output,
              metadata: {
                wrapped: true,
                original_type: output.class.name
              }
            )
          end
        end

        # Create error result from an exception
        #
        # @param exception [Exception] The exception that was raised
        # @return [Error] An error result instance
        def from_exception(exception)
          case exception
          when TaskerCore::Errors::PermanentError
            Error.new(
              success: false,
              error_type: 'PermanentError',
              message: exception.message,
              error_code: exception.error_code,
              retryable: false,
              metadata: {
                context: exception.context || {}
              }.compact
            )
          when TaskerCore::Errors::RetryableError
            Error.new(
              success: false,
              error_type: 'RetryableError',
              message: exception.message,
              error_code: nil, # RetryableError doesn't have error_code
              retryable: true,
              metadata: {
                context: exception.context || {},
                retry_after: exception.retry_after
              }.compact
            )
          when TaskerCore::Errors::ValidationError
            Error.new(
              success: false,
              error_type: 'ValidationError',
              message: exception.message,
              error_code: exception.error_code,
              retryable: false,
              metadata: {
                context: exception.context || {},
                field: exception.field
              }.compact
            )
          else
            # Generic exception
            Error.new(
              success: false,
              error_type: 'UnexpectedError',
              message: exception.message,
              error_code: nil,
              retryable: true,
              metadata: {
                exception_class: exception.class.name,
                stack_trace: exception.backtrace&.first(10)&.join("\n")
              }.compact
            )
          end
        end
      end
    end
  end
end
