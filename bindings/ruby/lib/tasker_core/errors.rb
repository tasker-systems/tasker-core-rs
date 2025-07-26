# frozen_string_literal: true

module TaskerCore
  module Errors
    # Base error class for all TaskerCore-related errors
    class Error < StandardError; end

    # Raised when there are configuration-related issues in TaskerCore
    class ConfigurationError < Error; end

    # Base class for all TaskerCore-specific errors that occur during workflow execution
    # Maps to the comprehensive error system in Rust orchestration/errors.rs
    class ProceduralError < Error; end

    # Raised when there are orchestration-related issues in TaskerCore
    class OrchestrationError < Error; end

    # Error indicating a step failed but should be retried with backoff
    # Maps to Rust StepExecutionError::Retryable
    #
    # Use this error when an operation fails due to temporary conditions like:
    # - Network timeouts
    # - Rate limiting (429 status)
    # - Server errors (5xx status)
    # - Temporary service unavailability
    # - Resource exhaustion that may resolve
    #
    # @example Basic retryable error
    #   raise TaskerCore::Errors::RetryableError, "Payment service timeout"
    #
    # @example With retry delay
    #   raise TaskerCore::Errors::RetryableError.new("Rate limited", retry_after: 60)
    #
    # @example With context for monitoring
    #   raise TaskerCore::Errors::RetryableError.new(
    #     "External API unavailable",
    #     retry_after: 30,
    #     context: { service: 'billing_api', error_code: 503 }
    #   )
    class RetryableError < ProceduralError
      # @return [Integer, nil] Suggested retry delay in seconds
      attr_reader :retry_after

      # @return [Hash] Additional context for error monitoring and debugging
      attr_reader :context

      # @param message [String] Error message
      # @param retry_after [Integer, nil] Suggested retry delay in seconds
      # @param context [Hash] Additional context for monitoring
      def initialize(message, retry_after: nil, context: {})
        super(message)
        @retry_after = retry_after
        @context = context
      end

      # Get the error class name for Rust FFI compatibility
      def error_class
        'RetryableError'
      end
    end

    # Error indicating a step failed permanently and should not be retried
    # Maps to Rust StepExecutionError::Permanent
    #
    # Use this error when an operation fails due to permanent conditions like:
    # - Invalid request data (400 status)
    # - Authentication/authorization failures (401/403 status)
    # - Validation errors (422 status)
    # - Resource not found when it should exist (404 status in some contexts)
    # - Business logic violations
    # - Configuration errors
    #
    # @example Basic permanent error
    #   raise TaskerCore::Errors::PermanentError, "Invalid user ID format"
    #
    # @example With error code for categorization
    #   raise TaskerCore::Errors::PermanentError.new(
    #     "Insufficient funds for transaction",
    #     error_code: 'INSUFFICIENT_FUNDS'
    #   )
    #
    # @example With context for monitoring
    #   raise TaskerCore::Errors::PermanentError.new(
    #     "User not authorized for this operation",
    #     error_code: 'AUTHORIZATION_FAILED',
    #     context: { user_id: 123, operation: 'admin_access' }
    #   )
    class PermanentError < ProceduralError
      # @return [String, nil] Machine-readable error code for categorization
      attr_reader :error_code

      # @return [Hash] Additional context for error monitoring and debugging
      attr_reader :context

      # @param message [String] Error message
      # @param error_code [String, nil] Machine-readable error code
      # @param context [Hash] Additional context for monitoring
      def initialize(message, error_code: nil, context: {})
        super(message)
        @error_code = error_code
        @context = context
      end

      # Get the error class name for Rust FFI compatibility
      def error_class
        'PermanentError'
      end
    end

    # Error indicating a timeout occurred during step execution
    # Maps to Rust StepExecutionError::Timeout
    #
    # Use this error when an operation fails due to timeout conditions:
    # - Step execution timeout
    # - Network request timeout
    # - Database operation timeout
    # - External service timeout
    #
    # @example Basic timeout error
    #   raise TaskerCore::Errors::TimeoutError, "Payment processing timed out"
    #
    # @example With timeout duration
    #   raise TaskerCore::Errors::TimeoutError.new(
    #     "Database query timed out",
    #     timeout_duration: 30
    #   )
    class TimeoutError < ProceduralError
      # @return [Integer, nil] Timeout duration in seconds
      attr_reader :timeout_duration

      # @return [Hash] Additional context for error monitoring and debugging
      attr_reader :context

      # @param message [String] Error message
      # @param timeout_duration [Integer, nil] Timeout duration in seconds
      # @param context [Hash] Additional context for monitoring
      def initialize(message, timeout_duration: nil, context: {})
        super(message)
        @timeout_duration = timeout_duration
        @context = context
      end

      # Get the error class name for Rust FFI compatibility
      def error_class
        'TimeoutError'
      end
    end

    # Error indicating a network-related failure
    # Maps to Rust StepExecutionError::NetworkError
    #
    # Use this error when an operation fails due to network conditions:
    # - Connection failures
    # - DNS resolution errors
    # - HTTP errors (4xx/5xx when network-related)
    # - TLS/SSL errors
    # - Network timeouts
    #
    # @example Basic network error
    #   raise TaskerCore::Errors::NetworkError, "Connection refused"
    #
    # @example With HTTP status code
    #   raise TaskerCore::Errors::NetworkError.new(
    #     "Service unavailable",
    #     status_code: 503
    #   )
    class NetworkError < ProceduralError
      # @return [Integer, nil] HTTP status code if applicable
      attr_reader :status_code

      # @return [Hash] Additional context for error monitoring and debugging
      attr_reader :context

      # @param message [String] Error message
      # @param status_code [Integer, nil] HTTP status code if applicable
      # @param context [Hash] Additional context for monitoring
      def initialize(message, status_code: nil, context: {})
        super(message)
        @status_code = status_code
        @context = context
      end

      # Get the error class name for Rust FFI compatibility
      def error_class
        'NetworkError'
      end
    end

    # Error indicating validation failed
    # Maps to Rust OrchestrationError::ValidationError
    #
    # Use this error when data validation fails:
    # - Schema validation errors
    # - Business rule validation
    # - Input format validation
    # - Required field validation
    class ValidationError < PermanentError
      # @return [String, nil] Field that failed validation
      attr_reader :field

      # @param message [String] Error message
      # @param field [String, nil] Field that failed validation
      # @param error_code [String, nil] Machine-readable error code
      # @param context [Hash] Additional context for monitoring
      def initialize(message, field: nil, error_code: nil, context: {})
        super(message, error_code: error_code, context: context)
        @field = field
      end

      # Get the error class name for Rust FFI compatibility
      def error_class
        'ValidationError'
      end
    end

    # Error indicating a handler or step was not found
    # Maps to Rust OrchestrationError::HandlerNotFound and StepHandlerNotFound
    class NotFoundError < PermanentError
      # @return [String, nil] Type of resource not found (handler, step, etc.)
      attr_reader :resource_type

      # @param message [String] Error message
      # @param resource_type [String, nil] Type of resource not found
      # @param error_code [String, nil] Machine-readable error code
      # @param context [Hash] Additional context for monitoring
      def initialize(message, resource_type: nil, error_code: nil, context: {})
        super(message, error_code: error_code, context: context)
        @resource_type = resource_type
      end

      # Get the error class name for Rust FFI compatibility
      def error_class
        'NotFoundError'
      end
    end

    # Error indicating an FFI operation failed
    # Maps to Rust OrchestrationError::FfiBridgeError
    class FFIError < Error
      # @return [String, nil] FFI operation that failed
      attr_reader :operation

      # @return [Hash] Additional context for error monitoring and debugging
      attr_reader :context

      # @param message [String] Error message
      # @param operation [String, nil] FFI operation that failed
      # @param context [Hash] Additional context for monitoring
      def initialize(message, operation: nil, context: {})
        super(message)
        @operation = operation
        @context = context
      end

      # Get the error class name for Rust FFI compatibility
      def error_class
        'FFIError'
      end
    end
  end
end
