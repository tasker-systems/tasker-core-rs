# frozen_string_literal: true

module DataPipeline
  module StepHandlers
    # Extract inventory data from warehouse system (simulated)
    #
    # This handler demonstrates:
    # - Parallel execution (runs concurrently with other extracts)
    # - Self-contained simulation of inventory source
    # - Inline sample data
    #
    # TAS-137 Best Practices Demonstrated:
    # - Root DAG node: No task context or dependency access needed
    # - Data sourced from inline sample data (simulated external system)
    # - Results passed to downstream transform_inventory step via DAG
    class ExtractInventoryDataHandler < TaskerCore::StepHandler::Base
      # Sample inventory data (inline simulation)
      SAMPLE_INVENTORY_DATA = [
        { product_id: 'PROD-A', sku: 'SKU-A-001', warehouse: 'WH-01', quantity_on_hand: 150, reorder_point: 50 },
        { product_id: 'PROD-B', sku: 'SKU-B-002', warehouse: 'WH-01', quantity_on_hand: 75, reorder_point: 25 },
        { product_id: 'PROD-C', sku: 'SKU-C-003', warehouse: 'WH-02', quantity_on_hand: 200, reorder_point: 100 },
        { product_id: 'PROD-A', sku: 'SKU-A-001', warehouse: 'WH-02', quantity_on_hand: 100, reorder_point: 50 },
        { product_id: 'PROD-B', sku: 'SKU-B-002', warehouse: 'WH-03', quantity_on_hand: 50, reorder_point: 25 }
      ].freeze

      def call(context)
        logger.info "📦 ExtractInventoryDataHandler: Extracting inventory data - task_uuid=#{context.task_uuid}"

        # Simulate data extraction from inventory system
        inventory_data = extract_inventory_from_source

        logger.info "✅ ExtractInventoryDataHandler: Extracted #{inventory_data[:records].count} inventory records"

        TaskerCore::Types::StepHandlerCallResult.success(
          result: inventory_data,
          metadata: {
            operation: 'extract_inventory',
            source: 'InventorySystem',
            records_extracted: inventory_data[:records].count,
            warehouses: inventory_data[:warehouses],
            extraction_time: Time.now.utc.iso8601,
            handler_class: self.class.name
          }
        )
      rescue StandardError => e
        logger.error "❌ ExtractInventoryDataHandler: Extraction failed - #{e.class.name}: #{e.message}"
        raise
      end

      private

      def extract_inventory_from_source
        # Simulate inventory query
        {
          records: SAMPLE_INVENTORY_DATA,
          extracted_at: Time.now.utc.iso8601,
          source: 'InventorySystem',
          total_quantity: SAMPLE_INVENTORY_DATA.sum { |r| r[:quantity_on_hand] },
          warehouses: SAMPLE_INVENTORY_DATA.map { |r| r[:warehouse] }.uniq,
          products_tracked: SAMPLE_INVENTORY_DATA.map { |r| r[:product_id] }.uniq.count
        }
      end
    end
  end
end
