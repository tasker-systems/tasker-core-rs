# frozen_string_literal: true

require 'dry-struct'
require 'dry-types'

module TaskerCore
  module Types
    # Orchestration-specific types for batch step execution
    module OrchestrationTypes
      module Types
        include Dry.Types()
      end

      # Represents a task with its context and metadata for step execution
      class TaskStruct < Dry::Struct
        attribute :task_id, Types::Integer
        attribute :context, Types::Hash
        attribute :metadata, Types::Hash.optional.default({}.freeze)

        # Validation for required task data
        def valid?
          task_id.is_a?(Integer) && task_id > 0 && context.is_a?(Hash)
        end

        def to_s
          "#<TaskStruct task_id=#{task_id} context_keys=#{context.keys.sort}>"
        end
      end

      # Represents step sequence information for coordination across workers
      class SequenceStruct < Dry::Struct
        attribute :sequence_number, Types::Integer
        attribute :total_steps, Types::Integer
        attribute :previous_results, Types::Hash

        # Validation for sequence coordination
        def valid?
          sequence_number.is_a?(Integer) && sequence_number > 0 &&
          total_steps.is_a?(Integer) && total_steps > 0 &&
          sequence_number <= total_steps &&
          previous_results.is_a?(Hash)
        end

        def to_s
          "#<SequenceStruct #{sequence_number}/#{total_steps} prev_results=#{previous_results.keys.size}>"
        end
      end

      # Represents an individual step to be executed by a worker
      # Note: retry_limit removed as retries are handled by Rust TaskFinalizer
      class StepStruct < Dry::Struct
        attribute :step_id, Types::Integer
        attribute :step_name, Types::String
        attribute :handler_config, Types::Hash.default({}.freeze)
        attribute :timeout_ms, Types::Integer.optional

        # Validation for step execution requirements
        def valid?
          step_id.is_a?(Integer) && step_id > 0 &&
          step_name.is_a?(String) && !step_name.empty? &&
          handler_config.is_a?(Hash)
        end

        # Extract timeout in seconds for execution
        def timeout_seconds
          return 30.0 unless timeout_ms # Default 30 second timeout
          timeout_ms / 1000.0
        end

        def to_s
          "#<StepStruct #{step_name} step_id=#{step_id} timeout=#{timeout_seconds}s>"
        end
      end

      # Factory methods for creating structs with validation
      module StructFactory
        # Create TaskStruct with validation and error handling
        def self.create_task(step_data)
          task_id = step_data[:task_id] || step_data['task_id']
          context = step_data[:task_context] || step_data['task_context']
          metadata = step_data[:metadata] || step_data['metadata'] || {}

          raise ArgumentError, "task_id is required and must be positive integer" unless task_id.is_a?(Integer) && task_id > 0
          raise ArgumentError, "task_context is required and must be a hash" unless context.is_a?(Hash)

          TaskStruct.new(
            task_id: task_id,
            context: context,
            metadata: metadata
          )
        end

        # Create SequenceStruct with validation and error handling  
        def self.create_sequence(step_data)
          sequence = step_data.dig(:metadata, :sequence) || step_data.dig('metadata', 'sequence')
          total_steps = step_data.dig(:metadata, :total_steps) || step_data.dig('metadata', 'total_steps')
          previous_results = step_data[:previous_results] || step_data['previous_results']

          raise ArgumentError, "sequence number is required" unless sequence.is_a?(Integer) && sequence > 0
          raise ArgumentError, "total_steps is required" unless total_steps.is_a?(Integer) && total_steps > 0
          raise ArgumentError, "previous_results is required and must be a hash" unless previous_results.is_a?(Hash)

          SequenceStruct.new(
            sequence_number: sequence,
            total_steps: total_steps,
            previous_results: previous_results
          )
        end

        # Create StepStruct with validation and error handling
        def self.create_step(step_data)
          step_id = step_data[:step_id] || step_data['step_id']
          step_name = step_data[:step_name] || step_data['step_name'] 
          handler_config = step_data[:handler_config] || step_data['handler_config'] || {}
          timeout_ms = step_data.dig(:metadata, :timeout_ms) || step_data.dig('metadata', 'timeout_ms')

          raise ArgumentError, "step_id is required and must be positive integer" unless step_id.is_a?(Integer) && step_id > 0
          raise ArgumentError, "step_name is required and must be non-empty string" unless step_name.is_a?(String) && !step_name.empty?

          StepStruct.new(
            step_id: step_id,
            step_name: step_name,
            handler_config: handler_config,
            timeout_ms: timeout_ms
          )
        end
      end
    end
  end
end