# frozen_string_literal: true

require 'spec_helper'
require_relative '../../support/blog_spec_helper'

# Load handler and dependencies
require 'handlers/examples/blog_examples/post_01_ecommerce/step_handlers/update_inventory_handler'
require 'handlers/examples/blog_examples/post_01_ecommerce/models/product'
require 'blog_examples/support/mock_services/inventory_service'

RSpec.describe Ecommerce::StepHandlers::UpdateInventoryHandler do
  include BlogExampleHelpers

  let(:handler) { described_class.new }

  # Build test task with context
  let(:task) do
    double('Task',
           task_uuid: generate_test_task_uuid,
           context: task_context)
  end

  # Build test sequence with cart validation results
  let(:sequence) do
    double('Sequence',
           task_uuid: task.task_uuid,
           get_results: cart_validation_result)
  end

  # Build test step
  let(:step) do
    double('Step',
           name: 'update_inventory',
           task_uuid: task.task_uuid)
  end

  # Default cart validation result (from previous validate_cart step)
  let(:cart_validation_result) do
    {
      'validated_items' => [
        {
          'product_id' => 1,
          'name' => 'Widget A',
          'price' => 29.99,
          'quantity' => 2,
          'line_total' => 59.98
        },
        {
          'product_id' => 2,
          'name' => 'Gadget B',
          'price' => 49.99,
          'quantity' => 1,
          'line_total' => 49.99
        }
      ],
      'subtotal' => 109.97,
      'total' => 124.76
    }
  end

  # Default valid task context with customer info
  let(:task_context) do
    {
      'customer_info' => {
        'email' => 'customer@example.com',
        'name' => 'Test Customer',
        'id' => 'cust_12345'
      }
    }
  end

  before do
    # Reset mock service before each test
    MockInventoryService.reset!
  end

  describe '#call' do
    context 'with valid inventory update' do
      it 'updates inventory successfully' do
        result = handler.call(task, sequence, step)

        expect(result).to be_success
        expect(result.result[:updated_products]).to be_an(Array)
        expect(result.result[:updated_products].length).to eq(2)
        expect(result.result[:total_items_reserved]).to eq(3) # 2 + 1
      end

      it 'includes inventory changes' do
        result = handler.call(task, sequence, step)

        inventory_changes = result.result[:inventory_changes]
        expect(inventory_changes).to be_an(Array)
        expect(inventory_changes.length).to eq(2)
        expect(inventory_changes.all? { |c| c[:change_type] == 'reservation' }).to be true
      end

      it 'includes updated product details' do
        result = handler.call(task, sequence, step)

        first_product = result.result[:updated_products][0]
        expect(first_product[:product_id]).to eq(1)
        expect(first_product[:name]).to eq('Widget A')
        expect(first_product[:quantity_reserved]).to eq(2)
        expect(first_product[:reservation_id]).to be_present

        second_product = result.result[:updated_products][1]
        expect(second_product[:product_id]).to eq(2)
        expect(second_product[:name]).to eq('Gadget B')
        expect(second_product[:quantity_reserved]).to eq(1)
      end

      it 'includes stock level changes' do
        result = handler.call(task, sequence, step)

        first_product = result.result[:updated_products][0]
        expect(first_product[:previous_stock]).to be_present
        expect(first_product[:new_stock]).to be_present
        expect(first_product[:new_stock]).to eq(first_product[:previous_stock] - 2)
      end

      it 'includes execution metadata' do
        result = handler.call(task, sequence, step)

        expect(result.metadata[:operation]).to eq('update_inventory')
        expect(result.metadata[:execution_hints]).to be_present
        expect(result.metadata[:execution_hints][:products_updated]).to eq(2)
        expect(result.metadata[:execution_hints][:total_items_reserved]).to eq(3)
      end

      it 'includes HTTP headers in metadata' do
        result = handler.call(task, sequence, step)

        headers = result.metadata[:http_headers]
        expect(headers['X-Inventory-Service']).to eq('MockInventoryService')
        expect(headers['X-Reservation-Count']).to eq('2')
        expect(headers['X-Total-Quantity']).to eq('3')
      end

      it 'includes input references in metadata' do
        result = handler.call(task, sequence, step)

        input_refs = result.metadata[:input_refs]
        expect(input_refs[:validated_items]).to eq('sequence.validate_cart.result.validated_items')
        expect(input_refs[:customer_info]).to eq('task.context.customer_info')
      end

      it 'includes timestamp' do
        result = handler.call(task, sequence, step)

        expect(result.result[:updated_at]).to be_present
        expect(result.result[:updated_at]).to match(/\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z/)
      end

      it 'calls MockInventoryService with correct parameters' do
        handler.call(task, sequence, step)

        # Check availability was called for both products
        expect(MockInventoryService.called?(:check_availability)).to be true
        expect(MockInventoryService.call_count(:check_availability)).to eq(2)

        # Check reserve_inventory was called for both products
        expect(MockInventoryService.called?(:reserve_inventory)).to be true
        expect(MockInventoryService.call_count(:reserve_inventory)).to eq(2)

        # Verify parameters for first product
        first_reservation = MockInventoryService.calls_for(:reserve_inventory)[0]
        expect(first_reservation[:args][:product_id]).to eq(1)
        expect(first_reservation[:args][:quantity]).to eq(2)
        expect(first_reservation[:args][:customer_id]).to eq('cust_12345')
      end
    end

    context 'with single item' do
      let(:cart_validation_result) do
        {
          'validated_items' => [
            { 'product_id' => 1, 'name' => 'Widget A', 'quantity' => 1 }
          ]
        }
      end

      it 'updates inventory for single item' do
        result = handler.call(task, sequence, step)

        expect(result).to be_success
        expect(result.result[:updated_products].length).to eq(1)
        expect(result.result[:total_items_reserved]).to eq(1)
      end
    end

    context 'with missing validated items' do
      let(:cart_validation_result) { nil }

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Validated cart items are required')
          expect(error.error_code).to eq('MISSING_VALIDATED_ITEMS')
        end
      end
    end

    context 'with empty validated items' do
      let(:cart_validation_result) do
        { 'validated_items' => [] }
      end

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Validated cart items are required')
        end
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

    context 'with non-existent product' do
      let(:cart_validation_result) do
        {
          'validated_items' => [
            { 'product_id' => 999999, 'name' => 'Unknown Product', 'quantity' => 1 }
          ]
        }
      end

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Product 999999 not found')
          expect(error.error_code).to eq('PRODUCT_NOT_FOUND')
        end
      end
    end

    context 'with stock unavailable' do
      before do
        MockInventoryService.stub_response(:check_availability, {
          available: false,
          stock_level: 1,
          product_id: 1
        })
      end

      it 'raises retryable error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::RetryableError) do |error|
          expect(error.message).to include('Stock not available')
          expect(error.message).to include('Available: 1, Needed: 2')
          expect(error.retry_after).to eq(30)
          expect(error.context[:product_name]).to eq('Widget A')
          expect(error.context[:available_stock]).to eq(1)
          expect(error.context[:requested_quantity]).to eq(2)
        end
      end
    end

    context 'with insufficient stock status' do
      before do
        # Let availability pass but reservation fail
        MockInventoryService.stub_response(:reserve_inventory, {
          status: 'insufficient_stock',
          message: 'Not enough stock to reserve'
        })
      end

      it 'raises retryable error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::RetryableError) do |error|
          expect(error.message).to include('Insufficient stock')
          expect(error.retry_after).to eq(30)
          expect(error.context[:status]).to eq('insufficient_stock')
        end
      end
    end

    context 'with reservation failed status' do
      before do
        MockInventoryService.stub_response(:reserve_inventory, {
          status: 'reservation_failed',
          message: 'Reservation system error'
        })
      end

      it 'raises retryable error with shorter backoff' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::RetryableError) do |error|
          expect(error.message).to include('Failed to reserve inventory')
          expect(error.retry_after).to eq(15)
          expect(error.context[:status]).to eq('reservation_failed')
        end
      end
    end

    context 'with unknown reservation status' do
      before do
        MockInventoryService.stub_response(:reserve_inventory, {
          status: 'unknown_error',
          message: 'Something went wrong'
        })
      end

      it 'raises retryable error for safety' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::RetryableError) do |error|
          expect(error.message).to include('Unknown reservation status')
          expect(error.message).to include('unknown_error')
          expect(error.retry_after).to eq(30)
          expect(error.context[:status]).to eq('unknown_error')
        end
      end
    end

    context 'with MockInventoryService failure' do
      before do
        MockInventoryService.stub_failure(:check_availability,
                                         MockInventoryService::InventoryError,
                                         'Service unavailable')
      end

      it 'raises the service error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(MockInventoryService::InventoryError) do |error|
          expect(error.message).to include('Service unavailable')
        end
      end
    end

    context 'with symbolized keys in context' do
      let(:task_context) do
        {
          customer_info: {
            email: 'customer@example.com',
            name: 'Test Customer',
            id: 'cust_12345'
          }
        }
      end

      it 'handles symbolized keys correctly' do
        result = handler.call(task, sequence, step)
        expect(result).to be_success
        expect(result.result[:total_items_reserved]).to eq(3)
      end
    end

    context 'with symbolized keys in cart validation result' do
      let(:cart_validation_result) do
        {
          validated_items: [
            { product_id: 1, name: 'Widget A', quantity: 2 }
          ]
        }
      end

      it 'handles symbolized cart results correctly' do
        result = handler.call(task, sequence, step)
        expect(result).to be_success
        expect(result.result[:updated_products].length).to eq(1)
      end
    end

    context 'with mixed string and symbol keys' do
      let(:cart_validation_result) do
        {
          'validated_items' => [
            { product_id: 1, 'name' => 'Widget A', quantity: 2 }
          ]
        }
      end

      it 'normalizes keys correctly' do
        result = handler.call(task, sequence, step)
        expect(result).to be_success
      end
    end

    context 'with customer email but no id' do
      let(:task_context) do
        {
          'customer_info' => {
            'email' => 'customer@example.com',
            'name' => 'Test Customer'
          }
        }
      end

      it 'uses email as customer identifier' do
        result = handler.call(task, sequence, step)

        expect(result).to be_success
        # Verify the service was called with email as customer_id
        first_reservation = MockInventoryService.calls_for(:reserve_inventory)[0]
        expect(first_reservation[:args][:customer_id]).to eq('customer@example.com')
      end
    end
  end

  describe 'inventory change tracking' do
    it 'records reservation changes for each product' do
      result = handler.call(task, sequence, step)

      inventory_changes = result.result[:inventory_changes]
      expect(inventory_changes.length).to eq(2)

      first_change = inventory_changes[0]
      expect(first_change[:product_id]).to eq(1)
      expect(first_change[:change_type]).to eq('reservation')
      expect(first_change[:quantity]).to eq(-2)
      expect(first_change[:reason]).to eq('order_checkout')
      expect(first_change[:timestamp]).to be_present
      expect(first_change[:reservation_id]).to be_present
    end
  end

  describe 'deep_symbolize_keys helper' do
    it 'symbolizes nested hash keys' do
      result = handler.call(task, sequence, step)
      # Implicitly tested through successful processing with mixed key types
      expect(result).to be_success
    end

    it 'symbolizes array elements' do
      # Tested through cart_validation_result handling
      result = handler.call(task, sequence, step)
      expect(result).to be_success
    end
  end
end
