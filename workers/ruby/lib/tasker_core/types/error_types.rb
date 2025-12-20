# frozen_string_literal: true

module TaskerCore
  module Types
    # Standard error type constants for cross-language consistency.
    #
    # These error types are used across Ruby, Python, and Rust workers
    # to categorize step execution failures consistently.
    #
    # Note: Values use PascalCase to match StepHandlerCallResult::Error constraints.
    #
    # @example Using error types in a handler
    #   failure(
    #     message: "Invalid order total",
    #     error_type: TaskerCore::Types::ErrorTypes::VALIDATION_ERROR,
    #     retryable: false
    #   )
    #
    # @example Checking if an error type is valid
    #   TaskerCore::Types::ErrorTypes.valid?('ValidationError')  # => true
    #   TaskerCore::Types::ErrorTypes.valid?('unknown_type')     # => false
    module ErrorTypes
      # Permanent error that should not be retried
      PERMANENT_ERROR = 'PermanentError'

      # Retryable error that may succeed on subsequent attempts
      RETRYABLE_ERROR = 'RetryableError'

      # Validation error due to invalid input data
      VALIDATION_ERROR = 'ValidationError'

      # Unexpected error from unknown causes
      UNEXPECTED_ERROR = 'UnexpectedError'

      # Step completion error
      STEP_COMPLETION_ERROR = 'StepCompletionError'

      # All valid error types (matching StepHandlerCallResult::Error constraints)
      ALL = [
        PERMANENT_ERROR,
        RETRYABLE_ERROR,
        VALIDATION_ERROR,
        UNEXPECTED_ERROR,
        STEP_COMPLETION_ERROR
      ].freeze

      # Check if an error type is a recognized standard type
      #
      # @param error_type [String] The error type to check
      # @return [Boolean] True if the error type is recognized
      def self.valid?(error_type)
        ALL.include?(error_type)
      end

      # Check if an error type is typically retryable
      #
      # @param error_type [String] The error type to check
      # @return [Boolean] True if the error type typically allows retry
      def self.typically_retryable?(error_type)
        [RETRYABLE_ERROR, UNEXPECTED_ERROR].include?(error_type)
      end

      # Check if an error type is typically permanent (not retryable)
      #
      # @param error_type [String] The error type to check
      # @return [Boolean] True if the error type is typically permanent
      def self.typically_permanent?(error_type)
        [PERMANENT_ERROR, VALIDATION_ERROR].include?(error_type)
      end
    end
  end
end
