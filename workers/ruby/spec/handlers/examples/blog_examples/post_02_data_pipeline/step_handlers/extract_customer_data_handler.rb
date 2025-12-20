# frozen_string_literal: true

module DataPipeline
  module StepHandlers
    # Extract customer data from CRM system (simulated)
    #
    # This handler demonstrates:
    # - Parallel execution (runs concurrently with other extracts)
    # - Self-contained simulation of CRM source
    # - Inline sample data
    class ExtractCustomerDataHandler < TaskerCore::StepHandler::Base
      # Sample customer data (inline simulation)
      SAMPLE_CUSTOMER_DATA = [
        { customer_id: 'CUST-001', name: 'Alice Johnson', tier: 'gold', lifetime_value: 5000.00, join_date: '2024-01-15' },
        { customer_id: 'CUST-002', name: 'Bob Smith', tier: 'silver', lifetime_value: 2500.00, join_date: '2024-03-20' },
        { customer_id: 'CUST-003', name: 'Carol White', tier: 'premium', lifetime_value: 15000.00, join_date: '2023-11-10' },
        { customer_id: 'CUST-004', name: 'David Brown', tier: 'standard', lifetime_value: 500.00, join_date: '2025-01-05' },
        { customer_id: 'CUST-005', name: 'Eve Davis', tier: 'gold', lifetime_value: 7500.00, join_date: '2024-06-12' }
      ].freeze

      def call(context)
        logger.info "👥 ExtractCustomerDataHandler: Extracting customer data - task_uuid=#{context.task_uuid}"

        # Simulate data extraction from CRM
        customer_data = extract_customers_from_source

        logger.info "✅ ExtractCustomerDataHandler: Extracted #{customer_data[:records].count} customer records"

        TaskerCore::Types::StepHandlerCallResult.success(
          result: customer_data,
          metadata: {
            operation: 'extract_customers',
            source: 'CRMSystem',
            records_extracted: customer_data[:records].count,
            customer_tiers: customer_data[:tier_breakdown].keys,
            extraction_time: Time.now.utc.iso8601,
            handler_class: self.class.name
          }
        )
      rescue StandardError => e
        logger.error "❌ ExtractCustomerDataHandler: Extraction failed - #{e.class.name}: #{e.message}"
        raise
      end

      private

      def extract_customers_from_source
        # Simulate CRM query
        tier_breakdown = SAMPLE_CUSTOMER_DATA.group_by { |c| c[:tier] }.transform_values(&:count)

        {
          records: SAMPLE_CUSTOMER_DATA,
          extracted_at: Time.now.utc.iso8601,
          source: 'CRMSystem',
          total_customers: SAMPLE_CUSTOMER_DATA.count,
          total_lifetime_value: SAMPLE_CUSTOMER_DATA.sum { |r| r[:lifetime_value] },
          tier_breakdown: tier_breakdown,
          avg_lifetime_value: SAMPLE_CUSTOMER_DATA.sum { |r| r[:lifetime_value] } / SAMPLE_CUSTOMER_DATA.count.to_f
        }
      end
    end
  end
end
