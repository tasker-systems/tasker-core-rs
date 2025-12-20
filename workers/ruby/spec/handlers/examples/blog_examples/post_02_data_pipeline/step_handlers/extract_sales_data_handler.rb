# frozen_string_literal: true

module DataPipeline
  module StepHandlers
    # Extract sales data from source system (simulated)
    #
    # This handler demonstrates:
    # - Parallel execution (runs concurrently with other extracts)
    # - Self-contained simulation of data source
    # - Inline sample data
    class ExtractSalesDataHandler < TaskerCore::StepHandler::Base
      # Sample sales data (inline simulation)
      SAMPLE_SALES_DATA = [
        { order_id: 'ORD-001', date: '2025-11-01', product_id: 'PROD-A', quantity: 5, amount: 499.95 },
        { order_id: 'ORD-002', date: '2025-11-05', product_id: 'PROD-B', quantity: 3, amount: 299.97 },
        { order_id: 'ORD-003', date: '2025-11-10', product_id: 'PROD-A', quantity: 2, amount: 199.98 },
        { order_id: 'ORD-004', date: '2025-11-15', product_id: 'PROD-C', quantity: 10, amount: 1499.90 },
        { order_id: 'ORD-005', date: '2025-11-18', product_id: 'PROD-B', quantity: 7, amount: 699.93 }
      ].freeze

      def call(context)
        logger.info "📦 ExtractSalesDataHandler: Extracting sales data - task_uuid=#{context.task_uuid}"

        # Simulate data extraction from sales database
        sales_data = extract_sales_from_source

        logger.info "✅ ExtractSalesDataHandler: Extracted #{sales_data[:records].count} sales records"

        TaskerCore::Types::StepHandlerCallResult.success(
          result: sales_data,
          metadata: {
            operation: 'extract_sales',
            source: 'SalesDatabase',
            records_extracted: sales_data[:records].count,
            extraction_time: Time.now.utc.iso8601,
            handler_class: self.class.name
          }
        )
      rescue StandardError => e
        logger.error "❌ ExtractSalesDataHandler: Extraction failed - #{e.class.name}: #{e.message}"
        raise
      end

      private

      def extract_sales_from_source
        # Simulate database query
        {
          records: SAMPLE_SALES_DATA,
          extracted_at: Time.now.utc.iso8601,
          source: 'SalesDatabase',
          total_amount: SAMPLE_SALES_DATA.sum { |r| r[:amount] },
          date_range: {
            start_date: SAMPLE_SALES_DATA.map { |r| r[:date] }.min,
            end_date: SAMPLE_SALES_DATA.map { |r| r[:date] }.max
          }
        }
      end
    end
  end
end
