# frozen_string_literal: true

require_relative '../../../../../lib/tasker_core/errors'

module OrderFulfillment
  module StepHandlers
    class ValidateOrderHandler < TaskerCore::StepHandler::Base
      def call(task, sequence, step)
        logger.info "🎯 VALIDATE_ORDER: Starting order validation - task_id=#{task.task_id}, step_name=#{step.name}"

        # Extract and validate all required inputs
        order_inputs = extract_and_validate_inputs(task, sequence, step)
        logger.info "✅ VALIDATE_ORDER: Input validation complete - customer_id=#{order_inputs[:customer_id]}, item_count=#{order_inputs[:order_items]&.length}"

        # Validate order data - let Rust orchestration handle general error wrapping
        validation_results = validate_order_data(order_inputs)
        logger.info "✅ VALIDATE_ORDER: Order data validation complete - total_amount=#{validation_results[:total]}, validated_items=#{validation_results[:items]&.length}"

        # Return standardized StepHandlerCallResult
        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            customer_validated: true,
            customer_id: validation_results[:customer_id],
            validated_items: validation_results[:items],
            order_total: validation_results[:total],
            validation_status: 'complete',
            validated_at: Time.now.iso8601
          },
          metadata: {
            operation: 'validate_order',
            item_count: validation_results[:items]&.length || 0,
            input_refs: {
              customer_id: 'task.context.customer_info.id',
              order_items: 'task.context.order_items'
            }
          }
        )
      end

      private

      def extract_and_validate_inputs(task, _sequence, _step)
        logger.info "📋 VALIDATE_ORDER: Extracting task context - task.context.class=#{task.context.class}"
        context = deep_symbolize_keys(task.context)
        logger.info "🔍 VALIDATE_ORDER: Task context keys - #{context.keys.inspect}" if context.respond_to?(:keys)

        customer_info = context[:customer_info]
        order_items = context[:order_items]
        logger.debug "🔍 VALIDATE_ORDER: Extracted data - customer_info=#{customer_info.inspect}, order_items.count=#{order_items&.length}"

        unless customer_info&.dig(:id)
          raise TaskerCore::Errors::PermanentError.new(
            'Customer ID is required',
            error_code: 'MISSING_CUSTOMER_ID'
          )
        end

        unless customer_info&.dig(:email)
          raise TaskerCore::Errors::PermanentError.new(
            'Customer email is required',
            error_code: 'MISSING_CUSTOMER_EMAIL'
          )
        end

        unless order_items&.any?
          raise TaskerCore::Errors::PermanentError.new(
            'Order items are required',
            error_code: 'MISSING_ORDER_ITEMS'
          )
        end

        {
          customer_id: customer_info[:id],
          customer_email: customer_info[:email],
          customer_tier: customer_info[:tier] || 'standard',
          order_items: order_items
        }
      end

      def validate_order_data(inputs)
        # Validate each order item
        validated_items = inputs[:order_items].map.with_index do |item, index|
          unless item[:product_id] && item[:quantity] && item[:price]
            raise TaskerCore::Errors::PermanentError.new(
              "Invalid order item at position #{index + 1}: missing required fields",
              error_code: 'INVALID_ORDER_ITEM',
              context: { item_index: index, item: item }
            )
          end

          # Validate quantity is positive
          if item[:quantity] <= 0
            raise TaskerCore::Errors::PermanentError.new(
              "Invalid quantity #{item[:quantity]} for product #{item[:product_id]}",
              error_code: 'INVALID_QUANTITY',
              context: { product_id: item[:product_id], quantity: item[:quantity] }
            )
          end

          # Validate price is non-negative
          if item[:price] < 0
            raise TaskerCore::Errors::PermanentError.new(
              "Invalid price #{item[:price]} for product #{item[:product_id]}",
              error_code: 'INVALID_PRICE',
              context: { product_id: item[:product_id], price: item[:price] }
            )
          end

          # Simulate product lookup (in real implementation, this would query a product service)
          product_data = simulate_product_lookup(item[:product_id])

          {
            product_id: item[:product_id],
            product_name: product_data[:name],
            quantity: item[:quantity],
            unit_price: item[:price],
            line_total: item[:price] * item[:quantity],
            available_stock: product_data[:stock],
            category: product_data[:category]
          }
        end

        total_amount = validated_items.sum { |item| item[:line_total] }

        # Validate reasonable order total
        if total_amount > 50_000.00
          raise TaskerCore::Errors::PermanentError.new(
            "Order total $#{total_amount} exceeds maximum allowed value",
            error_code: 'ORDER_TOTAL_TOO_HIGH',
            context: { total_amount: total_amount, max_allowed: 50_000.00 }
          )
        end

        {
          items: validated_items,
          total: total_amount,
          customer_id: inputs[:customer_id],
          item_count: validated_items.length
        }
      end

      def simulate_product_lookup(product_id)
        # Simulate product database lookup
        # In real implementation, this would call a product service
        case product_id
        when 101
          { name: 'Premium Widget A', stock: 100, category: 'widgets' }
        when 102
          { name: 'Deluxe Widget B', stock: 50, category: 'widgets' }
        when 103
          { name: 'Standard Gadget C', stock: 200, category: 'gadgets' }
        else
          # Simulate product not found
          raise TaskerCore::Errors::PermanentError.new(
            "Product #{product_id} not found",
            error_code: 'PRODUCT_NOT_FOUND',
            context: { product_id: product_id }
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
