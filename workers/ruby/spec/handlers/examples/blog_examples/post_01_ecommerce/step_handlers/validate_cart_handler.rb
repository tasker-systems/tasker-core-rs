# frozen_string_literal: true

module Ecommerce
  module StepHandlers
    class ValidateCartHandler < TaskerCore::StepHandler::Base
      # Mock product database for e-commerce demonstration
      PRODUCTS = {
        1 => { id: 1, name: 'Widget A', price: 29.99, stock: 100, active: true },
        2 => { id: 2, name: 'Widget B', price: 49.99, stock: 50, active: true },
        3 => { id: 3, name: 'Widget C', price: 19.99, stock: 25, active: true },
        4 => { id: 4, name: 'Widget D', price: 39.99, stock: 0, active: true },  # Out of stock
        5 => { id: 5, name: 'Widget E', price: 59.99, stock: 10, active: false } # Inactive
      }.freeze
      def call(task, sequence, step)
        # Extract and validate all required inputs
        cart_inputs = extract_and_validate_inputs(task, sequence, step)

        logger.info "🛒 ValidateCartHandler: Validating cart - task_uuid=#{task.task_uuid}, item_count=#{cart_inputs[:cart_items].length}"

        # Validate each item and build validated items list
        validated_items = validate_cart_items(cart_inputs[:cart_items])

        # Calculate totals
        subtotal = validated_items.sum { |item| item[:line_total] }
        tax_rate = 0.08 # 8% tax rate
        tax = (subtotal * tax_rate).round(2)
        shipping = calculate_shipping(validated_items)
        total = subtotal + tax + shipping

        logger.info "✅ ValidateCartHandler: Cart validation completed - subtotal=$#{subtotal}, total=$#{total}"

        # Return standardized StepHandlerCallResult
        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            validated_items: validated_items,
            subtotal: subtotal,
            tax: tax,
            shipping: shipping,
            total: total,
            item_count: validated_items.length,
            validated_at: Time.now.utc.iso8601
          },
          metadata: {
            operation: 'validate_cart',
            execution_hints: {
              items_validated: validated_items.length,
              total_amount: total,
              tax_rate: tax_rate,
              shipping_cost: shipping
            },
            input_refs: {
              cart_items: 'task.context.cart_items'
            }
          }
        )
      rescue StandardError => e
        logger.error "❌ ValidateCartHandler: Cart validation failed - #{e.message}"
        raise
      end

      private

      # Extract and validate all required inputs for cart validation
      def extract_and_validate_inputs(task, _sequence, _step)
        # Normalize all hash keys to symbols for consistent access
        context = task.context.deep_symbolize_keys
        cart_items = context[:cart_items]

        unless cart_items&.any?
          raise TaskerCore::Errors::PermanentError.new(
            'Cart items are required but were not provided',
            error_code: 'MISSING_CART_ITEMS'
          )
        end

        # Normalize each cart item's keys to symbols
        normalized_cart_items = cart_items.map { |item| item.deep_symbolize_keys }

        # Validate each cart item has required fields
        normalized_cart_items.each_with_index do |item, index|
          unless item[:product_id]
            raise TaskerCore::Errors::PermanentError.new(
              "Product ID is required for cart item #{index + 1}",
              error_code: 'MISSING_PRODUCT_ID'
            )
          end

          next if item[:quantity]&.positive?

          raise TaskerCore::Errors::PermanentError.new(
            "Valid quantity is required for cart item #{index + 1}",
            error_code: 'INVALID_QUANTITY'
          )
        end

        {
          cart_items: normalized_cart_items
        }
      end

      # Validate each cart item exists and is available
      def validate_cart_items(cart_items)
        cart_items.map do |item|
          product = PRODUCTS[item[:product_id]]

          unless product
            raise TaskerCore::Errors::PermanentError.new(
              "Product #{item[:product_id]} not found",
              error_code: 'PRODUCT_NOT_FOUND'
            )
          end

          unless product[:active]
            raise TaskerCore::Errors::PermanentError.new(
              "Product #{product[:name]} is no longer available",
              error_code: 'PRODUCT_INACTIVE'
            )
          end

          if product[:stock] < item[:quantity]
            # Temporary failure - inventory might be updated soon
            raise TaskerCore::Errors::RetryableError.new(
              "Insufficient stock for #{product[:name]}. Available: #{product[:stock]}, Requested: #{item[:quantity]}",
              retry_after: 30,
              context: {
                product_id: product[:id],
                product_name: product[:name],
                available_stock: product[:stock],
                requested_quantity: item[:quantity]
              }
            )
          end

          {
            product_id: product[:id],
            name: product[:name],
            price: product[:price],
            quantity: item[:quantity],
            line_total: (product[:price] * item[:quantity]).round(2)
          }
        end
      end

      def calculate_shipping(items)
        # Simple shipping calculation based on weight
        total_weight = items.sum { |item| item[:quantity] * 0.5 } # 0.5 lbs per item

        case total_weight
        when 0..2
          5.99
        when 2..10
          9.99
        else
          14.99
        end
      end
    end
  end
end
