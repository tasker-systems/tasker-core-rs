# frozen_string_literal: true

# TAS-65: Domain Event Publishing - Validate Order Step Handler
#
# This handler demonstrates step-level domain event declarations.
# The handler focuses on business logic only - event publishing is handled
# by the worker's post-execution callback system based on YAML declarations.
#
# Event published on success:
#   - order.validated (fast delivery mode)
#
module DomainEvents
  module StepHandlers
    class ValidateOrderHandler < TaskerCore::StepHandler::Base
      # Process the validation step
      #
      # @param task [Object] The task being executed
      # @param _sequence [Object] The workflow sequence (unused)
      # @param _step [Object] The current step (unused)
      # @return [TaskerCore::Types::StepHandlerCallResult] The step result
      def call(context)
        start_time = Process.clock_gettime(Process::CLOCK_MONOTONIC)

        # Extract context data
        context = context.task.context || {}
        order_id = context['order_id'] || SecureRandom.uuid
        customer_id = context['customer_id'] || 'unknown'
        amount = context['amount'] || 0

        # Get configuration from handler config
        validation_mode = config['validation_mode'] || 'standard'

        log_info("Validating order: #{order_id} for customer: #{customer_id}")

        # Perform validation
        validation_checks = ['order_id_present', 'customer_id_present']

        if validation_mode == 'strict' && amount <= 0
          return TaskerCore::Types::StepHandlerCallResult.failure(
            error_message: 'Amount must be positive in strict mode',
            error_code: 'VALIDATION_ERROR',
            error_type: 'ValidationError',
            retryable: false,
            metadata: { validation_mode: validation_mode }
          )
        end

        validation_checks << 'amount_positive' if amount.positive?

        execution_time_ms = ((Process.clock_gettime(Process::CLOCK_MONOTONIC) - start_time) * 1000).to_i

        # Return success - event publishing is handled by worker callback
        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            order_id: order_id,
            validation_timestamp: Time.now.iso8601,
            validation_checks: validation_checks,
            validated: true
          },
          metadata: {
            execution_time_ms: execution_time_ms,
            validation_mode: validation_mode
          }
        )
      end

      private

      def log_info(message)
        puts "[DomainEvents::ValidateOrderHandler] #{message}" if ENV['TASKER_ENV'] == 'test'
      end
    end
  end
end
