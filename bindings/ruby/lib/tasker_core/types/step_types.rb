# frozen_string_literal: true

require 'dry-struct'
require 'dry-types'

module TaskerCore
  module Types
    # Step-related type definitions for workflow execution
    module StepTypes
      module Types
        include Dry.Types()
      end

      # Step execution status enum
      class StepExecutionStatus < Dry::Struct
        attribute :status, Types::String.enum('success', 'failed', 'cancelled', 'timeout')
        
        def success?
          status == 'success'
        end
        
        def failed?
          status == 'failed'
        end
      end

      # Step execution error details
      class StepExecutionError < Dry::Struct
        attribute :error_type, Types::String.enum('HandlerNotFound', 'HandlerException', 'ProcessingError', 'MaxRetriesExceeded')
        attribute :message, Types::String
        attribute :retryable, Types::Bool.default(true)
        attribute? :error_code, Types::String.optional
        attribute? :stack_trace, Types::String.optional
        
        def to_h
          {
            error_type: error_type,
            message: message,
            retryable: retryable,
            error_code: error_code,
            stack_trace: stack_trace
          }.compact
        end
      end

      # Step execution result from pgmq worker processing
      class StepResult < Dry::Struct
        attribute :step_id, Types::Integer
        attribute :task_id, Types::Integer
        attribute :status, StepExecutionStatus
        attribute :execution_time_ms, Types::Integer
        attribute :completed_at, Types::Constructor(Time) { |value| value.is_a?(Time) ? value : Time.parse(value.to_s) }
        attribute? :result_data, Types::Any.optional
        attribute? :error, StepExecutionError.optional
        attribute? :orchestration_metadata, Types::Hash.optional

        # Factory methods for creating results
        def self.success(step_id:, task_id:, result_data: nil, execution_time_ms: 0)
          new(
            step_id: step_id,
            task_id: task_id,
            status: StepExecutionStatus.new(status: 'success'),
            execution_time_ms: execution_time_ms,
            completed_at: Time.now,
            result_data: result_data
          )
        end

        def self.failure(step_id:, task_id:, error:, execution_time_ms: 0)
          new(
            step_id: step_id,
            task_id: task_id,
            status: StepExecutionStatus.new(status: 'failed'),
            execution_time_ms: execution_time_ms,
            completed_at: Time.now,
            error: error
          )
        end

        # Convenience methods
        def success?
          status.success?
        end

        def failed?
          status.failed?
        end

        # Convert to hash for message serialization
        def to_h
          {
            step_id: step_id,
            task_id: task_id,
            status: status.status,
            success: success?,
            result_data: result_data,
            error: error&.to_h,
            execution_time_ms: execution_time_ms,
            completed_at: completed_at.iso8601,
            orchestration_metadata: orchestration_metadata
          }.compact
        end
      end

      # Step completion struct for workflow step results
      class StepCompletion < Dry::Struct
        attribute :step_name, Types::Coercible::String
        attribute :status, Types::Coercible::String.enum('complete', 'failed', 'pending')
        attribute :results, Types::Hash.default({}.freeze)
        attribute :duration_ms, Types::Integer.optional
        attribute :completed_at, Types::Constructor(Time).optional
        attribute :error_message, Types::Coercible::String.optional

        # Validation for step completion data
        def valid?
          !step_name.empty? && 
          %w[complete failed pending].include?(status) &&
          results.is_a?(Hash)
        end

        # Check if step completed successfully
        def completed?
          status == 'complete'
        end

        # Check if step failed
        def failed?
          status == 'failed'
        end

        # Check if step is still pending
        def pending?
          status == 'pending'
        end

        # Get execution duration in seconds
        def duration_seconds
          return nil unless duration_ms
          duration_ms / 1000.0
        end

        def to_s
          "#<StepCompletion #{step_name} status=#{status} duration=#{duration_seconds}s>"
        end
      end
    end
  end
end