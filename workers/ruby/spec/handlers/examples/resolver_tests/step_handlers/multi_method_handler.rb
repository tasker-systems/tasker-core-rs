# frozen_string_literal: true

# TAS-93 Phase 5: Multi-Method Handler for Resolver Chain E2E Testing.
#
# This handler demonstrates the method dispatch feature of the resolver chain.
# It has multiple entry points beyond the default `call` method, allowing
# YAML templates to specify `method: "validate"` or `method: "process"` etc.
#
# Example YAML configuration:
#   handler:
#     callable: ResolverTests::StepHandlers::MultiMethodHandler
#     method: validate  # Invokes validate() instead of call()

module ResolverTests
  module StepHandlers
    # Multi-method handler demonstrating method dispatch.
    #
    # Available methods:
    # - call: Default entry point (standard processing)
    # - validate: Validation-only path
    # - process: Processing-specific path
    # - refund: Refund-specific path (payment domain example)
    #
    # Each method returns a result with `invoked_method` so tests can verify
    # which method was actually called.
    class MultiMethodHandler < TaskerCore::StepHandler::Base
      # Default entry point - standard processing.
      def call(context)
        input = context.task.context['data'] || {}

        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            invoked_method: 'call',
            handler: 'MultiMethodHandler',
            message: 'Default call method invoked',
            input_received: input,
            step_name: context.step_name
          }
        )
      end

      # Validation-only entry point.
      #
      # Can be invoked with: `method: "validate"`
      def validate(context)
        input = context.task.context['data'] || {}

        # Simple validation logic for testing
        has_required_fields = input.key?('amount')

        unless has_required_fields
          return TaskerCore::Types::StepHandlerCallResult.failure(
            message: 'Validation failed: missing required field "amount"',
            error_type: 'validation_error',
            retryable: false
          )
        end

        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            invoked_method: 'validate',
            handler: 'MultiMethodHandler',
            message: 'Validation completed successfully',
            validated: true,
            input_validated: input,
            step_name: context.step_name
          }
        )
      end

      # Processing entry point.
      #
      # Can be invoked with: `method: "process"`
      def process(context)
        input = context.task.context['data'] || {}
        amount = input['amount'] || 0

        # Simple processing logic for testing
        processed_amount = amount * 1.1 # Add 10% processing fee

        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            invoked_method: 'process',
            handler: 'MultiMethodHandler',
            message: 'Processing completed',
            original_amount: amount,
            processed_amount: processed_amount,
            processing_fee: processed_amount - amount,
            step_name: context.step_name
          }
        )
      end

      # Refund entry point.
      #
      # Can be invoked with: `method: "refund"`
      # Demonstrates payment domain method dispatch pattern.
      def refund(context)
        input = context.task.context['data'] || {}
        amount = input['amount'] || 0
        reason = input['reason'] || 'not_specified'

        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            invoked_method: 'refund',
            handler: 'MultiMethodHandler',
            message: 'Refund processed',
            refund_amount: amount,
            refund_reason: reason,
            refund_id: "refund_#{Time.now.to_i}",
            step_name: context.step_name
          }
        )
      end
    end

    # Second multi-method handler for testing resolver chain with different handlers.
    #
    # This handler is used to verify that the resolver chain can find
    # handlers by different callable addresses.
    class AlternateMethodHandler < TaskerCore::StepHandler::Base
      # Default entry point.
      def call(context)
        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            invoked_method: 'call',
            handler: 'AlternateMethodHandler',
            message: 'Alternate handler default method',
            step_name: context.step_name
          }
        )
      end

      # Custom action method.
      #
      # Can be invoked with: `method: "execute_action"`
      def execute_action(context)
        action = context.task.context['action_type'] || 'default_action'

        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            invoked_method: 'execute_action',
            handler: 'AlternateMethodHandler',
            message: 'Custom action executed',
            action_type: action,
            step_name: context.step_name
          }
        )
      end
    end
  end
end
