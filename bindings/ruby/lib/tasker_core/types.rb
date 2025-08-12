# frozen_string_literal: true

require 'dry-struct'
require 'dry-types'

# Load all type modules
require_relative 'types/task_types'
require_relative 'types/step_types'
require_relative 'types/step_message'
require_relative 'types/simple_message' # NEW: simplified UUID-based messages
require_relative 'types/task_template' # TaskTemplate and StepTemplate types
require_relative 'types/step_handler_call_result' # NEW: standardized handler results

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
  # @example Using simple message types
  #   simple_message = TaskerCore::Types::SimpleStepMessage.new(
  #     task_uuid: "task-123",
  #     step_uuid: "step-456",
  #     ready_dependency_step_uuids: ["dep-1", "dep-2"]
  #   )
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

    # Simple message types (UUID-based) - already defined in the namespace
  end
end
