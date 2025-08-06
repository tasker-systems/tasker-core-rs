# frozen_string_literal: true

require 'dry-struct'
require 'dry-types'

module TaskerCore
  module Types
    # Define custom types for the system
    include Dry.Types()

    # Custom types for TaskerCore
    StepId = Types::Integer.constrained(gteq: 0) # Allow 0 for synthetic/dependency steps
    TaskId = Types::Integer.constrained(gt: 0)
    Namespace = Types::String.constrained(filled: true)
    StepName = Types::String.constrained(filled: true)
    TaskName = Types::String # Allow empty strings for task names
    TaskVersion = Types::String.constrained(filled: true)
    Priority = Types::Integer.constrained(gteq: 1, lteq: 10)
    RetryCount = Types::Integer.constrained(gteq: 0)
    TimeoutMs = Types::Integer.constrained(gt: 0)

    # Result data from a dependency step that this step depends on
    class StepDependencyResult < Dry::Struct
      transform_keys(&:to_sym)

      attribute :step_name, Types::String
      attribute :step_id, Types::StepId
      attribute :named_step_id, Types::Integer
      attribute :results, Types::Hash.optional.default(nil)
      attribute :processed_at, Types::Constructor(Time) { |value|
        if value.nil?
          nil
        else
          (value.is_a?(Time) ? value : Time.parse(value.to_s))
        end
      }.optional.default(nil)
      attribute :metadata, Types::Hash.default({}.freeze)

      # Add metadata to the dependency result
      # @param key [String] metadata key
      # @param value [Object] metadata value
      # @return [StepDependencyResult] new instance with added metadata
      def with_metadata(key, value)
        new(metadata: metadata.merge(key => value))
      end
    end

    # Wrapper class for convenient access to dependency results
    class DependencyChain
      def initialize(dependencies)
        @dependencies = dependencies.map do |dep|
          if dep.is_a?(Hash)
            StepDependencyResult.new(dep)
          else
            dep
          end
        end
        @index = @dependencies.each_with_object({}) { |dep, hash| hash[dep.step_name] = dep }
      end

      # Get dependency result by step name
      # @param step_name [String] name of the dependency step
      # @return [StepDependencyResult, nil] dependency result or nil if not found
      def get(step_name)
        @index[step_name.to_s]
      end
      alias [] get

      # Get all dependencies
      # @return [Array<StepDependencyResult>] all dependency results
      def all
        @dependencies
      end

      # Get dependency names
      # @return [Array<String>] names of all dependencies
      def names
        @dependencies.map(&:step_name)
      end

      # Check if dependency exists
      # @param step_name [String] name of the dependency step
      # @return [Boolean] true if dependency exists
      def has?(step_name)
        @index.key?(step_name.to_s)
      end
      alias include? has?

      # Get number of dependencies
      # @return [Integer] number of dependencies
      def count
        @dependencies.count
      end
      alias size count
      alias length count

      # Check if any dependencies
      # @return [Boolean] true if no dependencies
      def empty?
        @dependencies.empty?
      end

      # Iterate over dependencies
      def each(&)
        @dependencies.each(&)
      end

      # Convert to array for compatibility
      # @return [Array<StepDependencyResult>] array of dependencies
      def to_a
        @dependencies
      end

      # Convert to hash representation
      # @return [Hash] hash representation
      def to_h
        {
          dependencies: @dependencies.map(&:to_h),
          count: count
        }
      end
    end

    # Execution context that provides (task, sequence, step) to handlers
    class StepExecutionContext < Dry::Struct
      transform_keys(&:to_sym)

      attribute :task, Types::Hash
      attribute :sequence, Types::Array.of(StepDependencyResult).default([].freeze)
      attribute :step, Types::Hash
      attribute :additional_context, Types::Hash.default({}.freeze)

      # Get dependency chain wrapper for convenient access
      # @return [DependencyChain] wrapper for accessing dependencies
      def dependencies
        @dependencies ||= DependencyChain.new(sequence)
      end

      # Add additional context
      # @param key [String] context key
      # @param value [Object] context value
      # @return [StepExecutionContext] new instance with added context
      def with_context(key, value)
        new(additional_context: additional_context.merge(key => value))
      end

      # Create execution context with empty dependencies (for root steps)
      # @param task [Hash] task data
      # @param step [Hash] step data
      # @return [StepExecutionContext] new execution context
      def self.new_root_step(task:, step:)
        new(
          task: task,
          sequence: [],
          step: step
        )
      end
    end

    # Step message metadata for queue processing
    class StepMessageMetadata < Dry::Struct
      # Make the struct more flexible for additional attributes
      transform_keys(&:to_sym)

      attribute(:created_at, Types::Time.default { Time.now.utc })
      attribute :retry_count, Types::RetryCount.default(0)
      attribute :max_retries, Types::RetryCount.default(3)
      attribute :timeout_ms, Types::TimeoutMs.default(30_000) # 30 seconds
      attribute(:correlation_id, Types::String.optional.default { SecureRandom.uuid })
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

      # Execution context for (task, sequence, step) pattern
      attribute(:execution_context, StepExecutionContext.default do
        StepExecutionContext.new(task: {}, sequence: [], step: {})
      end)

      # Message metadata
      attribute(:metadata, StepMessageMetadata.default { StepMessageMetadata.new })

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
          execution_context: execution_context.to_h,
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
          metadata_hash = symbolized[:metadata].transform_keys(&:to_sym)

          # Convert created_at string back to Time object if needed
          if metadata_hash[:created_at].is_a?(String)
            metadata_hash[:created_at] = Time.parse(metadata_hash[:created_at])
          end

          symbolized[:metadata] = StepMessageMetadata.new(metadata_hash)
        end

        if symbolized[:execution_context].is_a?(Hash)
          symbolized[:execution_context] = StepExecutionContext.new(symbolized[:execution_context])
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
      # @param execution_context [StepExecutionContext] execution context
      # @return [StepMessage] new step message
      def self.build_test(step_id:, task_id:, namespace:, task_name:, step_name:,
                          step_payload: {}, task_version: '1.0.0', execution_context: nil)
        execution_context ||= StepExecutionContext.new_root_step(
          task: { task_id: task_id, namespace: namespace },
          step: { step_id: step_id, step_name: step_name }
        )

        new(
          step_id: step_id,
          task_id: task_id,
          namespace: namespace,
          task_name: task_name,
          task_version: task_version,
          step_name: step_name,
          step_payload: step_payload,
          execution_context: execution_context
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
      attribute(:completed_at, Types::Time.default { Time.now.utc })

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
      def self.success(step_id:, task_id:, execution_time_ms:, result_data: nil)
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

    # Queue message wrapper from pgmq
    # This represents the actual message structure returned by pgmq
    class QueueMessage < Dry::Struct
      transform_keys(&:to_sym)

      # Required fields from pgmq
      attribute :msg_id, Types::Integer.constrained(gt: 0)
      attribute :read_ct, Types::Integer.constrained(gteq: 0)
      attribute :enqueued_at, Types::Constructor(Time) { |value|
        value.is_a?(Time) ? value : Time.parse(value.to_s)
      }
      attribute :vt, Types::Constructor(Time) { |value|
        value.is_a?(Time) ? value : Time.parse(value.to_s)
      }
      attribute :message, Types::Hash

      # Convert raw pgmq hash to typed struct
      # @param hash [Hash] raw hash from pgmq
      # @return [QueueMessage] typed queue message
      def self.from_pgmq_hash(hash)
        # Deep symbolize keys
        symbolized = deep_symbolize_keys(hash)
        new(symbolized)
      end

      # Get the step message from the queue message payload
      # @return [StepMessage] parsed step message
      def step_message
        @step_message ||= StepMessage.from_hash(message)
      end

      # Deep symbolize keys helper
      def self.deep_symbolize_keys(hash)
        hash.each_with_object({}) do |(key, value), result|
          new_key = key.is_a?(String) ? key.to_sym : key
          result[new_key] = case value
                            when Hash then deep_symbolize_keys(value)
                            when Array then value.map { |v| v.is_a?(Hash) ? deep_symbolize_keys(v) : v }
                            else value
                            end
        end
      end
    end

    # Composite type for queue message with parsed step message
    # This is what queue workers receive after reading from pgmq
    class QueueMessageData < Dry::Struct
      attribute :queue_message, QueueMessage
      attribute :step_message, StepMessage

      # Create from raw pgmq result
      # @param pgmq_result [Hash] raw result from pgmq read operation
      # @return [QueueMessageData] typed message data
      def self.from_pgmq_result(pgmq_result)
        queue_msg = QueueMessage.from_pgmq_hash(pgmq_result)
        step_msg = queue_msg.step_message

        new(
          queue_message: queue_msg,
          step_message: step_msg
        )
      end

      # Convenience accessors
      def msg_id
        queue_message.msg_id
      end

      def message
        queue_message.message
      end
    end
  end
end
