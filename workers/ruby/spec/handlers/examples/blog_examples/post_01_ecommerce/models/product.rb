# frozen_string_literal: true

# Demo Product model for the e-commerce example
# Simplified PORO without ActiveModel dependencies
module Ecommerce
  class Product
    attr_accessor :id, :name, :description, :price, :stock, :active, :sku, :category, :weight, :created_at, :updated_at

    def initialize(attributes = {})
      @id = attributes[:id]
      @name = attributes[:name]
      @description = attributes[:description]
      @price = attributes[:price]
      @stock = attributes[:stock] || 0
      @active = attributes.key?(:active) ? attributes[:active] : true
      @sku = attributes[:sku]
      @category = attributes[:category]
      @weight = attributes[:weight]
      @created_at = attributes[:created_at] || Time.now
      @updated_at = attributes[:updated_at] || Time.now
    end

    # Class methods for finding products (mock implementations)
    def self.find(id)
      find_by(id: id)
    end

    def self.find_by(attributes)
      # Return mock products that match the test context expectations
      return unless attributes[:id]

      case attributes[:id]
      when 1
        new(
          id: 1,
          name: 'Widget A',
          price: 29.99,
          stock: 100,
          active: true,
          sku: 'WIDGET-A'
        )
      when 2
        new(
          id: 2,
          name: 'Gadget B',
          price: 49.99,
          stock: 100,
          active: true,
          sku: 'GADGET-B'
        )
      when 3
        new(
          id: 3,
          name: 'Premium Item',
          price: 99.99,
          stock: 50,
          active: true,
          sku: 'PREMIUM-001'
        )
      when 4..100
        # Return mock products for IDs 4-100
        new(
          id: attributes[:id],
          name: "Mock Product #{attributes[:id]}",
          price: 29.99,
          stock: 100,
          active: true,
          sku: "MOCK-#{attributes[:id]}"
        )
      else
        # Return nil for IDs > 100 (simulates product not found)
        nil
      end
    end

    # Instance methods
    def active?
      active
    end

    def in_stock?
      stock > 0
    end

    def sufficient_stock?(quantity)
      stock >= quantity
    end

    # Validation helper
    def valid?
      name.to_s.strip != '' && price.to_f > 0 && stock.to_i >= 0
    end
  end
end
