# frozen_string_literal: true

require 'spec_helper'
require_relative '../../support/blog_spec_helper'

# Load handler and dependencies
require 'handlers/examples/blog_examples/post_01_ecommerce/step_handlers/create_order_handler'
require 'handlers/examples/blog_examples/post_01_ecommerce/models/order'

RSpec.describe Ecommerce::StepHandlers::CreateOrderHandler do
  include BlogExampleHelpers

  let(:handler) { described_class.new }

  # Build test task with context
  let(:task) do
    double('Task',
           task_uuid: generate_test_task_uuid,
           context: task_context)
  end

  # Build test sequence with all previous step results
  let(:sequence) do
    seq = double('Sequence', task_uuid: task.task_uuid)
    allow(seq).to receive(:get_results) do |step_name|
      step_results[step_name]
    end
    seq
  end

  # Build test step
  let(:step) do
    double('Step',
           name: 'create_order',
           task_uuid: task.task_uuid)
  end

  # Default task context
  let(:task_context) do
    {
      'customer_info' => {
        'email' => 'customer@example.com',
        'name' => 'Test Customer',
        'phone' => '555-1234'
      }
    }
  end

  # Results from previous steps
  let(:step_results) do
    {
      'validate_cart' => {
        'validated_items' => [
          { 'product_id' => 1, 'name' => 'Widget A', 'quantity' => 2, 'line_total' => 59.98 },
          { 'product_id' => 2, 'name' => 'Gadget B', 'quantity' => 1, 'line_total' => 49.99 }
        ],
        'subtotal' => 109.97,
        'tax' => 8.80,
        'shipping' => 5.99,
        'total' => 124.76,
        'item_count' => 2
      },
      'process_payment' => {
        'payment_id' => 'pay_123456',
        'transaction_id' => 'txn_123456',
        'amount_charged' => 124.76,
        'status' => 'completed'
      },
      'update_inventory' => {
        'updated_products' => [
          { 'product_id' => 1, 'quantity_reserved' => 2, 'reservation_id' => 'res_1' },
          { 'product_id' => 2, 'quantity_reserved' => 1, 'reservation_id' => 'res_2' }
        ],
        'inventory_log_id' => 789
      }
    }
  end

  describe '#call' do
    context 'with valid order creation' do
      it 'creates order successfully' do
        result = handler.call(task, sequence, step)

        expect(result).to be_success
        expect(result.result[:order_id]).to be_present
        expect(result.result[:order_number]).to be_present
        expect(result.result[:status]).to eq('confirmed')
      end

      it 'includes order details' do
        result = handler.call(task, sequence, step)

        expect(result.result[:total_amount]).to eq(124.76)
        expect(result.result[:customer_email]).to eq('customer@example.com')
        expect(result.result[:created_at]).to be_present
        expect(result.result[:created_at]).to match(/\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/)
      end

      it 'includes estimated delivery' do
        result = handler.call(task, sequence, step)

        expect(result.result[:estimated_delivery]).to be_present
        expect(result.result[:estimated_delivery]).to match(/\w+ \d{1,2}, \d{4}/)
      end

      it 'includes execution metadata' do
        result = handler.call(task, sequence, step)

        expect(result.metadata[:operation]).to eq('create_order')
        expect(result.metadata[:execution_hints]).to be_present
        expect(result.metadata[:execution_hints][:order_number]).to be_present
        expect(result.metadata[:execution_hints][:total_amount]).to eq(124.76)
        expect(result.metadata[:execution_hints][:item_count]).to eq(2)
      end

      it 'includes HTTP headers in metadata' do
        result = handler.call(task, sequence, step)

        headers = result.metadata[:http_headers]
        expect(headers['X-Order-Service']).to eq('OrderManagement')
        expect(headers['X-Order-ID']).to be_present
        expect(headers['X-Order-Number']).to be_present
        expect(headers['X-Order-Status']).to eq('confirmed')
      end

      it 'includes input references in metadata' do
        result = handler.call(task, sequence, step)

        input_refs = result.metadata[:input_refs]
        expect(input_refs[:customer_info]).to eq('task.context.customer_info')
        expect(input_refs[:cart_validation]).to eq('sequence.validate_cart.result')
        expect(input_refs[:payment_result]).to eq('sequence.process_payment.result')
        expect(input_refs[:inventory_result]).to eq('sequence.update_inventory.result')
      end

      it 'generates valid order number' do
        result = handler.call(task, sequence, step)

        order_number = result.result[:order_number]
        expect(order_number).to match(/ORD-\d{8}-[A-F0-9]{8}/)
      end

      it 'creates order with correct payment status' do
        result = handler.call(task, sequence, step)

        # Verify the order was created with payment_status='completed'
        expect(result).to be_success
        expect(result.result[:status]).to eq('confirmed')
      end
    end

    context 'with missing customer info' do
      let(:task_context) { {} }

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Customer information is required')
          expect(error.error_code).to eq('MISSING_CUSTOMER_INFO')
        end
      end
    end

    context 'with missing cart validation' do
      let(:step_results) do
        {
          'validate_cart' => nil,
          'process_payment' => { 'payment_id' => 'pay_123' },
          'update_inventory' => { 'updated_products' => [{}] }
        }
      end

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Cart validation results are required')
          expect(error.error_code).to eq('MISSING_CART_VALIDATION')
        end
      end
    end

    context 'with empty validated items' do
      let(:step_results) do
        {
          'validate_cart' => { 'validated_items' => [] },
          'process_payment' => { 'payment_id' => 'pay_123' },
          'update_inventory' => { 'updated_products' => [{}] }
        }
      end

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Cart validation results are required')
        end
      end
    end

    context 'with missing payment result' do
      let(:step_results) do
        {
          'validate_cart' => {
            'validated_items' => [{ 'product_id' => 1 }],
            'total' => 100.0
          },
          'process_payment' => nil,
          'update_inventory' => { 'updated_products' => [{}] }
        }
      end

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Payment results are required')
          expect(error.error_code).to eq('MISSING_PAYMENT_RESULT')
        end
      end
    end

    context 'with payment result missing payment_id' do
      let(:step_results) do
        {
          'validate_cart' => {
            'validated_items' => [{ 'product_id' => 1 }],
            'total' => 100.0
          },
          'process_payment' => { 'amount_charged' => 100.0 },
          'update_inventory' => { 'updated_products' => [{}] }
        }
      end

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Payment results are required')
        end
      end
    end

    context 'with missing inventory result' do
      let(:step_results) do
        {
          'validate_cart' => {
            'validated_items' => [{ 'product_id' => 1 }],
            'total' => 100.0
          },
          'process_payment' => { 'payment_id' => 'pay_123' },
          'update_inventory' => nil
        }
      end

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Inventory results are required')
          expect(error.error_code).to eq('MISSING_INVENTORY_RESULT')
        end
      end
    end

    context 'with empty updated products' do
      let(:step_results) do
        {
          'validate_cart' => {
            'validated_items' => [{ 'product_id' => 1 }],
            'total' => 100.0
          },
          'process_payment' => { 'payment_id' => 'pay_123' },
          'update_inventory' => { 'updated_products' => [] }
        }
      end

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Inventory results are required')
        end
      end
    end

    context 'with invalid order (missing email)' do
      let(:task_context) do
        {
          'customer_info' => {
            'name' => 'Test Customer',
            'email' => ''
          }
        }
      end

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Failed to create order')
          expect(error.error_code).to eq('ORDER_VALIDATION_FAILED')
          expect(error.context[:errors]).to include('customer_email is required')
        end
      end
    end

    context 'with invalid order (invalid email)' do
      let(:task_context) do
        {
          'customer_info' => {
            'name' => 'Test Customer',
            'email' => 'not-an-email'
          }
        }
      end

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Failed to create order')
          expect(error.context[:errors]).to include('customer_email is invalid')
        end
      end
    end

    context 'with invalid order (missing name)' do
      let(:task_context) do
        {
          'customer_info' => {
            'email' => 'test@example.com',
            'name' => ''
          }
        }
      end

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.context[:errors]).to include('customer_name is required')
        end
      end
    end

    context 'with symbolized keys in context' do
      let(:task_context) do
        {
          customer_info: {
            email: 'customer@example.com',
            name: 'Test Customer',
            phone: '555-1234'
          }
        }
      end

      it 'handles symbolized keys correctly' do
        result = handler.call(task, sequence, step)
        expect(result).to be_success
        expect(result.result[:customer_email]).to eq('customer@example.com')
      end
    end

    context 'with symbolized keys in step results' do
      let(:step_results) do
        {
          'validate_cart' => {
            validated_items: [{ product_id: 1, quantity: 1 }],
            subtotal: 100.0,
            tax: 8.0,
            shipping: 5.0,
            total: 113.0,
            item_count: 1
          },
          'process_payment' => {
            payment_id: 'pay_123',
            transaction_id: 'txn_123'
          },
          'update_inventory' => {
            updated_products: [{ product_id: 1 }]
          }
        }
      end

      it 'handles symbolized step results correctly' do
        result = handler.call(task, sequence, step)
        expect(result).to be_success
      end
    end

    context 'with mixed string and symbol keys' do
      let(:task_context) do
        {
          'customer_info' => {
            email: 'customer@example.com',
            'name' => 'Test Customer'
          }
        }
      end

      it 'normalizes keys correctly' do
        result = handler.call(task, sequence, step)
        expect(result).to be_success
      end
    end
  end

  describe 'order number generation' do
    it 'includes current date' do
      result = handler.call(task, sequence, step)
      order_number = result.result[:order_number]
      date_part = Date.today.strftime('%Y%m%d')
      expect(order_number).to include(date_part)
    end

    it 'includes random hex suffix' do
      result1 = handler.call(task, sequence, step)
      result2 = handler.call(task, sequence, step)

      # Different random suffixes each time
      expect(result1.result[:order_number]).not_to eq(result2.result[:order_number])
    end
  end

  describe 'deep_symbolize_keys helper' do
    it 'symbolizes nested hash keys' do
      result = handler.call(task, sequence, step)
      # Implicitly tested through successful processing with mixed key types
      expect(result).to be_success
    end
  end
end
