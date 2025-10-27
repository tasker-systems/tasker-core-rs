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
require_relative 'types/decision_point_outcome' # TAS-53: decision point outcomes

module TaskerCore
  # Centralized type definitions for TaskerCore domain objects
  #
  # This module serves as the main entry point for all TaskerCore types,
  # organizing them into logical groupings for better maintainability.
  # All types are powered by dry-types for automatic validation, coercion,
  # and schema enforcement.
  #
  # Type Categories:
  # - **Task Types**: TaskRequest, TaskResponse for task creation and responses
  # - **Step Types**: StepCompletion for step execution results
  # - **Message Types**: SimpleStepMessage, SimpleTaskMessage for lightweight messaging
  # - **Template Types**: TaskTemplate, StepTemplate for workflow definitions
  # - **Result Types**: StepHandlerCallResult for standardized handler responses
  #
  # @example Using task types with validation
  #   # Build a task request with automatic validation
  #   request = TaskerCore::Types::TaskRequest.new(
  #     namespace: "fulfillment",
  #     name: "process_order",
  #     context: { order_id: "123", items: [...] }
  #   )
  #   # => Automatically validates required fields and types
  #
  # @example Type coercion and defaults
  #   # dry-types automatically coerces compatible types
  #   completion = TaskerCore::Types::StepCompletion.new(
  #     task_uuid: "123",
  #     step_uuid: "456",
  #     success: "true"  # Automatically coerced to boolean true
  #   )
  #
  # @example Building test fixtures
  #   # Use build_test for quick test data generation
  #   request = TaskerCore::Types::TaskRequest.build_test(
  #     namespace: "fulfillment",
  #     name: "process_order",
  #     context: { order_id: "123" }
  #   )
  #   # => Creates valid request with sensible test defaults
  #
  # @example Using simple message types for UUID-based communication
  #   # Lightweight message for step execution
  #   simple_message = TaskerCore::Types::SimpleStepMessage.new(
  #     task_uuid: "550e8400-e29b-41d4-a716-446655440000",
  #     step_uuid: "7c9e6679-7425-40de-944b-e07fc1f90ae7",
  #     ready_dependency_step_uuids: [
  #       "123e4567-e89b-12d3-a456-426614174000",
  #       "456e7890-e12b-34d5-a678-426614174001"
  #     ]
  #   )
  #
  # @example Using template types for workflow definitions
  #   # Define a task template
  #   template = TaskerCore::Types::TaskTemplate.new(
  #     namespace: "payments",
  #     name: "process_payment",
  #     steps: [
  #       { name: "validate", handler_class: "ValidatePaymentHandler" },
  #       { name: "charge", handler_class: "ChargePaymentHandler" }
  #     ]
  #   )
  #
  # @example Standardized handler results
  #   # Return structured results from handlers
  #   result = TaskerCore::Types::StepHandlerCallResult.success(
  #     result: { payment_id: "pay_123", amount: 100.00 },
  #     metadata: {
  #       processing_time_ms: 125,
  #       gateway: "stripe"
  #     }
  #   )
  #
  # Validation Benefits:
  # - **Type Safety**: Automatic type checking and coercion
  # - **Required Fields**: Ensures all required data is present
  # - **Schema Enforcement**: Validates structure matches expectations
  # - **Early Error Detection**: Catches data issues before processing
  # - **Documentation**: Types serve as living documentation
  #
  # @see TaskerCore::Types::TaskRequest For task creation
  # @see TaskerCore::Types::TaskResponse For task responses
  # @see TaskerCore::Types::StepCompletion For step execution results
  # @see TaskerCore::Types::SimpleStepMessage For UUID-based step messaging
  # @see TaskerCore::Types::SimpleTaskMessage For UUID-based task messaging
  # @see TaskerCore::Types::TaskTemplate For workflow definitions
  # @see TaskerCore::Types::StepHandlerCallResult For standardized handler responses
  # @see https://dry-rb.org/gems/dry-types For dry-types documentation
  module Types
    include Dry.Types()

    # Make base types available in class scope

    # Re-export all type classes for backward compatibility and convenience

    # Task-related types
    TaskRequest = TaskTypes::TaskRequest
    TaskResponse = TaskTypes::TaskResponse

    # Step-related types
    StepCompletion = StepTypes::StepCompletion

    # Simple message types (UUID-based) - already defined in the namespace
  end
end
