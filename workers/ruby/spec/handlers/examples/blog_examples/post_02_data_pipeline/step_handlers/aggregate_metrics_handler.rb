# frozen_string_literal: true

module DataPipeline
  module StepHandlers
    # Aggregate metrics from all transformed data sources
    #
    # This handler demonstrates:
    # - DAG convergence (depends on all 3 transform steps)
    # - Accessing results from multiple parallel branches
    # - Data aggregation across sources
    class AggregateMetricsHandler < TaskerCore::StepHandler::Base
      def call(context)
        logger.info "📊 AggregateMetricsHandler: Aggregating metrics from all sources - task_uuid=#{context.task_uuid}"

        # Get results from all transform steps (3 parallel branches converge here)
        sales_data = context.get_dependency_result('transform_sales')
        inventory_data = context.get_dependency_result('transform_inventory')
        customer_data = context.get_dependency_result('transform_customers')

        # Validate all sources present
        validate_all_sources_present!(sales_data, inventory_data, customer_data)

        # Aggregate across all sources
        aggregated = aggregate_all_sources(sales_data, inventory_data, customer_data)

        logger.info '✅ AggregateMetricsHandler: Aggregated metrics from 3 sources'
        logger.info "   Total revenue: $#{aggregated[:total_revenue]}"
        logger.info "   Total inventory: #{aggregated[:total_inventory_quantity]}"
        logger.info "   Total customers: #{aggregated[:total_customers]}"

        TaskerCore::Types::StepHandlerCallResult.success(
          result: aggregated,
          metadata: {
            operation: 'aggregate_metrics',
            sources: %w[transform_sales transform_inventory transform_customers],
            sources_aggregated: 3,
            aggregated_at: Time.now.utc.iso8601,
            handler_class: self.class.name
          }
        )
      rescue StandardError => e
        logger.error "❌ AggregateMetricsHandler: Aggregation failed - #{e.class.name}: #{e.message}"
        raise
      end

      private

      def validate_all_sources_present!(sales, inventory, customers)
        missing = []
        missing << 'transform_sales' unless sales
        missing << 'transform_inventory' unless inventory
        missing << 'transform_customers' unless customers

        return if missing.empty?

        raise TaskerCore::Errors::PermanentError.new(
          "Missing transform results: #{missing.join(', ')}",
          error_code: 'MISSING_TRANSFORM_RESULTS'
        )
      end

      def aggregate_all_sources(sales_data, inventory_data, customer_data)
        # Extract metrics from each source
        total_revenue = sales_data['total_revenue'] || sales_data[:total_revenue] || 0
        sales_record_count = sales_data['record_count'] || sales_data[:record_count] || 0

        total_inventory = inventory_data['total_quantity_on_hand'] || inventory_data[:total_quantity_on_hand] || 0
        reorder_alerts = inventory_data['reorder_alerts'] || inventory_data[:reorder_alerts] || 0

        total_customers = customer_data['record_count'] || customer_data[:record_count] || 0
        total_ltv = customer_data['total_lifetime_value'] || customer_data[:total_lifetime_value] || 0

        # Calculate cross-source metrics
        revenue_per_customer = total_customers.positive? ? total_revenue / total_customers.to_f : 0
        inventory_turnover_indicator = total_inventory.positive? ? total_revenue / total_inventory.to_f : 0

        {
          total_revenue: total_revenue,
          total_inventory_quantity: total_inventory,
          total_customers: total_customers,
          total_customer_lifetime_value: total_ltv,
          sales_transactions: sales_record_count,
          inventory_reorder_alerts: reorder_alerts,
          revenue_per_customer: revenue_per_customer.round(2),
          inventory_turnover_indicator: inventory_turnover_indicator.round(4),
          aggregation_complete: true,
          sources_included: 3
        }
      end
    end
  end
end
