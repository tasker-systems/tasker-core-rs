# frozen_string_literal: true

require 'spec_helper'
require 'yaml'
require 'timeout'

require_relative 'test_helpers/shared_test_loop'

require_relative '../handlers/examples/order_fulfillment/handlers/order_fulfillment_handler'
require_relative '../handlers/examples/order_fulfillment/step_handlers/validate_order_handler'
require_relative '../handlers/examples/order_fulfillment/step_handlers/reserve_inventory_handler'
require_relative '../handlers/examples/order_fulfillment/step_handlers/process_payment_handler'
require_relative '../handlers/examples/order_fulfillment/step_handlers/ship_order_handler'

RSpec.describe 'Order Fulfillment PGMQ Integration', type: :integration do
  let(:config_path) do
    File.expand_path('../../handlers/examples/order_fulfillment/config/order_fulfillment_handler.yaml', __dir__)
  end
  let(:task_config) { YAML.load_file(config_path) }

  # Sample order data that represents a real e-commerce order
  let(:sample_order_data) do
    {
      customer_info: {
        id: 12_345,
        email: 'customer@example.com',
        tier: 'premium'
      },
      order_items: [
        {
          product_id: 101,
          quantity: 2,
          price: 29.99
        },
        {
          product_id: 102,
          quantity: 1,
          price: 149.99
        }
      ],
      payment_info: {
        method: 'credit_card',
        token: 'tok_test_payment_token_12345',
        amount: 209.97
      },
      shipping_info: {
        address: '123 Test Street, Test City, TS 12345',
        method: 'express'
      }
    }
  end

  let(:namespace) { 'fulfillment' }
  let(:shared_loop) { SharedTestLoop.new }

  before do
    shared_loop.start
  end

  after do
    shared_loop.stop
  end

  describe 'Complete Order Fulfillment Workflow' do
    it 'executes full order fulfillment through pgmq architecture', :aggregate_failures do
      task_request = TaskerCore::Types::TaskRequest.new(
        namespace: 'fulfillment',
        name: 'process_order',
        version: '1.0.0',
        context: sample_order_data.merge({ order_id: SecureRandom.uuid }),
        initiator: 'integration_test',
        source_system: 'pgmq_integration_spec',
        reason: 'Full integration test of order fulfillment workflow',
        priority: 5,
        claim_timeout_seconds: 300
      )

      task = shared_loop.run(task_request: task_request, num_workers: 2, namespace: namespace)
      expect(task).not_to be_nil

      # Validate all 4 steps completed in order
      expect(task.workflow_steps.count).to eq(4) # validate, reserve, payment, ship

      task.workflow_steps.each do |step|
        results = JSON.parse(step.results)
        expect(results).to be_a(Hash)
        expect(results.keys).to include('result')

        # Validate specific step results
        case step.name
        when 'validate_order'
          expect(results['result']).to include('customer_validated' => true)
        when 'reserve_inventory'
          expect(results['result']).to include('items_reserved' => 2)
        when 'process_payment'
          expect(results['result']).to include('payment_processed' => true)
        when 'ship_order'
          expect(results['result']).to include('shipping_status' => 'label_created')
        end
      end
    end
  end
end
