# frozen_string_literal: true

module DataPipeline
  module StepHandlers
    # Transform customer data for analytics
    #
    # This handler demonstrates:
    # - Sequential execution (depends on extract_customer_data)
    # - Accessing prior step results via context.get_dependency_result()
    # - Data transformation logic
    class TransformCustomersHandler < TaskerCore::StepHandler::Base
      def call(context)
        logger.info "🔄 TransformCustomersHandler: Transforming customer data - task_uuid=#{context.task_uuid}"

        # Get extracted customer data from prior step
        extract_results = context.get_dependency_result('extract_customer_data')
        unless extract_results
          raise TaskerCore::Errors::PermanentError.new(
            'Customer extraction results not found',
            error_code: 'MISSING_EXTRACT_RESULTS'
          )
        end

        # Transform the data
        transformed = transform_customer_data(extract_results)

        logger.info "✅ TransformCustomersHandler: Transformed #{transformed[:record_count]} customer records"

        TaskerCore::Types::StepHandlerCallResult.success(
          result: transformed,
          metadata: {
            operation: 'transform_customers',
            source_step: 'extract_customer_data',
            transformation_applied: true,
            record_count: transformed[:record_count],
            transformed_at: Time.now.utc.iso8601,
            handler_class: self.class.name
          }
        )
      rescue StandardError => e
        logger.error "❌ TransformCustomersHandler: Transformation failed - #{e.class.name}: #{e.message}"
        raise
      end

      private

      def transform_customer_data(extract_results)
        raw_records = extract_results['records'] || extract_results[:records]

        # Transform: Calculate tier summaries and value segmentation
        tier_analysis = raw_records.group_by { |r| r[:tier] || r['tier'] }
                                   .transform_values do |tier_records|
          {
            customer_count: tier_records.count,
            total_lifetime_value: tier_records.sum { |r| r[:lifetime_value] || r['lifetime_value'] || 0 },
            avg_lifetime_value: tier_records.sum do |r|
              r[:lifetime_value] || r['lifetime_value'] || 0
            end / tier_records.count.to_f
          }
        end

        # Segment customers by value
        value_segments = {
          high_value: raw_records.count { |r| (r[:lifetime_value] || r['lifetime_value'] || 0) >= 10_000 },
          medium_value: raw_records.count { |r| (r[:lifetime_value] || r['lifetime_value'] || 0).between?(1000, 9999) },
          low_value: raw_records.count { |r| (r[:lifetime_value] || r['lifetime_value'] || 0) < 1000 }
        }

        {
          record_count: raw_records.count,
          tier_analysis: tier_analysis,
          value_segments: value_segments,
          total_lifetime_value: raw_records.sum { |r| r[:lifetime_value] || r['lifetime_value'] || 0 },
          avg_customer_value: raw_records.sum do |r|
            r[:lifetime_value] || r['lifetime_value'] || 0
          end / raw_records.count.to_f,
          transformation_type: 'customer_analytics',
          source: 'extract_customer_data'
        }
      end
    end
  end
end
