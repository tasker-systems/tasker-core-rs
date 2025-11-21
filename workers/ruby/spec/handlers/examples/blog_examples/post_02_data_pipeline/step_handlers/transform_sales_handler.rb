# frozen_string_literal: true

module DataPipeline
  module StepHandlers
    # Transform sales data for analytics
    #
    # This handler demonstrates:
    # - Sequential execution (depends on extract_sales_data)
    # - Accessing prior step results via sequence.get_results()
    # - Data transformation logic
    class TransformSalesHandler < TaskerCore::StepHandler::Base
      def call(task, sequence, _step)
        logger.info "🔄 TransformSalesHandler: Transforming sales data - task_uuid=#{task.task_uuid}"

        # Get extracted sales data from prior step
        extract_results = sequence.get_results('extract_sales_data')
        unless extract_results
          raise TaskerCore::Errors::PermanentError.new(
            'Sales extraction results not found',
            error_code: 'MISSING_EXTRACT_RESULTS'
          )
        end

        # Transform the data
        transformed = transform_sales_data(extract_results)

        logger.info "✅ TransformSalesHandler: Transformed #{transformed[:record_count]} sales records"

        TaskerCore::Types::StepHandlerCallResult.success(
          result: transformed,
          metadata: {
            operation: 'transform_sales',
            source_step: 'extract_sales_data',
            transformation_applied: true,
            record_count: transformed[:record_count],
            transformed_at: Time.now.utc.iso8601,
            handler_class: self.class.name
          }
        )
      rescue StandardError => e
        logger.error "❌ TransformSalesHandler: Transformation failed - #{e.class.name}: #{e.message}"
        raise
      end

      private

      def transform_sales_data(extract_results)
        raw_records = extract_results['records'] || extract_results[:records]

        # Transform: Calculate daily totals and product summaries
        daily_sales = raw_records.group_by { |r| r[:date] || r['date'] }
          .transform_values do |day_records|
            {
              total_amount: day_records.sum { |r| r[:amount] || r['amount'] || 0 },
              order_count: day_records.count,
              avg_order_value: day_records.sum { |r| r[:amount] || r['amount'] || 0 } / day_records.count.to_f
            }
          end

        product_sales = raw_records.group_by { |r| r[:product_id] || r['product_id'] }
          .transform_values do |product_records|
            {
              total_quantity: product_records.sum { |r| r[:quantity] || r['quantity'] || 0 },
              total_revenue: product_records.sum { |r| r[:amount] || r['amount'] || 0 },
              order_count: product_records.count
            }
          end

        {
          record_count: raw_records.count,
          daily_sales: daily_sales,
          product_sales: product_sales,
          total_revenue: raw_records.sum { |r| r[:amount] || r['amount'] || 0 },
          transformation_type: 'sales_analytics',
          source: 'extract_sales_data'
        }
      end
    end
  end
end
