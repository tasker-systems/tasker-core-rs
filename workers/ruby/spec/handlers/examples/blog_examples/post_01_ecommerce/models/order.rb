# frozen_string_literal: true

require 'json'

# Demo Order model for the e-commerce example
# Simplified PORO without ActiveModel dependencies
module Ecommerce
  class Order
    attr_accessor :id, :customer_email, :customer_name, :customer_phone,
                  :subtotal, :tax_amount, :shipping_amount, :total_amount,
                  :payment_id, :payment_status, :transaction_id,
                  :items, :item_count, :inventory_log_id,
                  :status, :order_number, :placed_at,
                  :task_uuid, :workflow_version,
                  :created_at, :updated_at

    def initialize(attributes = {})
      @id = attributes[:id] || self.class.next_id
      @customer_email = attributes[:customer_email]
      @customer_name = attributes[:customer_name]
      @customer_phone = attributes[:customer_phone]
      @subtotal = attributes[:subtotal]
      @tax_amount = attributes[:tax_amount] || 0
      @shipping_amount = attributes[:shipping_amount] || 0
      @total_amount = attributes[:total_amount]
      @payment_id = attributes[:payment_id]
      @payment_status = attributes[:payment_status] || 'pending'
      @transaction_id = attributes[:transaction_id]
      @items = attributes[:items] # JSON string or array
      @item_count = attributes[:item_count] || 0
      @inventory_log_id = attributes[:inventory_log_id]
      @status = attributes[:status] || 'pending'
      @order_number = attributes[:order_number]
      @placed_at = attributes[:placed_at]
      @task_uuid = attributes[:task_uuid]
      @workflow_version = attributes[:workflow_version]
      @created_at = attributes[:created_at] || Time.now
      @updated_at = attributes[:updated_at] || Time.now
    end

    # Validations
    def valid?
      errors.clear

      errors << 'customer_email is required' if customer_email.to_s.strip.empty?
      errors << 'customer_email is invalid' unless valid_email?(customer_email)
      errors << 'customer_name is required' if customer_name.to_s.strip.empty?
      errors << 'total_amount must be greater than 0' unless total_amount.to_f > 0
      errors << 'order_number is required' if order_number.to_s.strip.empty?
      errors << 'status is required' if status.to_s.strip.empty?

      errors.empty?
    end

    def errors
      @errors ||= []
    end

    # Status methods
    def pending?
      status == 'pending'
    end

    def confirmed?
      status == 'confirmed'
    end

    def processing?
      status == 'processing'
    end

    def shipped?
      status == 'shipped'
    end

    def delivered?
      status == 'delivered'
    end

    def cancelled?
      status == 'cancelled'
    end

    def refunded?
      status == 'refunded'
    end

    # Payment status methods
    def payment_pending?
      payment_status == 'pending'
    end

    def payment_completed?
      payment_status == 'completed'
    end

    def payment_failed?
      payment_status == 'failed'
    end

    def payment_refunded?
      payment_status == 'refunded'
    end

    # Utility methods
    def items_array
      return [] if items.nil? || items.to_s.strip.empty?

      if items.is_a?(String)
        JSON.parse(items)
      elsif items.is_a?(Array)
        items
      else
        []
      end
    rescue JSON::ParserError
      []
    end

    def items_array=(value)
      self.items = value.to_json
      self.item_count = value.sum { |item| (item['quantity'] || item[:quantity] || 0).to_i }
    end

    def formatted_total
      '$%.2f' % total_amount
    end

    def formatted_order_number
      order_number
    end

    # Simulate ID generation for demo purposes
    def self.next_id
      @next_id ||= 1000
      @next_id += 1
    end

    private

    def valid_email?(email)
      return false if email.nil? || email.to_s.strip.empty?

      # Simple email regex
      email.to_s.match?(/\A[^@\s]+@[^@\s]+\.[^@\s]+\z/)
    end
  end
end
