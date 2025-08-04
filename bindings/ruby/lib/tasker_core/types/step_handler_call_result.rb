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
        attribute :metadata, Types::Hash.default { {} }
      end

      class Error < Dry::Struct
        # Always false for error results
        attribute :success, Types::Bool.default(false)
        
        # Error type (matches our error class names)
        attribute :error_type, Types::String.enum('PermanentError', 'RetryableError', 'ValidationError', 'UnexpectedError')
        
        # Human-readable error message
        attribute :message, Types::String
        
        # Optional error code for categorization
        attribute :error_code, Types::String.optional
        
        # Whether this error should trigger a retry
        attribute :retryable, Types::Bool.default(false)
        
        # Additional error context and metadata
        attribute :metadata, Types::Hash.default { {} }
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

        # Convert arbitrary handler output to a StepHandlerCallResult
        #
        # @param output [Object] The output from a handler's call method
        # @return [Success, Error] A properly structured result
        def from_handler_output(output)
          case output
          when Success, Error
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
              error_code: nil,  # RetryableError doesn't have error_code
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