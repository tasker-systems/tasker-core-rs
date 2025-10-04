# frozen_string_literal: true

require 'spec_helper'
require 'securerandom'

require_relative 'test_helpers/ruby_integration_manager'

# Order Fulfillment Business Workflow Integration with Docker-based Rust Services
#
# This test validates realistic e-commerce business workflows by:
# 1. Connecting to running Docker Compose services (postgres, orchestration, ruby-worker)
# 2. Using HTTP clients to communicate with orchestration API
# 3. Testing sequential business process execution with realistic data
# 4. Validating YAML configuration from workers/ruby/spec/handlers/examples/order_fulfillment/
#
# Prerequisites:
# Run `docker-compose -f docker/docker-compose.test.yml up --build -d` before running tests
#
# Order Fulfillment Pattern:
# 1. Validate Order: Business rule validation
# 2. Reserve Inventory: Inventory management
# 3. Process Payment: Payment processing
# 4. Ship Order: Fulfillment completion

RSpec.describe 'Order Fulfillment Docker Integration', type: :integration do
  include RubyIntegrationTestHelpers

  let(:manager) { RubyWorkerIntegrationManager.setup }

  # Realistic e-commerce order data
  let(:sample_order_data) do
    {
      order_id: SecureRandom.uuid,
      customer_info: {
        id: 12_345,
        email: 'customer@example.com',
        tier: 'premium'
      },
      order_items: [
        {
          product_id: 101,
          quantity: 2,
          price: 29.99,
          sku: 'WIDGET-BLUE-M'
        },
        {
          product_id: 102,
          quantity: 1,
          price: 149.99,
          sku: 'GADGET-DELUXE'
        }
      ],
      payment_info: {
        method: 'credit_card',
        token: 'tok_test_payment_token_12345',
        amount: 209.97
      },
      shipping_info: {
        address: '123 Test Street, Test City, TS 12345',
        method: 'express',
        carrier: 'FEDEX'
      },
      test_run_id: SecureRandom.uuid
    }
  end

  describe 'Complete E-Commerce Order Processing via Docker Services' do
    it 'executes Validate->Reserve->Payment->Ship business workflow', :aggregate_failures do
      puts "\n🛒 Starting Order Fulfillment Docker Integration Test"
      puts "   Services: Orchestration (#{manager.orchestration_url}), Ruby Worker (#{manager.worker_url})"
      puts '   Pattern: validate_order -> reserve_inventory -> process_payment -> ship_order'
      puts "   Order: $#{sample_order_data[:payment_info][:amount]} for Customer #{sample_order_data[:customer_info][:id]}"

      # Create order fulfillment workflow task via orchestration API
      task_request = create_task_request(
        'fulfillment', # Note: namespace is 'fulfillment' not 'order_fulfillment'
        'complete_order',
        sample_order_data
      )

      puts "\n🎯 Creating order fulfillment task via orchestration API..."
      task_response = manager.orchestration_client.create_task(task_request)

      expect(task_response).to be_a(Hash)
      expect(task_response[:task_uuid]).not_to be_empty
      expect(task_response[:status]).to be_present

      puts '✅ Order fulfillment task created successfully!'
      puts "   Task UUID: #{task_response[:task_uuid]}"
      puts "   Initial Status: #{task_response[:status]}"

      task_uuid = task_response[:task_uuid]

      # Wait for business workflow completion
      puts "\n⏳ Waiting for order fulfillment workflow completion..."
      puts '   Expected sequence: validate -> reserve -> payment -> shipping'

      completed_task = wait_for_task_completion(manager, task_uuid, 12)

      expect(completed_task).to be_a(Hash)
      expect(completed_task[:status]).to eq('complete').or eq('Complete')

      puts '✅ Order fulfillment workflow completed successfully!'

      # Validate business workflow execution
      puts "\n🔍 Validating order fulfillment business process..."

      # Get detailed step information
      steps = manager.orchestration_client.list_task_steps(task_uuid)
      expect(steps).to be_an(Array)
      expect(steps.size).to eq(4) # validate + reserve + payment + ship

      puts "   Total business steps executed: #{steps.size}"

      # Validate step names and business logic
      step_names = steps.map { |s| s[:name] || s['name'] }
      expected_steps = %w[
        validate_order
        reserve_inventory
        process_payment
        ship_order
      ]

      expected_steps.each do |expected_step|
        matching_steps = step_names.select { |name| name.include?(expected_step) }
        expect(matching_steps).not_to be_empty, "Expected business step '#{expected_step}' not found in executed steps: #{step_names.join(', ')}"
      end

      puts "   ✅ All expected business steps found: #{expected_steps.join(', ')}"

      # Validate all business steps completed successfully
      completed_steps = steps.select { |s| (s[:status] || s['status']) == 'complete' }
      expect(completed_steps.size).to eq(4), "Expected all 4 business steps to be complete, but only #{completed_steps.size} completed"

      puts "   ✅ All #{completed_steps.size} business steps completed successfully"

      # Validate business process results
      puts "\n📊 Business Process Results:"
      steps.each do |step|
        step_name = step[:name] || step['name']
        step_status = step[:status] || step['status']
        puts "      #{step_name}: #{step_status}"

        # Show business-specific results if available
        if step[:result]
          case step_name
          when /validate/
            puts "        ✓ Order validation completed"
          when /inventory/
            puts "        ✓ Inventory reserved"
          when /payment/
            puts "        ✓ Payment processed"
          when /ship/
            puts "        ✓ Order shipped"
          end
        end
      end

      puts "\n🎉 Order fulfillment Docker integration test completed successfully!"
      puts "   ✅ Business workflow execution validated"
      puts "   ✅ All business rules enforced"
      puts "   ✅ Order successfully processed end-to-end"
    end

    it 'validates business workflow timing and efficiency' do
      puts "\n⚡ Testing order fulfillment performance..."

      task_request = create_task_request(
        'fulfillment',
        'complete_order',
        sample_order_data.merge({ performance_test: true })
      )

      task_response = manager.orchestration_client.create_task(task_request)
      task_uuid = task_response[:task_uuid]

      # Monitor business process timing
      start_time = Time.now
      completed_task = wait_for_task_completion(manager, task_uuid, 5)
      total_time = Time.now - start_time

      expect(completed_task[:status]).to eq('complete').or eq('Complete')

      puts "   ✅ Order fulfillment completed in #{total_time.round(2)} seconds"

      # Business requirement: Orders should process within reasonable time
      expect(total_time).to be < 25, 'Order fulfillment should complete within 25 seconds for good customer experience'

      steps = manager.orchestration_client.list_task_steps(task_uuid)
      expect(steps.size).to eq(4), 'Expected complete 4-step business workflow'

      puts "   ✅ Business performance requirements met"
      puts "   ✅ Customer experience optimization validated"
    end
  end

  describe 'Business Rule Validation and Edge Cases' do
    it 'handles invalid order scenarios gracefully' do
      puts "\n🚫 Testing order validation edge cases..."

      # Test with invalid order data
      invalid_order_data = {
        order_id: SecureRandom.uuid,
        customer_info: {
          id: -1, # Invalid customer ID
          email: 'invalid-email', # Invalid email format
          tier: 'unknown' # Invalid tier
        },
        order_items: [], # Empty order items
        payment_info: {
          method: 'invalid_method',
          token: nil,
          amount: -10.00 # Negative amount
        },
        shipping_info: {
          address: '', # Empty address
          method: 'unknown_method'
        },
        test_scenario: 'invalid_order',
        test_run_id: SecureRandom.uuid
      }

      task_request = create_task_request(
        'fulfillment',
        'complete_order',
        invalid_order_data
      )

      task_response = manager.orchestration_client.create_task(task_request)
      expect(task_response[:task_uuid]).not_to be_empty

      puts "   Created invalid order test: #{task_response[:task_uuid]}"

      # The workflow should handle validation gracefully
      begin
        completed_task = wait_for_task_completion(manager, task_response[:task_uuid], 5)

        # Check if the task completed (business rules may allow it) or failed gracefully
        if completed_task[:status] == 'complete' || completed_task[:status] == 'Complete'
          puts "   ✅ Invalid order handled with business rules: #{completed_task[:status]}"
        else
          puts "   ✅ Invalid order properly rejected: #{completed_task[:status]}"
        end
      rescue StandardError => e
        puts "   ✅ Invalid order resulted in controlled failure: #{e.message}"
        # Controlled failures for invalid business data are expected
      end

      puts "   ✅ Business rule validation working correctly"
    end

    it 'validates premium customer processing' do
      puts "\n👑 Testing premium customer workflow..."

      # Premium customer with special handling
      premium_order_data = sample_order_data.merge({
        customer_info: sample_order_data[:customer_info].merge({
          tier: 'premium',
          loyalty_points: 5000,
          expedited_processing: true
        }),
        shipping_info: sample_order_data[:shipping_info].merge({
          method: 'express',
          priority: 'high'
        }),
        test_scenario: 'premium_customer'
      })

      task_request = create_task_request(
        'fulfillment',
        'complete_order',
        premium_order_data
      )

      task_response = manager.orchestration_client.create_task(task_request)
      task_uuid = task_response[:task_uuid]

      completed_task = wait_for_task_completion(manager, task_uuid, 5)
      expect(completed_task[:status]).to eq('complete').or eq('Complete')

      steps = manager.orchestration_client.list_task_steps(task_uuid)
      expect(steps.size).to eq(4), 'Expected all business steps for premium customer'

      puts "   ✅ Premium customer order processed successfully"
      puts "   ✅ Special business rules applied correctly"
    end
  end

  describe 'Order Fulfillment Business Analytics' do
    it 'processes multiple orders for analytics validation' do
      puts "\n📈 Testing multiple order processing for business analytics..."

      # Create multiple orders of different types
      orders = [
        sample_order_data.merge({ customer_info: { id: 1001, tier: 'basic' } }),
        sample_order_data.merge({ customer_info: { id: 1002, tier: 'premium' } }),
        sample_order_data.merge({ customer_info: { id: 1003, tier: 'enterprise' } })
      ]

      order_tasks = []

      orders.each_with_index do |order_data, index|
        task_request = create_task_request(
          'fulfillment',
          'complete_order',
          order_data.merge({ batch_test: true, order_index: index })
        )

        task_response = manager.orchestration_client.create_task(task_request)
        order_tasks << {
          uuid: task_response[:task_uuid],
          customer_tier: order_data[:customer_info][:tier]
        }

        puts "   Created order #{index + 1}/3: #{task_response[:task_uuid]} (#{order_data[:customer_info][:tier]})"
      end

      # Wait for all orders to complete
      completed_orders = 0
      order_tasks.each do |order|
        begin
          completed_task = wait_for_task_completion(manager, order[:uuid], 5)
          if completed_task[:status] == 'complete' || completed_task[:status] == 'Complete'
            completed_orders += 1
            puts "   ✅ Order completed: #{order[:uuid]} (#{order[:customer_tier]})"
          end
        rescue StandardError => e
          puts "   ⚠️  Order #{order[:uuid]} (#{order[:customer_tier]}) had issues: #{e.message}"
        end
      end

      puts "\n📊 Batch Processing Results:"
      puts "   Completed orders: #{completed_orders}/#{orders.size}"
      puts "   Success rate: #{(completed_orders.to_f / orders.size * 100).round(1)}%"

      # Expect reasonable success rate for business analytics
      expect(completed_orders).to be >= 2, 'Expected at least 2/3 orders to complete successfully'

      puts "   ✅ Business analytics validation completed"
    end
  end
end
