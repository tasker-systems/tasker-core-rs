# frozen_string_literal: true

module DataPipeline
  module StepHandlers
    # Transform inventory data for analytics
    #
    # This handler demonstrates:
    # - Sequential execution (depends on extract_inventory_data)
    # - Accessing prior step results via sequence.get_results()
    # - Data transformation logic
    class TransformInventoryHandler < TaskerCore::StepHandler::Base
      def call(task, sequence, _step)
        logger.info "🔄 TransformInventoryHandler: Transforming inventory data - task_uuid=#{task.task_uuid}"

        # Get extracted inventory data from prior step
        extract_results = sequence.get_results('extract_inventory_data')
        unless extract_results
          raise TaskerCore::Errors::PermanentError.new(
            'Inventory extraction results not found',
            error_code: 'MISSING_EXTRACT_RESULTS'
          )
        end

        # Transform the data
        transformed = transform_inventory_data(extract_results)

        logger.info "✅ TransformInventoryHandler: Transformed #{transformed[:record_count]} inventory records"

        TaskerCore::Types::StepHandlerCallResult.success(
          result: transformed,
          metadata: {
            operation: 'transform_inventory',
            source_step: 'extract_inventory_data',
            transformation_applied: true,
            record_count: transformed[:record_count],
            transformed_at: Time.now.utc.iso8601,
            handler_class: self.class.name
          }
        )
      rescue StandardError => e
        logger.error "❌ TransformInventoryHandler: Transformation failed - #{e.class.name}: #{e.message}"
        raise
      end

      private

      def transform_inventory_data(extract_results)
        raw_records = extract_results['records'] || extract_results[:records]

        # Transform: Calculate warehouse summaries and reorder alerts
        warehouse_summary = raw_records.group_by { |r| r[:warehouse] || r['warehouse'] }
          .transform_values do |wh_records|
            {
              total_quantity: wh_records.sum { |r| r[:quantity_on_hand] || r['quantity_on_hand'] || 0 },
              product_count: wh_records.map { |r| r[:product_id] || r['product_id'] }.uniq.count,
              reorder_alerts: wh_records.count { |r| (r[:quantity_on_hand] || r['quantity_on_hand'] || 0) <= (r[:reorder_point] || r['reorder_point'] || 0) }
            }
          end

        product_inventory = raw_records.group_by { |r| r[:product_id] || r['product_id'] }
          .transform_values do |product_records|
            total_qty = product_records.sum { |r| r[:quantity_on_hand] || r['quantity_on_hand'] || 0 }
            total_reorder = product_records.sum { |r| r[:reorder_point] || r['reorder_point'] || 0 }
            {
              total_quantity: total_qty,
              warehouse_count: product_records.map { |r| r[:warehouse] || r['warehouse'] }.uniq.count,
              needs_reorder: total_qty < total_reorder
            }
          end

        {
          record_count: raw_records.count,
          warehouse_summary: warehouse_summary,
          product_inventory: product_inventory,
          total_quantity_on_hand: raw_records.sum { |r| r[:quantity_on_hand] || r['quantity_on_hand'] || 0 },
          reorder_alerts: product_inventory.count { |_id, data| data[:needs_reorder] },
          transformation_type: 'inventory_analytics',
          source: 'extract_inventory_data'
        }
      end
    end
  end
end
