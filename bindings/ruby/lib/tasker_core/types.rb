# frozen_string_literal: true

require 'dry-struct'
require 'dry-types'

# Load all type modules
require_relative 'types/task_types'
require_relative 'types/step_types'
require_relative 'types/orchestration_types'
require_relative 'types/execution_types'
require_relative 'types/step_message'  # NEW: pgmq message types
require_relative 'types/task_template'  # TaskTemplate and StepTemplate types

module TaskerCore
  # Centralized type definitions for TaskerCore domain objects
  # 
  # This module serves as the main entry point for all TaskerCore types,
  # organizing them into logical groupings for better maintainability.
  #
  # @example Using task types
  #   request = TaskerCore::Types::TaskRequest.build_test(
  #     namespace: "fulfillment",
  #     name: "process_order", 
  #     context: { order_id: "123" }
  #   )
  #
  # @example Using orchestration types  
  #   task = TaskerCore::Types::OrchestrationTypes::StructFactory.create_task(step_data)
  #   sequence = TaskerCore::Types::OrchestrationTypes::StructFactory.create_sequence(step_data)
  #   step = TaskerCore::Types::OrchestrationTypes::StructFactory.create_step(step_data)
  #
  module Types
    include Dry.Types()

    # Make base types available in class scope
    Types = Dry.Types()

    # Re-export all type classes for backward compatibility and convenience
    
    # Task-related types
    TaskRequest = TaskTypes::TaskRequest
    TaskResponse = TaskTypes::TaskResponse
    
    # Step-related types  
    StepCompletion = StepTypes::StepCompletion
    
    # Orchestration-related types
    TaskStruct = OrchestrationTypes::TaskStruct
    SequenceStruct = OrchestrationTypes::SequenceStruct
    StepStruct = OrchestrationTypes::StepStruct
    
    # Factory methods for orchestration types
    StructFactory = OrchestrationTypes::StructFactory
    
    # Execution-related types (TCP executor responses)
    HealthCheckResponse = ExecutionTypes::HealthCheckResponse
    WorkerRegistrationResponse = ExecutionTypes::WorkerRegistrationResponse
    HeartbeatResponse = ExecutionTypes::HeartbeatResponse
    WorkerUnregistrationResponse = ExecutionTypes::WorkerUnregistrationResponse
    ErrorResponse = ExecutionTypes::ErrorResponse
    ResponseFactory = ExecutionTypes::ResponseFactory
  end
end