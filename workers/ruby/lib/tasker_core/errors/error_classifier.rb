# frozen_string_literal: true

require_relative 'common'

module TaskerCore
  module Errors
    # ErrorClassifier provides systematic classification of errors for retry logic
    #
    # This classifier determines whether errors should be retried based on their type.
    # It maintains explicit lists of permanent (non-retryable) and retryable error classes.
    #
    # @example Classify a configuration error
    #   ErrorClassifier.retryable?(ConfigurationError.new("Missing handler"))
    #   # => false
    #
    # @example Classify a network error
    #   ErrorClassifier.retryable?(NetworkError.new("Connection timeout"))
    #   # => true
    class ErrorClassifier
      # System-level errors that should NEVER be retried
      # These represent fundamental configuration or setup issues that won't resolve on retry
      PERMANENT_ERROR_CLASSES = [
        ConfigurationError,      # Configuration issues (missing config, invalid settings)
        PermanentError,          # Explicitly marked permanent errors
        ValidationError,         # Data validation failures
        NotFoundError,           # Resource not found (handler, step, etc.)
        DatabaseError,           # Database schema/constraint errors
        FFIError                 # FFI bridge failures
      ].freeze

      # Application errors that are retryable by default
      # These represent transient failures that may resolve on subsequent attempts
      RETRYABLE_ERROR_CLASSES = [
        RetryableError,          # Explicitly marked retryable errors
        NetworkError,            # Network/connection failures
        TimeoutError             # Timeout errors
      ].freeze

      # Determine if an error should be retried
      #
      # @param error [StandardError] The error to classify
      # @return [Boolean] true if the error should be retried, false otherwise
      #
      # @note The default behavior is to mark errors as retryable unless they are
      #   explicitly in the PERMANENT_ERROR_CLASSES list. This follows the principle
      #   that it's safer to retry an error that shouldn't be retried (will eventually
      #   hit retry limit) than to not retry an error that should be (will fail immediately).
      def self.retryable?(error)
        # Explicit permanent errors - never retry
        return false if PERMANENT_ERROR_CLASSES.any? { |klass| error.is_a?(klass) }

        # Explicit retryable errors - always retry
        return true if RETRYABLE_ERROR_CLASSES.any? { |klass| error.is_a?(klass) }

        # Default: StandardError and subclasses are retryable unless explicitly marked permanent
        # This is a safe default - worst case, we retry until max_attempts is hit
        true
      end
    end
  end
end
