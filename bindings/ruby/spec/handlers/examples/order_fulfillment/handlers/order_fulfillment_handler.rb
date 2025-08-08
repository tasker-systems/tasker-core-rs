# frozen_string_literal: true

require_relative '../../../../../lib/tasker_core/task_handler/base'
require_relative '../../../../../lib/tasker_core/errors'

module OrderFulfillment
  class OrderFulfillmentHandler < TaskerCore::TaskHandler::Base
    # No need to override initialize - use base class initialization
    # No need to override handle - base class delegates to Rust orchestration

    # Override initialize_task to apply custom configuration before delegation
    def initialize_task(task_request)
      # Apply our custom configuration logic to the task context
      setup_task_specific_configuration(task_request[:context])

      # Delegate to parent for actual task initialization
      super
    end

    # Custom configuration based on task context
    def setup_task_specific_configuration(context)
      # Apply any configuration overrides based on customer tier
      apply_premium_optimizations!(context) if context['customer_info']['tier'] == 'premium'

      # Validate order constraints
      validate_order_constraints(context)

      # Log task setup for monitoring
      logger.info "OrderFulfillment task setup complete for customer #{context['customer_info']['id']}"
    end

    # Metadata for monitoring and introspection
    def metadata
      super.merge(
        workflow_type: 'order_fulfillment',
        supports_premium_tier: true,
        max_concurrent_steps: 2,
        average_completion_time: '45s'
      )
    end

    private

    def apply_premium_optimizations!(context)
      # Premium customers get faster processing
      context['processing_priority'] = 'high'
      context['expedited_shipping'] = true

      # Premium timeout overrides
      context['step_timeouts'] = {
        'validate_order' => 3,
        'reserve_inventory' => 5,
        'process_payment' => 10,
        'ship_order' => 15
      }

      logger.info "Applied premium optimizations for customer #{context['customer_info']['id']}"
    end

    def validate_order_constraints(context)
      order_items = context['order_items'] || []
      payment_info = context['payment_info'] || {}

      # Get configuration constraints
      config = @task_config&.dig('handler_config') || {}
      max_items = config['order_validation_rules']&.dig('max_items_per_order') || 50
      max_value = config['order_validation_rules']&.dig('max_order_value') || 10_000.00
      min_value = config['order_validation_rules']&.dig('min_order_value') || 0.01

      # Validate item count
      if order_items.length > max_items
        raise TaskerCore::Errors::ValidationError.new(
          "Order exceeds maximum items limit of #{max_items}",
          field: 'order_items',
          error_code: 'MAX_ITEMS_EXCEEDED',
          context: { item_count: order_items.length, max_allowed: max_items }
        )
      end

      # Validate order value
      order_value = payment_info['amount'] || 0
      if order_value > max_value
        raise TaskerCore::Errors::ValidationError.new(
          "Order value exceeds maximum limit of $#{max_value}",
          field: 'payment_info.amount',
          error_code: 'MAX_VALUE_EXCEEDED',
          context: { order_value: order_value, max_allowed: max_value }
        )
      end

      if order_value < min_value
        raise TaskerCore::Errors::ValidationError.new(
          "Order value below minimum limit of $#{min_value}",
          field: 'payment_info.amount',
          error_code: 'MIN_VALUE_NOT_MET',
          context: { order_value: order_value, min_required: min_value }
        )
      end

      logger.debug "Order constraints validated: #{order_items.length} items, $#{order_value} value"
    end
  end
end
