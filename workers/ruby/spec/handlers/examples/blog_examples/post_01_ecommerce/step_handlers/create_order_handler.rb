# frozen_string_literal: true

module Ecommerce
  module StepHandlers
    class CreateOrderHandler < TaskerCore::StepHandler::Base
      def call(context)
        # Extract and validate all required inputs
        order_inputs = extract_and_validate_inputs(context)

        logger.info "📝 CreateOrderHandler: Creating order - task_uuid=#{context.task_uuid}, customer=#{order_inputs[:customer_info][:email]}"

        # Create the order record - this is the core integration
        order_response = create_order_record(order_inputs, context)
        order = order_response[:order]

        logger.info "✅ CreateOrderHandler: Order created - order_id=#{order[:id]}, order_number=#{order[:order_number]}"

        # Return standardized StepHandlerCallResult
        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            order_id: order[:id],
            order_number: order[:order_number],
            status: order[:status],
            total_amount: order[:total_amount],
            customer_email: order[:customer_email],
            created_at: order[:created_at].iso8601,
            estimated_delivery: calculate_estimated_delivery
          },
          metadata: {
            operation: 'create_order',
            execution_hints: {
              order_id: order[:id],
              order_number: order[:order_number],
              item_count: order[:item_count],
              total_amount: order[:total_amount],
              creation_timestamp: order_response[:creation_timestamp]
            },
            http_headers: {
              'X-Order-Service' => 'OrderManagement',
              'X-Order-ID' => order[:id].to_s,
              'X-Order-Number' => order[:order_number],
              'X-Order-Status' => order[:status]
            },
            input_refs: {
              customer_info: 'context.task.context.customer_info',
              cart_validation: 'sequence.validate_cart.result',
              payment_result: 'sequence.process_payment.result',
              inventory_result: 'sequence.update_inventory.result'
            }
          }
        )
      rescue StandardError => e
        logger.error "❌ CreateOrderHandler: Order creation failed - #{e.class.name}: #{e.message}"
        raise
      end

      private

      # Extract and validate all required inputs for order creation
      def extract_and_validate_inputs(context)
        # Normalize all hash keys to symbols for consistent access
        task_context = context.task.context.deep_symbolize_keys
        customer_info = task_context[:customer_info]

        cart_validation = context.get_dependency_result('validate_cart')
        cart_validation = cart_validation.deep_symbolize_keys if cart_validation

        payment_result = context.get_dependency_result('process_payment')
        payment_result = payment_result.deep_symbolize_keys if payment_result

        inventory_result = context.get_dependency_result('update_inventory')
        inventory_result = inventory_result.deep_symbolize_keys if inventory_result

        unless customer_info
          raise TaskerCore::Errors::PermanentError.new(
            'Customer information is required but was not provided',
            error_code: 'MISSING_CUSTOMER_INFO'
          )
        end

        unless cart_validation&.dig(:validated_items)&.any?
          raise TaskerCore::Errors::PermanentError.new(
            'Cart validation results are required but were not found from validate_cart step',
            error_code: 'MISSING_CART_VALIDATION'
          )
        end

        unless payment_result&.dig(:payment_id)
          raise TaskerCore::Errors::PermanentError.new(
            'Payment results are required but were not found from process_payment step',
            error_code: 'MISSING_PAYMENT_RESULT'
          )
        end

        unless inventory_result&.dig(:updated_products)&.any?
          raise TaskerCore::Errors::PermanentError.new(
            'Inventory results are required but were not found from update_inventory step',
            error_code: 'MISSING_INVENTORY_RESULT'
          )
        end

        {
          customer_info: customer_info,
          cart_validation: cart_validation,
          payment_result: payment_result,
          inventory_result: inventory_result
        }
      end

      # Create the order record using validated inputs
      def create_order_record(order_inputs, context)
        customer_info = order_inputs[:customer_info]
        cart_validation = order_inputs[:cart_validation]
        payment_result = order_inputs[:payment_result]
        inventory_result = order_inputs[:inventory_result]

        # Simulate order creation (inline instead of using Order model)
        order_id = rand(1000..9999)
        order_number = generate_order_number
        order_data = {
          id: order_id,
          order_number: order_number,
          status: 'confirmed',

          # Customer information
          customer_email: customer_info[:email],
          customer_name: customer_info[:name],
          customer_phone: customer_info[:phone],

          # Order totals
          subtotal: cart_validation[:subtotal],
          tax_amount: cart_validation[:tax],
          shipping_amount: cart_validation[:shipping],
          total_amount: cart_validation[:total],

          # Payment information
          payment_id: payment_result[:payment_id],
          payment_status: 'completed',
          transaction_id: payment_result[:transaction_id],

          # Order items
          items: cart_validation[:validated_items],
          item_count: cart_validation[:item_count],

          # Inventory tracking
          inventory_log_id: inventory_result[:inventory_log_id],

          # Tracking
          task_uuid: context.task_uuid,
          workflow_version: '1.0.0',

          # Timestamps
          created_at: Time.now,
          updated_at: Time.now,
          placed_at: Time.now
        }

        {
          order: order_data,
          creation_timestamp: Time.now.utc.iso8601
        }
      end

      def generate_order_number
        "ORD-#{Date.today.strftime('%Y%m%d')}-#{SecureRandom.hex(4).upcase}"
      end

      def calculate_estimated_delivery
        # Simple delivery estimation - 7 days from now
        delivery_date = Time.now + (7 * 24 * 60 * 60) # 7 days in seconds
        delivery_date.strftime('%B %d, %Y')
      end
    end
  end
end
