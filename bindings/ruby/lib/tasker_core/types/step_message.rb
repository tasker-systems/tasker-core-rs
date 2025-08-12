# frozen_string_literal: true

require 'dry-struct'
require 'dry-types'

module TaskerCore
  module Types
    # Define custom types for the system
    include Dry.Types()

    # Custom types for TaskerCore - Essential validation types
    Namespace = Types::String.constrained(filled: true)
    StepName = Types::String.constrained(filled: true)
    TaskName = Types::String # Allow empty strings for task names
    TaskVersion = Types::String.constrained(filled: true)
    Priority = Types::Integer.constrained(gteq: 1, lteq: 10)
    RetryCount = Types::Integer.constrained(gteq: 0)
    TimeoutMs = Types::Integer.constrained(gt: 0)

    # Minimal StepMessage stub for registry compatibility only
    # This is a lightweight replacement for the complex 324-line StepMessage class
    # Used only for temporary registry lookup objects in queue_worker.rb
    class StepMessage
      attr_reader :step_name, :namespace, :task_name, :task_version, :step_id, :task_uuid, :step_uuid

      def initialize(attrs = {})
        @step_name = attrs[:step_name]
        @namespace = attrs[:namespace] 
        @task_name = attrs[:task_name]
        @task_version = attrs[:task_version]
        @step_id = attrs[:step_id]
        @task_uuid = attrs[:task_uuid]
        @step_uuid = attrs[:step_uuid]
      end

      # Minimal interface for registry compatibility
      def execution_context
        OpenStruct.new(
          step: { 
            step_name: @step_name,
            workflow_step_id: @step_id,
            step_uuid: @step_uuid 
          },
          task: { 
            task_uuid: @task_uuid,
            namespace: @namespace,
            task_name: @task_name,
            task_version: @task_version
          }
        )
      end
    end

    # Simple queue message data for new UUID-based architecture
    # This is the core message structure for the simplified messaging system
    # Used extensively in pgmq_client.rb and queue_worker.rb
    class SimpleQueueMessageData < Dry::Struct
      transform_keys(&:to_sym)

      # pgmq envelope data
      attribute :msg_id, Types::Integer
      attribute :read_ct, Types::Integer
      # Accept either String or Time for timestamps (PostgreSQL may return either)
      attribute :enqueued_at, Types::String | Types::Instance(Time)
      attribute :vt, Types::String | Types::Instance(Time)

      # Simple message content (UUID-based)
      attribute :message, Types::Hash.schema(
        task_uuid: Types::String,
        step_uuid: Types::String,
        ready_dependency_step_uuids: Types::Array.of(Types::String)
      )

      # Convenience accessor for the simple message
      def step_message
        @step_message ||= SimpleStepMessage.new(message)
      end

      def queue_message
        @queue_message ||= OpenStruct.new(
          msg_id: msg_id,
          read_ct: read_ct,
          enqueued_at: enqueued_at,
          vt: vt,
          message: message
        )
      end
      
      # Delegate UUID accessors to the message hash for convenience
      def task_uuid
        message[:task_uuid]
      end
      
      def step_uuid
        message[:step_uuid]
      end
      
      def ready_dependency_step_uuids
        message[:ready_dependency_step_uuids]
      end
    end

    # SimpleStepMessage is defined in simple_message.rb with full UUID validation
    # This avoids duplicate class definitions
  end
end