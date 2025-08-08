# frozen_string_literal: true

require_relative '../../../../../lib/tasker_core/errors'

module OrderFulfillment
  module StepHandlers
    class ReserveInventoryHandler < TaskerCore::StepHandler::Base
      def call(task, sequence, step)
        # Extract and validate all required inputs
        reservation_inputs = extract_and_validate_inputs(task, sequence, step)

        puts "Reserving inventory: task_id=#{task.task_id}, items=#{reservation_inputs[:validated_items].length}"

        # Reserve inventory for validated items
        reservation_results = reserve_inventory_items(reservation_inputs)

        # Return standardized StepHandlerCallResult
        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            reservation_id: reservation_results[:reservation_id],
            items_reserved: reservation_results[:reservations].length,
            reservation_status: 'confirmed',
            total_reserved_value: reservation_results[:total_value],
            expires_at: reservation_results[:expires_at],
            reserved_at: Time.now.iso8601,
            reservation_details: reservation_results[:reservations]
          },
          metadata: {
            operation: 'reserve_inventory',
            items_count: reservation_results[:reservations].length,
            input_refs: {
              validated_items: 'sequence.validate_order.result.validated_items',
              customer_id: 'sequence.validate_order.result.customer_id'
            }
          }
        )
      end

      private

      def extract_and_validate_inputs(_task, sequence, _step)
        # Get validated items from the validate_order step
        validate_order_results = sequence.get_results('validate_order')

        unless validate_order_results
          raise TaskerCore::Errors::PermanentError.new(
            'validate_order step results not found',
            error_code: 'MISSING_DEPENDENCY_RESULTS'
          )
        end

        validated_items = validate_order_results[:validated_items]

        unless validated_items&.any?
          raise TaskerCore::Errors::PermanentError.new(
            'No validated items found from validate_order step',
            error_code: 'NO_VALIDATED_ITEMS'
          )
        end

        # Convert to symbol keys for consistent access
        validated_items = validated_items.map { |item| deep_symbolize_keys(item) }

        {
          validated_items: validated_items,
          customer_id: validate_order_results[:customer_id],
          order_total: validate_order_results[:order_total]
        }
      end

      def reserve_inventory_items(inputs)
        # Generate reservation ID
        reservation_id = "RES-#{Time.now.to_i}-#{SecureRandom.hex(4).upcase}"

        # Set reservation expiration (15 minutes from now)
        expires_at = (Time.now + (15 * 60)).iso8601

        # Reserve each item
        reservations = inputs[:validated_items].map do |item|
          # Simulate inventory check and reservation
          reservation_result = simulate_inventory_reservation(
            product_id: item[:product_id],
            quantity: item[:quantity],
            reservation_id: reservation_id
          )

          {
            product_id: item[:product_id],
            product_name: item[:product_name],
            quantity_requested: item[:quantity],
            quantity_reserved: reservation_result[:reserved_quantity],
            unit_price: item[:unit_price],
            line_total: item[:line_total],
            stock_location: reservation_result[:location],
            reservation_reference: reservation_result[:reference]
          }
        end

        total_value = reservations.sum { |r| r[:line_total] }

        {
          reservations: reservations,
          reservation_id: reservation_id,
          total_value: total_value,
          expires_at: expires_at
        }
      end

      def simulate_inventory_reservation(product_id:, quantity:, reservation_id:)
        # Simulate inventory system interaction
        # In real implementation, this would call an inventory service API

        case product_id
        when 101
          available_stock = 100
          location = 'WH-EAST-A1'
        when 102
          available_stock = 50
          location = 'WH-WEST-B2'
        when 103
          available_stock = 200
          location = 'WH-CENTRAL-C3'
        else
          raise TaskerCore::Errors::PermanentError.new(
            "Product #{product_id} not found in inventory system",
            error_code: 'PRODUCT_NOT_IN_INVENTORY',
            context: { product_id: product_id }
          )
        end

        # Check if we have sufficient stock
        if available_stock < quantity
          # This is a retryable error - inventory might be replenished
          raise TaskerCore::Errors::RetryableError.new(
            "Insufficient stock for product #{product_id}. Available: #{available_stock}, Requested: #{quantity}",
            retry_after: 60, # Wait 1 minute before retrying
            context: {
              product_id: product_id,
              available: available_stock,
              requested: quantity
            }
          )
        end

        # Success case
        {
          reserved_quantity: quantity,
          location: location,
          reference: "#{reservation_id}-#{product_id}"
        }
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
