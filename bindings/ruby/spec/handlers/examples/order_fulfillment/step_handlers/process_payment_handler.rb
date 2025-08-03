# frozen_string_literal: true

require_relative '../../../../../lib/tasker_core/errors'

module OrderFulfillment
  module StepHandlers
    class ProcessPaymentHandler < TaskerCore::StepHandler::Base
      def call(task, sequence, step)
        # Extract and validate all required inputs
        payment_inputs = extract_and_validate_inputs(task, sequence, step)

        logger.info "🔍 ProcessPaymentHandler: Processing payment - task_id=#{task.task_id}, amount=$#{payment_inputs[:amount_to_charge]}"

        # Process payment through payment gateway
        payment_results = process_payment_transaction(payment_inputs)

        {
          payment_processed: true,
          payment_id: payment_results[:payment_id],
          transaction_id: payment_results[:transaction_id],
          amount_charged: payment_results[:amount_charged],
          payment_method_used: payment_results[:payment_method],
          gateway_response: payment_results[:gateway_response],
          processed_at: Time.now.iso8601,
          payment_status: 'completed',
          # Include orchestration metadata for the orchestration layer
          _orchestration_metadata: {
            http_headers: {
              'X-Payment-Gateway' => 'stripe',
              'X-Gateway-Request-ID' => payment_results[:transaction_id],
              'X-Idempotency-Key' => payment_results[:payment_id]
            },
            execution_hints: {
              gateway_response_time_ms: payment_results[:gateway_response][:processing_time_ms] || 150,
              gateway_fee_amount: payment_results[:gateway_response][:gateway_fee],
              requires_3ds_authentication: false
            },
            backoff_hints: {
              # If this was a retry after rate limiting, suggest backing off
              suggested_backoff_seconds: payment_results[:was_rate_limited] ? 30 : nil,
              gateway_load_indicator: payment_results[:gateway_response][:load_indicator] || 'normal'
            }
          }
        }
      end

      private

      def extract_and_validate_inputs(task, sequence, step)
        # Get order validation results
        logger.info "🔍 ProcessPaymentHandler: Extracting and validating inputs - task=#{task.to_h}, sequence=#{sequence.to_h}, step=#{step.to_h}"
        validate_order_step = sequence.steps.find { |s| s.name == 'validate_order' }
        reserve_inventory_step = sequence.steps.find { |s| s.name == 'reserve_inventory' }

        unless validate_order_step&.results
          raise TaskerCore::Errors::PermanentError.new(
            'validate_order step results not found',
            error_code: 'MISSING_VALIDATION_RESULTS'
          )
        end

        unless reserve_inventory_step&.results
          raise TaskerCore::Errors::PermanentError.new(
            'reserve_inventory step results not found',
            error_code: 'MISSING_RESERVATION_RESULTS'
          )
        end

        # Extract payment info from task context
        context = deep_symbolize_keys(task.context)
        payment_info = context[:payment_info]

        unless payment_info
          raise TaskerCore::Errors::PermanentError.new(
            'Payment information is required',
            error_code: 'MISSING_PAYMENT_INFO'
          )
        end

        # Validate payment method
        unless payment_info[:method]
          raise TaskerCore::Errors::PermanentError.new(
            'Payment method is required',
            error_code: 'MISSING_PAYMENT_METHOD'
          )
        end

        # Validate payment token
        unless payment_info[:token]
          raise TaskerCore::Errors::PermanentError.new(
            'Payment token is required',
            error_code: 'MISSING_PAYMENT_TOKEN'
          )
        end

        # Get amount from validation results
        order_total = validate_order_step.results['order_total']
        payment_amount = payment_info[:amount]

        # Validate amounts match
        if payment_amount != order_total
          raise TaskerCore::Errors::PermanentError.new(
            "Payment amount mismatch. Order total: $#{order_total}, Payment amount: $#{payment_amount}",
            error_code: 'PAYMENT_AMOUNT_MISMATCH',
            context: { order_total: order_total, payment_amount: payment_amount }
          )
        end

        {
          amount_to_charge: order_total,
          payment_method: payment_info[:method],
          payment_token: payment_info[:token],
          customer_id: validate_order_step.results['customer_id'],
          reservation_id: reserve_inventory_step.results['reservation_id']
        }
      end

      def process_payment_transaction(inputs)
        # Generate payment ID
        payment_id = "PAY-#{Time.now.to_i}-#{SecureRandom.hex(6).upcase}"

        # Simulate payment gateway interaction
        gateway_response = simulate_payment_gateway_call(
          amount: inputs[:amount_to_charge],
          method: inputs[:payment_method],
          token: inputs[:payment_token],
          payment_id: payment_id
        )

        # Validate payment was successful
        unless gateway_response[:status] == 'succeeded'
          handle_payment_failure(gateway_response)
        end

        {
          payment_id: payment_id,
          transaction_id: gateway_response[:transaction_id],
          amount_charged: gateway_response[:amount],
          payment_method: inputs[:payment_method],
          gateway_response: gateway_response
        }
      end

      def simulate_payment_gateway_call(amount:, method:, token:, payment_id:)
        # Simulate various payment gateway scenarios

        # Simulate network timeout (2% chance)
        if rand < 0.02
          raise TaskerCore::Errors::TimeoutError.new(
            "Payment gateway timeout",
            timeout_duration: 45,
            context: { payment_id: payment_id, amount: amount }
          )
        end

        # Simulate rate limiting (1% chance)
        if rand < 0.01
          raise TaskerCore::Errors::RetryableError.new(
            "Payment gateway rate limited",
            retry_after: 30,
            context: { payment_id: payment_id, service: 'payment_gateway' }
          )
        end

        # Simulate network error (2% chance)
        if rand < 0.02
          raise TaskerCore::Errors::NetworkError.new(
            "Payment gateway network error",
            status_code: 502,
            context: { payment_id: payment_id, gateway: 'stripe_api' }
          )
        end

        # Simulate payment method specific failures
        case method
        when 'credit_card'
          # Simulate card declined (5% chance)
          if rand < 0.05
            return {
              status: 'failed',
              error_code: 'card_declined',
              error_message: 'Your card was declined',
              decline_code: 'insufficient_funds'
            }
          end
        when 'paypal'
          # Simulate PayPal auth failure (3% chance)
          if rand < 0.03
            return {
              status: 'failed',
              error_code: 'authentication_failed',
              error_message: 'PayPal authentication failed'
            }
          end
        end

        # Success case
        processing_time = (50 + rand(200)).to_i  # Simulate 50-250ms processing time
        {
          status: 'succeeded',
          transaction_id: "TXN-#{SecureRandom.hex(8).upcase}",
          amount: amount,
          currency: 'USD',
          payment_method_type: method,
          reference: "#{payment_id}-#{Time.now.to_i}",
          gateway_fee: (amount * 0.029).round(2),  # 2.9% gateway fee
          processed_at: Time.now.iso8601,
          processing_time_ms: processing_time,
          load_indicator: processing_time > 200 ? 'high' : 'normal'
        }
      end

      def handle_payment_failure(gateway_response)
        error_code = gateway_response[:error_code]
        error_message = gateway_response[:error_message]

        case error_code
        when 'card_declined', 'insufficient_funds', 'invalid_card'
          # Permanent failures - don't retry
          raise TaskerCore::Errors::PermanentError.new(
            "Payment declined: #{error_message}",
            error_code: 'PAYMENT_DECLINED',
            context: { gateway_error: error_code, decline_code: gateway_response[:decline_code] }
          )
        when 'authentication_failed', 'invalid_token'
          # Authentication issues are permanent
          raise TaskerCore::Errors::PermanentError.new(
            "Payment authentication failed: #{error_message}",
            error_code: 'PAYMENT_AUTH_FAILED',
            context: { gateway_error: error_code }
          )
        when 'rate_limited'
          # Rate limiting is retryable
          raise TaskerCore::Errors::RetryableError.new(
            "Payment gateway rate limited: #{error_message}",
            retry_after: 60,
            context: { gateway_error: error_code }
          )
        else
          # Unknown errors are treated as retryable for safety
          raise TaskerCore::Errors::RetryableError.new(
            "Payment gateway error: #{error_message}",
            retry_after: 30,
            context: { gateway_error: error_code }
          )
        end
      end

      # Rails compatibility method - deep symbolize keys for hashes
      def deep_symbolize_keys(obj)
        case obj
        when Hash
          obj.each_with_object({}) do |(key, value), result|
            result[key.to_sym] = deep_symbolize_keys(value)
          end
        when Array
          obj.map { |item| deep_symbolize_keys(item) }
        else
          obj
        end
      end
    end
  end
end
