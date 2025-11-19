# frozen_string_literal: true

module TaskerCore
  # Clean Handlers Domain API
  #
  # This module provides the primary public interface for working with TaskerCore handlers.
  # It's organized into two main namespaces: Steps and Tasks, mirroring the Rails engine
  # architecture while providing a clean Ruby interface with enhanced type safety.
  #
  # The Handlers namespace serves as the recommended entry point for handler operations,
  # abstracting the underlying implementation details while preserving method signatures
  # that developers familiar with the Rails engine will recognize.
  #
  # @example Creating and using a step handler
  #   # Define a handler class
  #   class ProcessPaymentHandler < TaskerCore::Handlers::Steps::Base
  #     def call(task, sequence, step)
  #       # Access task context
  #       amount = task.context['amount']
  #       currency = task.context['currency']
  #
  #       # Process payment logic
  #       result = charge_payment(amount, currency)
  #
  #       # Return results to be stored in step.results
  #       { payment_id: result.id, status: "succeeded" }
  #     end
  #   end
  #
  #   # Create instance with configuration
  #   handler = TaskerCore::Handlers::Steps.create(
  #     ProcessPaymentHandler,
  #     config: { timeout: 30, max_attempts: 3 }
  #   )
  #
  # @example Validating handler implementation
  #   validation = TaskerCore::Handlers::Steps.validate(ProcessPaymentHandler)
  #   # => {
  #   #   valid: true,
  #   #   missing_required: [],
  #   #   optional_implemented: [:process_results],
  #   #   handler_class: "ProcessPaymentHandler"
  #   # }
  #
  #   if validation[:valid]
  #     puts "Handler implements all required methods"
  #   else
  #     puts "Missing: #{validation[:missing_required].join(', ')}"
  #   end
  #
  # @example Using API handlers for HTTP operations
  #   class FetchUserHandler < TaskerCore::Handlers::Steps::API
  #     def call(task, sequence, step)
  #       user_id = task.context['user_id']
  #
  #       # Automatic error classification and retry logic
  #       response = get("/users/#{user_id}")
  #
  #       response.body # Stored in step.results
  #     end
  #   end
  #
  # @example Task-level handler for workflow coordination
  #   # Task handlers coordinate multiple steps
  #   result = TaskerCore::Handlers::Tasks.handle(task_uuid: "123-456")
  #   # => Orchestrates all steps for the task
  #
  # Architecture:
  # - **Steps**: Individual business logic units (payment processing, API calls, etc.)
  # - **Tasks**: Workflow orchestration and step coordination
  # - **API**: Specialized step handlers for HTTP operations with automatic retry
  #
  # Method Signature Preservation:
  # This namespace preserves Rails engine method signatures for compatibility:
  # - `call(task, sequence, step)` - Primary handler execution
  # - `process(task, sequence, step)` - Alias for call (Rails compatibility)
  # - `process_results(task, sequence, step)` - Optional results processing hook
  #
  # @see TaskerCore::Handlers::Steps For step-level business logic
  # @see TaskerCore::Handlers::Tasks For task-level orchestration
  # @see TaskerCore::StepHandler::Base For low-level step handler implementation
  # @see TaskerCore::StepHandler::Api For HTTP-based handlers
  module Handlers
    # Step Handler API with preserved method signatures
    module Steps
      # Import the existing base step handler with preserved signatures
      require_relative 'step_handler/base'
      require_relative 'step_handler/api'
      require_relative 'step_handler/decision' # TAS-53: Decision point handlers
      require_relative 'step_handler/batchable' # TAS-59: Batch processing handlers

      # Re-export with clean namespace
      Base = TaskerCore::StepHandler::Base
      API = TaskerCore::StepHandler::Api
      Decision = TaskerCore::StepHandler::Decision
      Batchable = TaskerCore::StepHandler::Batchable

      class << self
        # Create a new step handler instance
        # @param handler_class [Class] Handler class to instantiate
        # @param config [Hash] Handler configuration
        # @return [Object] Handler instance
        def create(handler_class, config: {})
          handler_class.new(config: config)
        end

        # Validate step handler implementation
        # @param handler_class [Class] Handler class to validate
        # @return [Hash] Validation result
        def validate(handler_class)
          required_methods = %i[process]
          optional_methods = %i[process_results handle]

          missing = required_methods.reject { |method| handler_class.method_defined?(method) }
          present_optional = optional_methods.select { |method| handler_class.method_defined?(method) }

          {
            valid: missing.empty?,
            missing_required: missing,
            optional_implemented: present_optional,
            handler_class: handler_class.name
          }
        end
      end
    end

    # Task Handler API
    module Tasks
      require_relative 'task_handler/base'

      # Re-export with clean namespace
      Base = TaskHandler

      class << self
        # Handle task with preserved signature
        # @param task_uuid [Integer] Task ID to handle
        # @return [Object] Task handle result
        def handle(task_uuid)
          Base.new.handle(task_uuid)
        end

        # Create task handler instance
        # @param config [Hash] Handler configuration
        # @return [TaskHandler] Handler instance
        def create(config: {})
          Base.new(config: config)
        end
      end
    end
  end
end
