# frozen_string_literal: true

# Blog Examples Spec Helper
# Provides common utilities and setup for blog example tests

# Require mock services
require_relative 'mock_services/base_mock_service'
require_relative 'mock_services/payment_service'
require_relative 'mock_services/email_service'
require_relative 'mock_services/inventory_service'

module BlogExampleHelpers
  # Reset all mock services before each test
  def reset_mock_services!
    MockPaymentService.reset!
    MockEmailService.reset!
    MockInventoryService.reset!
  end

  # Build sample e-commerce context for testing
  def sample_ecommerce_context
    {
      'cart_items' => [
        {
          'product_id' => 1,
          'quantity' => 2,
          'price' => 29.99,
          'name' => 'Widget'
        },
        {
          'product_id' => 2,
          'quantity' => 1,
          'price' => 49.99,
          'name' => 'Gadget'
        }
      ],
      'payment_info' => {
        'method' => 'credit_card',
        'token' => 'tok_test_123456',
        'amount' => 109.97,
        'currency' => 'USD',
        'card_last_four' => '4242'
      },
      'customer_info' => {
        'id' => 123,
        'email' => 'customer@example.com',
        'name' => 'Test Customer',
        'tier' => 'standard',
        'phone' => '555-1234'
      },
      'shipping_info' => {
        'address' => '123 Main St',
        'city' => 'San Francisco',
        'state' => 'CA',
        'zip' => '94102'
      },
      'priority' => 'standard'
    }
  end

  # Build premium customer context
  def premium_customer_context
    sample_ecommerce_context.merge(
      'customer_info' => sample_ecommerce_context['customer_info'].merge(
        'tier' => 'premium'
      )
    )
  end

  # Build express order context
  def express_order_context
    sample_ecommerce_context.merge(
      'priority' => 'express'
    )
  end

  # Verify payment service was called correctly
  def verify_payment_processing(amount: 109.97, method: 'credit_card')
    expect(MockPaymentService.called?(:process_payment)).to be true
    last_payment = MockPaymentService.last_call(:process_payment)
    expect(last_payment[:args][:amount]).to eq(amount)
    expect(last_payment[:args][:method]).to eq(method)
  end

  # Verify email service was called correctly
  def verify_email_delivery(to: 'customer@example.com', template: 'order_confirmation')
    expect(MockEmailService.called?(:send_confirmation)).to be true
    last_email = MockEmailService.last_call(:send_confirmation)
    expect(last_email[:args][:to]).to eq(to)
    expect(last_email[:args][:template]).to eq(template)
  end

  # Verify inventory service was called correctly
  def verify_inventory_management(product_ids: [1, 2])
    expect(MockInventoryService.called?(:reserve_inventory)).to be true

    # Check that inventory was reserved for expected products
    product_ids.each do |product_id|
      reservations = MockInventoryService.calls_for(:reserve_inventory)
      reservation = reservations.find { |call| call[:args][:product_id] == product_id }
      expect(reservation).to be_present
    end
  end

  # Generate a test task UUID
  def generate_test_task_uuid
    require 'securerandom'
    SecureRandom.uuid
  end

  # Deep symbolize keys for hash (Rails compatibility helper)
  def deep_symbolize_keys(obj)
    case obj
    when Hash
      obj.each_with_object({}) do |(key, value), result|
        result[key.to_sym] = deep_symbolize_keys(value)
      end
    when Array
      obj.map { |item| deep_symbolize_keys(item) }
    else
      obj
    end
  end

  # Deep stringify keys for hash (Rails compatibility helper)
  def deep_stringify_keys(obj)
    case obj
    when Hash
      obj.each_with_object({}) do |(key, value), result|
        result[key.to_s] = deep_stringify_keys(value)
      end
    when Array
      obj.map { |item| deep_stringify_keys(item) }
    else
      obj
    end
  end
end

# Include helpers in RSpec if it's loaded
if defined?(RSpec)
  RSpec.configure do |config|
    config.include BlogExampleHelpers

    # Reset mock services before each test
    config.before(:each) do
      reset_mock_services!
    end
  end
end
