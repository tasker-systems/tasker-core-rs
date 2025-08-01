# frozen_string_literal: true

require 'dry-struct'
require 'dry-types'

module TaskerCore
  module Types
    # Define custom types for the system
    module Types
      include Dry.Types()

      # Custom types for TaskerCore
      StepId = Types::Integer.constrained(gt: 0)
      TaskId = Types::Integer.constrained(gt: 0)
      Namespace = Types::String.constrained(filled: true)
      StepName = Types::String.constrained(filled: true)
      TaskName = Types::String.constrained(filled: true)
      TaskVersion = Types::String.constrained(filled: true)
      Priority = Types::Integer.constrained(gteq: 1, lteq: 10)
      RetryCount = Types::Integer.constrained(gteq: 0)
      TimeoutMs = Types::Integer.constrained(gt: 0)
    end

    # Step message metadata for queue processing
    class StepMessageMetadata < Dry::Struct
      # Make the struct more flexible for additional attributes
      transform_keys(&:to_sym)

      attribute :created_at, Types::Time.default { Time.now.utc }
      attribute :retry_count, Types::RetryCount.default(0)
      attribute :max_retries, Types::RetryCount.default(3)
      attribute :timeout_ms, Types::TimeoutMs.default(30_000) # 30 seconds
      attribute :correlation_id, Types::String.optional.default { SecureRandom.uuid }
      attribute :priority, Types::Priority.default(5) # Normal priority
      attribute :context, Types::Hash.default({}.freeze)

      # Check if message has exceeded max retries
      # @return [Boolean] true if retry count >= max retries
      def max_retries_exceeded?
        retry_count >= max_retries
      end

      # Check if message has timed out
      # @return [Boolean] true if message age exceeds timeout
      def expired?
        age_ms > timeout_ms
      end

      # Get message age in milliseconds
      # @return [Integer] age in milliseconds
      def age_ms
        ((Time.now.utc - created_at) * 1000).to_i
      end

      # Increment retry count (returns new instance)
      # @return [StepMessageMetadata] new instance with incremented retry count
      def increment_retry
        new(retry_count: retry_count + 1)
      end
    end

    # Message for step execution via pgmq queues
    # This mirrors the Rust StepMessage structure for compatibility
    class StepMessage < Dry::Struct
      # Make the struct more flexible for additional attributes
      transform_keys(&:to_sym)

      # Core step identification
      attribute :step_id, Types::StepId
      attribute :task_id, Types::TaskId
      
      # Task context
      attribute :namespace, Types::Namespace
      attribute :task_name, Types::TaskName
      attribute :task_version, Types::TaskVersion
      attribute :step_name, Types::StepName
      
      # Step execution data
      attribute :step_payload, Types::Hash.default({}.freeze)
      
      # Message metadata
      attribute :metadata, StepMessageMetadata.default { StepMessageMetadata.new }

      # Get the queue name for this message based on namespace
      # @return [String] queue name (e.g., "fulfillment_queue")
      def queue_name
        "#{namespace}_queue"
      end

      # Convert to hash for JSON serialization
      # @return [Hash] hash representation suitable for JSON
      def to_h
        {
          step_id: step_id,
          task_id: task_id,
          namespace: namespace,
          task_name: task_name,
          task_version: task_version,
          step_name: step_name,
          step_payload: step_payload,
          metadata: metadata.to_h
        }
      end

      # Create from hash (for deserialization from JSON)
      # @param hash [Hash] hash representation
      # @return [StepMessage] new step message instance
      def self.from_hash(hash)
        # Convert string keys to symbols and handle nested metadata
        symbolized = hash.transform_keys(&:to_sym)
        
        if symbolized[:metadata].is_a?(Hash)
          symbolized[:metadata] = StepMessageMetadata.new(symbolized[:metadata])
        end
        
        new(symbolized)
      end

      # Check if message has exceeded max retries
      # @return [Boolean] true if retry count >= max retries
      def max_retries_exceeded?
        metadata.max_retries_exceeded?
      end

      # Check if message has timed out
      # @return [Boolean] true if message age exceeds timeout
      def expired?
        metadata.expired?
      end

      # Get message age in milliseconds
      # @return [Integer] age in milliseconds
      def age_ms
        metadata.age_ms
      end

      # Increment retry count (returns new instance)
      # @return [StepMessage] new instance with incremented retry count
      def increment_retry
        new(metadata: metadata.increment_retry)
      end

      # Create a step message for testing
      # @param step_id [Integer] step ID
      # @param task_id [Integer] task ID
      # @param namespace [String] namespace
      # @param task_name [String] task name
      # @param step_name [String] step name
      # @param step_payload [Hash] step payload
      # @param task_version [String] task version
      # @return [StepMessage] new step message
      def self.build_test(step_id:, task_id:, namespace:, task_name:, step_name:, 
                         step_payload: {}, task_version: "1.0.0")
        new(
          step_id: step_id,
          task_id: task_id,
          namespace: namespace,
          task_name: task_name,
          task_version: task_version,
          step_name: step_name,
          step_payload: step_payload
        )
      end
    end

    # Step execution status for result tracking
    class StepExecutionStatus < Dry::Struct
      # Define allowed status values
      VALID_STATUSES = %w[success failed cancelled timed_out retried].freeze

      transform_keys(&:to_sym)

      attribute :status, Types::String.enum(*VALID_STATUSES)
      
      # Convenience methods for status checking
      def success?
        status == 'success'
      end
      
      def failed?
        status == 'failed'
      end
      
      def cancelled?
        status == 'cancelled'
      end
      
      def timed_out?
        status == 'timed_out'
      end
      
      def retried?
        status == 'retried'
      end
    end

    # Error information for failed steps
    class StepExecutionError < Dry::Struct
      transform_keys(&:to_sym)

      attribute :error_type, Types::String
      attribute :message, Types::String
      attribute :retryable, Types::Bool.default(false)
      attribute :details, Types::Hash.optional.default(nil)
      attribute :stack_trace, Types::String.optional.default(nil)
    end

    # Result of step execution for completion tracking
    class StepResult < Dry::Struct
      transform_keys(&:to_sym)

      # Step identification
      attribute :step_id, Types::StepId
      attribute :task_id, Types::TaskId
      
      # Execution results
      attribute :status, StepExecutionStatus
      attribute :result_data, Types::Hash.optional.default(nil)
      attribute :error, StepExecutionError.optional.default(nil)
      
      # Timing information
      attribute :execution_time_ms, Types::Integer.constrained(gteq: 0)
      attribute :completed_at, Types::Time.default { Time.now.utc }

      # Convenience methods
      def success?
        status.success?
      end

      def failed?
        status.failed?
      end

      def retryable_error?
        error&.retryable == true
      end

      # Create successful step result
      # @param step_id [Integer] step ID
      # @param task_id [Integer] task ID
      # @param result_data [Hash] result data
      # @param execution_time_ms [Integer] execution time in milliseconds
      # @return [StepResult] successful result
      def self.success(step_id:, task_id:, result_data: nil, execution_time_ms:)
        new(
          step_id: step_id,
          task_id: task_id,
          status: StepExecutionStatus.new(status: 'success'),
          result_data: result_data,
          execution_time_ms: execution_time_ms
        )
      end

      # Create failed step result
      # @param step_id [Integer] step ID
      # @param task_id [Integer] task ID
      # @param error [StepExecutionError] error information
      # @param execution_time_ms [Integer] execution time in milliseconds
      # @return [StepResult] failed result
      def self.failure(step_id:, task_id:, error:, execution_time_ms:)
        new(
          step_id: step_id,
          task_id: task_id,
          status: StepExecutionStatus.new(status: 'failed'),
          error: error,
          execution_time_ms: execution_time_ms
        )
      end

      # Create timed out step result
      # @param step_id [Integer] step ID
      # @param task_id [Integer] task ID
      # @param execution_time_ms [Integer] execution time in milliseconds
      # @return [StepResult] timed out result
      def self.timeout(step_id:, task_id:, execution_time_ms:)
        error = StepExecutionError.new(
          error_type: 'TimeoutError',
          message: 'Step execution timed out',
          retryable: true
        )
        
        new(
          step_id: step_id,
          task_id: task_id,
          status: StepExecutionStatus.new(status: 'timed_out'),
          error: error,
          execution_time_ms: execution_time_ms
        )
      end
    end
  end
end