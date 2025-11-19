# frozen_string_literal: true

require_relative 'base_mock_service'

# Mock Inventory Service
# Simulates inventory management for e-commerce blog examples
class MockInventoryService < BaseMockService
  # Standard inventory service errors
  class InventoryError < StandardError; end
  class InsufficientStockError < InventoryError; end
  class ProductNotFoundError < InventoryError; end
  class ReservationError < InventoryError; end

  # Check product availability
  # @param product_id [Integer] Product ID to check
  # @param quantity [Integer] Requested quantity
  # @return [Hash] Availability result
  def self.check_availability(product_id:, quantity:)
    instance = new
    instance.check_availability_call(
      product_id: product_id,
      quantity: quantity
    )
  end

  # Instance method for checking availability
  def check_availability_call(product_id:, quantity:)
    log_call(:check_availability, {
               product_id: product_id,
               quantity: quantity
             })

    default_response = {
      product_id: product_id,
      available: true,
      stock_level: 100,
      requested_quantity: quantity,
      can_fulfill: true,
      reserved_until: (Time.now + (15 * 60)).utc.iso8601
    }

    handle_response(:check_availability, default_response)
  end

  # Reserve inventory for an order
  # @param product_id [Integer] Product ID
  # @param quantity [Integer] Quantity to reserve
  # @param order_id [String] Order ID for tracking
  # @param customer_id [Integer] Customer ID
  # @return [Hash] Reservation result
  def self.reserve_inventory(product_id:, quantity:, order_id:, customer_id: nil)
    instance = new
    instance.reserve_inventory_call(
      product_id: product_id,
      quantity: quantity,
      order_id: order_id,
      customer_id: customer_id
    )
  end

  # Instance method for reserving inventory
  def reserve_inventory_call(product_id:, quantity:, order_id:, customer_id: nil)
    log_call(:reserve_inventory, {
               product_id: product_id,
               quantity: quantity,
               order_id: order_id,
               customer_id: customer_id
             })

    default_response = {
      reservation_id: generate_id('res'),
      product_id: product_id,
      quantity_reserved: quantity,
      order_id: order_id,
      customer_id: customer_id,
      reserved_at: generate_timestamp,
      expires_at: (Time.now + (15 * 60)).utc.iso8601,
      status: 'reserved'
    }

    handle_response(:reserve_inventory, default_response)
  end

  # Commit inventory reservation (finalize the stock reduction)
  # @param reservation_id [String] Reservation ID to commit
  # @return [Hash] Commit result
  def self.commit_reservation(reservation_id:)
    instance = new
    instance.log_call(:commit_reservation, { reservation_id: reservation_id })

    default_response = {
      reservation_id: reservation_id,
      status: 'committed',
      committed_at: instance.generate_timestamp,
      final_stock_level: 95 # Simulated remaining stock
    }

    instance.handle_response(:commit_reservation, default_response)
  end

  # Release inventory reservation (return stock to available)
  # @param reservation_id [String] Reservation ID to release
  # @param reason [String] Reason for release
  # @return [Hash] Release result
  def self.release_reservation(reservation_id:, reason: 'order_cancelled')
    instance = new
    instance.log_call(:release_reservation, {
                        reservation_id: reservation_id,
                        reason: reason
                      })

    default_response = {
      reservation_id: reservation_id,
      status: 'released',
      reason: reason,
      released_at: instance.generate_timestamp,
      stock_returned: true
    }

    instance.handle_response(:release_reservation, default_response)
  end

  # Update inventory levels
  # @param product_id [Integer] Product ID
  # @param adjustment [Integer] Stock adjustment (positive or negative)
  # @param reason [String] Reason for adjustment
  # @return [Hash] Update result
  def self.update_inventory(product_id:, adjustment:, reason: 'manual_adjustment')
    instance = new
    instance.log_call(:update_inventory, {
                        product_id: product_id,
                        adjustment: adjustment,
                        reason: reason
                      })

    default_response = {
      product_id: product_id,
      previous_stock: 100,
      adjustment: adjustment,
      new_stock: 100 + adjustment,
      reason: reason,
      updated_at: instance.generate_timestamp
    }

    instance.handle_response(:update_inventory, default_response)
  end

  # Get current inventory levels
  # @param product_ids [Array<Integer>] Product IDs to check
  # @return [Hash] Inventory levels
  def self.get_inventory_levels(product_ids:)
    instance = new
    instance.log_call(:get_inventory_levels, { product_ids: product_ids })

    # Generate stock levels for each product
    inventory_data = product_ids.map do |product_id|
      {
        product_id: product_id,
        stock_level: rand(50..200),
        reserved_quantity: rand(0..10),
        available_quantity: rand(40..190),
        reorder_point: 25,
        last_updated: instance.generate_timestamp
      }
    end

    default_response = {
      inventory: inventory_data,
      checked_at: instance.generate_timestamp
    }

    instance.handle_response(:get_inventory_levels, default_response)
  end

  # Check for low stock alerts
  # @param threshold [Integer] Stock level threshold for alerts
  # @return [Hash] Low stock products
  def self.check_low_stock(threshold: 25)
    instance = new
    instance.log_call(:check_low_stock, { threshold: threshold })

    # Simulate some low stock products
    low_stock_items = [
      {
        product_id: 101,
        current_stock: 15,
        threshold: threshold,
        recommended_reorder: 100
      },
      {
        product_id: 205,
        current_stock: 8,
        threshold: threshold,
        recommended_reorder: 75
      }
    ]

    default_response = {
      low_stock_items: low_stock_items,
      alert_count: low_stock_items.length,
      checked_at: instance.generate_timestamp
    }

    instance.handle_response(:check_low_stock, default_response)
  end

  # Bulk inventory operations
  # @param operations [Array<Hash>] List of inventory operations
  # @return [Hash] Bulk operation result
  def self.bulk_update(operations:)
    instance = new
    instance.log_call(:bulk_update, { operations: operations })

    # Process each operation
    results = operations.map.with_index do |operation, index|
      {
        operation_id: index + 1,
        product_id: operation[:product_id],
        operation_type: operation[:type],
        status: 'success',
        previous_stock: 100,
        new_stock: 100 + (operation[:adjustment] || 0)
      }
    end

    default_response = {
      batch_id: instance.generate_id('batch'),
      operations_processed: operations.length,
      successful_operations: results.length,
      failed_operations: 0,
      results: results,
      processed_at: instance.generate_timestamp
    }

    instance.handle_response(:bulk_update, default_response)
  end
end
