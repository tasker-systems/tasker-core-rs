# frozen_string_literal: true

module Ecommerce
  module StepHandlers
    class UpdateInventoryHandler < TaskerCore::StepHandler::Base
      # Mock product inventory database (shared with ValidateCartHandler)
      PRODUCTS = {
        1 => { id: 1, name: 'Widget A', stock: 100 },
        2 => { id: 2, name: 'Widget B', stock: 50 },
        3 => { id: 3, name: 'Widget C', stock: 25 },
        4 => { id: 4, name: 'Widget D', stock: 0 },
        5 => { id: 5, name: 'Widget E', stock: 10 }
      }.freeze
      def call(context)
        # Extract and validate all required inputs
        inventory_inputs = extract_and_validate_inputs(context)

        logger.info "📦 UpdateInventoryHandler: Updating inventory - task_uuid=#{context.task_uuid}, item_count=#{inventory_inputs[:validated_items].length}"

        # Process inventory reservations - this is the core integration
        reservation_response = process_inventory_reservations(
          inventory_inputs[:validated_items],
          inventory_inputs[:customer_info],
          context.task_uuid
        )

        # Extract results from reservation response
        updated_products = reservation_response[:updated_products]
        inventory_changes = reservation_response[:inventory_changes]

        logger.info "✅ UpdateInventoryHandler: Inventory updated - products_updated=#{updated_products.length}"

        # Return standardized StepHandlerCallResult
        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            updated_products: updated_products,
            total_items_reserved: updated_products.sum { |product| product[:quantity_reserved] },
            inventory_changes: inventory_changes,
            inventory_log_id: reservation_response[:inventory_log_id],
            updated_at: Time.now.utc.iso8601
          },
          metadata: {
            operation: 'update_inventory',
            execution_hints: {
              products_updated: updated_products.length,
              total_items_reserved: updated_products.sum { |p| p[:quantity_reserved] },
              reservation_timestamp: reservation_response[:reservation_timestamp]
            },
            http_headers: {
              'X-Inventory-Service' => 'MockInventoryService',
              'X-Reservation-Count' => updated_products.length.to_s,
              'X-Total-Quantity' => updated_products.sum { |p| p[:quantity_reserved] }.to_s
            },
            input_refs: {
              validated_items: 'sequence.validate_cart.result.validated_items',
              customer_info: 'context.task.context.customer_info'
            }
          }
        )
      rescue StandardError => e
        logger.error "❌ UpdateInventoryHandler: Inventory update failed - #{e.class.name}: #{e.message}"
        raise
      end

      private

      # Extract and validate all required inputs for inventory processing
      def extract_and_validate_inputs(context)
        # Normalize all hash keys to symbols for consistent access
        cart_validation = context.get_dependency_result('validate_cart')
        cart_validation = cart_validation.deep_symbolize_keys if cart_validation
        customer_info = context.task.context.deep_symbolize_keys[:customer_info]

        unless cart_validation&.dig(:validated_items)&.any?
          raise TaskerCore::Errors::PermanentError.new(
            'Validated cart items are required but were not found from validate_cart step',
            error_code: 'MISSING_VALIDATED_ITEMS'
          )
        end

        unless customer_info
          raise TaskerCore::Errors::PermanentError.new(
            'Customer information is required but was not provided',
            error_code: 'MISSING_CUSTOMER_INFO'
          )
        end

        {
          validated_items: cart_validation[:validated_items],
          customer_info: customer_info
        }
      end

      # Process inventory reservations for all validated items
      def process_inventory_reservations(validated_items, _customer_info, _task_uuid)
        updated_products = []
        inventory_changes = []

        # Update inventory for each item using simulated inventory system
        validated_items.each do |item|
          product = PRODUCTS[item[:product_id]]

          unless product
            raise TaskerCore::Errors::PermanentError.new(
              "Product #{item[:product_id]} not found",
              error_code: 'PRODUCT_NOT_FOUND'
            )
          end

          # Check availability
          stock_level = product[:stock]
          available = stock_level >= item[:quantity]

          unless available
            raise TaskerCore::Errors::RetryableError.new(
              "Stock not available for #{product[:name]}. Available: #{stock_level}, Needed: #{item[:quantity]}",
              retry_after: 30,
              context: {
                product_id: product[:id],
                product_name: product[:name],
                available_stock: stock_level,
                requested_quantity: item[:quantity]
              }
            )
          end

          # Simulate inventory reservation
          reservation_id = "rsv_#{SecureRandom.hex(8)}"

          updated_products << {
            product_id: product[:id],
            name: product[:name],
            previous_stock: stock_level,
            new_stock: stock_level - item[:quantity],
            quantity_reserved: item[:quantity],
            reservation_id: reservation_id
          }

          inventory_changes << {
            product_id: product[:id],
            change_type: 'reservation',
            quantity: -item[:quantity],
            reason: 'order_checkout',
            timestamp: Time.now.utc.iso8601,
            reservation_id: reservation_id,
            inventory_log_id: "log_#{SecureRandom.hex(6)}"
          }
        end

        {
          updated_products: updated_products,
          inventory_changes: inventory_changes,
          reservation_timestamp: Time.now.utc.iso8601,
          inventory_log_id: "log_#{SecureRandom.hex(8)}"
        }
      end
    end
  end
end
