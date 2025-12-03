# frozen_string_literal: true

# TAS-65: Domain Event Publishing - Process Payment Step Handler
#
# This handler demonstrates step-level domain event declarations with:
# - Success condition event (payment.processed)
# - Failure condition event (payment.failed)
# - Durable delivery mode (PGMQ)
# - Custom publisher specification (PaymentEventPublisher)
#
# Events published based on outcome:
#   - payment.processed (durable, on success)
#   - payment.failed (durable, on failure)
#
module DomainEvents
  module StepHandlers
    class ProcessPaymentHandler < TaskerCore::StepHandler::Base
      # Process the payment step
      #
      # @param task [Object] The task being executed
      # @param _sequence [Object] The workflow sequence (unused)
      # @param _step [Object] The current step (unused)
      # @return [TaskerCore::Types::StepHandlerCallResult] The step result
      def call(task, _sequence, _step)
        start_time = Process.clock_gettime(Process::CLOCK_MONOTONIC)

        # Extract context data
        context = task.context || {}
        order_id = context['order_id'] || 'unknown'
        amount = context['amount'] || 0
        simulate_failure = context['simulate_failure'] || false

        # Get configuration from handler config
        payment_provider = config['payment_provider'] || 'mock'

        log_info("Processing payment for order: #{order_id}, amount: #{amount}, provider: #{payment_provider}")

        # Simulate failure if requested
        if simulate_failure
          execution_time_ms = ((Process.clock_gettime(Process::CLOCK_MONOTONIC) - start_time) * 1000).to_i

          return TaskerCore::Types::StepHandlerCallResult.failure(
            error_message: 'Simulated payment failure',
            error_code: 'PAYMENT_DECLINED',
            error_type: 'PaymentError',
            retryable: true,
            metadata: {
              execution_time_ms: execution_time_ms,
              order_id: order_id,
              failed_at: Time.now.iso8601
            }
          )
        end

        # Generate transaction ID
        transaction_id = "TXN-#{SecureRandom.uuid}"
        execution_time_ms = ((Process.clock_gettime(Process::CLOCK_MONOTONIC) - start_time) * 1000).to_i

        # Return success - event publishing is handled by worker callback
        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            transaction_id: transaction_id,
            amount: amount,
            payment_method: 'credit_card',
            processed_at: Time.now.iso8601,
            status: 'success'
          },
          metadata: {
            execution_time_ms: execution_time_ms,
            payment_provider: payment_provider
          }
        )
      end

      private

      def log_info(message)
        puts "[DomainEvents::ProcessPaymentHandler] #{message}" if ENV['TASKER_ENV'] == 'test'
      end
    end
  end
end
