# frozen_string_literal: true

require_relative '../../../../../lib/tasker_core/errors'

module OrderFulfillment
  module StepHandlers
    class ShipOrderHandler < TaskerCore::StepHandler::Base
      def call(task, sequence, step)
        # Extract and validate all required inputs
        shipping_inputs = extract_and_validate_inputs(task, sequence, step)

        puts "Processing shipment: task_id=#{task.task_id}, items=#{shipping_inputs[:items_to_ship].length}"

        # Create shipping label and process shipment
        shipping_results = create_shipment(shipping_inputs)

        {
          shipment_id: shipping_results[:shipment_id],
          tracking_number: shipping_results[:tracking_number],
          shipping_status: 'label_created',
          estimated_delivery: shipping_results[:estimated_delivery],
          shipping_cost: shipping_results[:shipping_cost],
          carrier: shipping_results[:carrier],
          service_type: shipping_results[:service_type],
          label_url: shipping_results[:label_url],
          processed_at: Time.now.iso8601,
          raw_shipping_response: shipping_results,
          # Include orchestration metadata
          _orchestration_metadata: {
            http_headers: {
              'X-Carrier-Name' => shipping_results[:carrier],
              'X-Tracking-Number' => shipping_results[:tracking_number],
              'X-Carrier-Request-ID' => shipping_results[:carrier_request_id] || "req-#{SecureRandom.hex(6)}"
            },
            execution_hints: {
              carrier_api_response_time_ms: shipping_results[:api_response_time] || 250,
              label_generation_time_ms: shipping_results[:label_generation_time] || 100,
              international_shipment: shipping_inputs[:shipping_address][:country] != 'US'
            },
            backoff_hints: {
              # Carrier-specific rate limits
              carrier_rate_limit_remaining: shipping_results[:rate_limit_remaining] || 100,
              carrier_rate_limit_reset_at: shipping_results[:rate_limit_reset_at],
              # Suggest backing off if we're getting close to rate limit
              suggested_backoff_seconds: shipping_results[:rate_limit_remaining] && shipping_results[:rate_limit_remaining] < 10 ? 60 : nil
            }
          }
        }
      end

      private

      def extract_and_validate_inputs(task, sequence, _step)
        # Get results from previous steps
        validate_order_step = sequence.steps.find { |s| s.name == 'validate_order' }
        reserve_inventory_step = sequence.steps.find { |s| s.name == 'reserve_inventory' }
        process_payment_step = sequence.steps.find { |s| s.name == 'process_payment' }

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

        unless process_payment_step&.results
          raise TaskerCore::Errors::PermanentError.new(
            'process_payment step results not found',
            error_code: 'MISSING_PAYMENT_RESULTS'
          )
        end

        # Extract shipping info from task context
        context = deep_symbolize_keys(task.context)
        shipping_info = context[:shipping_info]
        customer_info = context[:customer_info]

        unless shipping_info
          raise TaskerCore::Errors::PermanentError.new(
            'Shipping information is required',
            error_code: 'MISSING_SHIPPING_INFO'
          )
        end

        unless shipping_info[:address]
          raise TaskerCore::Errors::PermanentError.new(
            'Shipping address is required',
            error_code: 'MISSING_SHIPPING_ADDRESS'
          )
        end

        # Get validated items from order validation
        validated_items = validate_order_step.results['validated_items']

        {
          items_to_ship: validated_items,
          shipping_address: shipping_info[:address],
          shipping_method: shipping_info[:method] || 'standard',
          customer_email: customer_info[:email],
          customer_id: validate_order_step.results['customer_id'],
          reservation_id: reserve_inventory_step.results['reservation_id'],
          payment_id: process_payment_step.results['payment_id']
        }
      end

      def create_shipment(inputs)
        # Generate shipment ID
        shipment_id = "SHIP-#{Time.now.to_i}-#{SecureRandom.hex(4).upcase}"

        # Simulate shipping carrier API call
        carrier_response = simulate_shipping_carrier_call(
          shipment_id: shipment_id,
          items: inputs[:items_to_ship],
          address: inputs[:shipping_address],
          method: inputs[:shipping_method]
        )

        # Calculate estimated delivery
        estimated_delivery = calculate_delivery_estimate(inputs[:shipping_method])

        {
          shipment_id: shipment_id,
          tracking_number: carrier_response[:tracking_number],
          label_url: carrier_response[:label_url],
          estimated_delivery: estimated_delivery,
          shipping_cost: carrier_response[:cost],
          carrier: carrier_response[:carrier],
          service_type: carrier_response[:service]
        }
      end

      def simulate_shipping_carrier_call(shipment_id:, items:, address:, method:)
        # Simulate various shipping carrier scenarios

        # Simulate carrier API timeout (3% chance)
        if rand < 0.03
          raise TaskerCore::Errors::TimeoutError.new(
            "Shipping carrier API timeout",
            timeout_duration: 60,
            context: { shipment_id: shipment_id, carrier: 'fedex' }
          )
        end

        # Simulate network error (2% chance)
        if rand < 0.02
          raise TaskerCore::Errors::NetworkError.new(
            "Shipping carrier network error",
            status_code: 503,
            context: { shipment_id: shipment_id, service: 'shipping_api' }
          )
        end

        # Simulate rate limiting (1% chance)
        if rand < 0.01
          raise TaskerCore::Errors::RetryableError.new(
            "Shipping carrier rate limited",
            retry_after: 45,
            context: { shipment_id: shipment_id, carrier: 'fedex' }
          )
        end

        # Simulate address validation failure (4% chance)
        if rand < 0.04
          raise TaskerCore::Errors::PermanentError.new(
            "Invalid shipping address: #{address}",
            error_code: 'INVALID_SHIPPING_ADDRESS',
            context: { address: address, shipment_id: shipment_id }
          )
        end

        # Select carrier and service based on method
        carrier_info = select_carrier_and_service(method)

        # Calculate shipping cost based on items and method
        total_weight = items.sum { |item| item['quantity'] * 0.5 }  # 0.5 lbs per item
        shipping_cost = calculate_shipping_cost(total_weight, method)

        # Success case
        api_response_time = (100 + rand(300)).to_i  # 100-400ms
        rate_limit_remaining = 50 + rand(50)  # 50-100 remaining calls
        
        {
          tracking_number: generate_tracking_number(carrier_info[:carrier]),
          label_url: "https://labels.#{carrier_info[:carrier].downcase}.com/#{shipment_id}.pdf",
          cost: shipping_cost,
          carrier: carrier_info[:carrier],
          service: carrier_info[:service],
          api_response_time: api_response_time,
          label_generation_time: (50 + rand(100)).to_i,  # 50-150ms
          rate_limit_remaining: rate_limit_remaining,
          rate_limit_reset_at: rate_limit_remaining < 20 ? (Time.now + 3600).iso8601 : nil
        }
      end

      def select_carrier_and_service(method)
        case method
        when 'express'
          { carrier: 'FedEx', service: 'FedEx Express' }
        when 'overnight'
          { carrier: 'FedEx', service: 'FedEx Overnight' }
        else
          { carrier: 'UPS', service: 'UPS Ground' }
        end
      end

      def calculate_shipping_cost(weight, method)
        base_cost = case method
                   when 'overnight'
                     25.00
                   when 'express'
                     15.00
                   else
                     8.99
                   end

        # Add weight-based cost
        weight_cost = [weight - 1, 0].max * 2.50  # $2.50 per lb over 1 lb

        (base_cost + weight_cost).round(2)
      end

      def generate_tracking_number(carrier)
        case carrier
        when 'FedEx'
          "1Z#{SecureRandom.hex(8).upcase}"
        when 'UPS'
          "UPS#{SecureRandom.hex(6).upcase}"
        else
          "TRK#{SecureRandom.hex(7).upcase}"
        end
      end

      def calculate_delivery_estimate(method)
        business_days = case method
                       when 'overnight'
                         1
                       when 'express'
                         2
                       else
                         5
                       end

        # Calculate delivery date (skip weekends)
        delivery_date = Time.now.to_date
        days_added = 0

        while days_added < business_days
          delivery_date += (1 * 24 * 60 * 60)
        end

        delivery_date.iso8601
      end

      def send_shipping_notifications(step, shipping_response)
        # Get task configuration to check if notifications are enabled
        # This would be passed from the step configuration
        send_notifications = true  # Default for this example

        return unless send_notifications

        begin
          # Simulate sending email notification
          puts "Sending shipping notification email"

          # In real implementation, this would:
          # 1. Send confirmation email to customer
          # 2. Send tracking info via SMS if opted in
          # 3. Update order status in customer portal
          # 4. Send notification to customer service

          # Add notification info to step results
          step.results['notifications_sent'] = {
            email_sent: true,
            sms_sent: false,  # Not implemented in this example
            notification_timestamp: Time.now.iso8601
          }

        rescue StandardError => e
          # Don't fail the entire step if notifications fail
          puts "Failed to send shipping notifications: #{e.message}"
          step.results['notifications_sent'] = {
            email_sent: false,
            sms_sent: false,
            error: e.message,
            notification_timestamp: Time.now.iso8601
          }
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
