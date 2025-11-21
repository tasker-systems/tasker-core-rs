# frozen_string_literal: true

module Ecommerce
  class OrderProcessingHandler < TaskerCore::TaskHandler::Base
    # Simple task handler for e-commerce order processing workflow
    # Mainly validates input and delegates to orchestration

    # Override initialize_task to validate input before delegation
    def initialize_task(task_request)
      # Validate required inputs
      validate_order_input(task_request[:context])

      # Delegate to parent for actual task initialization
      super
    end

    # Metadata for monitoring and introspection
    def metadata
      super.merge(
        workflow_type: 'ecommerce_order_processing',
        blog_post: 'post_01',
        steps_count: 5,
        average_completion_time: '10s'
      )
    end

    private

    def validate_order_input(context)
      # Validate cart items
      cart_items = context['cart_items'] || []
      if cart_items.empty?
        raise TaskerCore::Errors::ValidationError.new(
          'Cart items are required but were not provided',
          field: 'cart_items',
          error_code: 'MISSING_CART_ITEMS'
        )
      end

      # Validate customer info
      customer_info = context['customer_info'] || {}
      unless customer_info['email']
        raise TaskerCore::Errors::ValidationError.new(
          'Customer email is required',
          field: 'customer_info.email',
          error_code: 'MISSING_CUSTOMER_EMAIL'
        )
      end

      unless customer_info['name']
        raise TaskerCore::Errors::ValidationError.new(
          'Customer name is required',
          field: 'customer_info.name',
          error_code: 'MISSING_CUSTOMER_NAME'
        )
      end

      # Validate payment info
      payment_info = context['payment_info'] || {}
      unless payment_info['method']
        raise TaskerCore::Errors::ValidationError.new(
          'Payment method is required',
          field: 'payment_info.method',
          error_code: 'MISSING_PAYMENT_METHOD'
        )
      end

      unless payment_info['token']
        raise TaskerCore::Errors::ValidationError.new(
          'Payment token is required',
          field: 'payment_info.token',
          error_code: 'MISSING_PAYMENT_TOKEN'
        )
      end

      unless payment_info['amount']
        raise TaskerCore::Errors::ValidationError.new(
          'Payment amount is required',
          field: 'payment_info.amount',
          error_code: 'MISSING_PAYMENT_AMOUNT'
        )
      end

      logger.info "✅ Order input validated: #{cart_items.length} items, customer=#{customer_info['email']}"
    end
  end
end
