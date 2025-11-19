# frozen_string_literal: true

require 'spec_helper'
require_relative '../../support/blog_spec_helper'

# Load handler and model files using require since spec/ is in load path
require 'handlers/examples/blog_examples/post_01_ecommerce/step_handlers/validate_cart_handler'
require 'handlers/examples/blog_examples/post_01_ecommerce/models/product'

RSpec.describe Ecommerce::StepHandlers::ValidateCartHandler do
  include BlogExampleHelpers

  let(:handler) { described_class.new }

  # Build test task with context
  let(:task) do
    double('Task',
           task_uuid: generate_test_task_uuid,
           context: task_context)
  end

  # Build test sequence (minimal for this step)
  let(:sequence) do
    double('Sequence',
           task_uuid: task.task_uuid,
           get_results: nil)
  end

  # Build test step
  let(:step) do
    double('Step',
           name: 'validate_cart',
           task_uuid: task.task_uuid)
  end

  # Default valid task context
  let(:task_context) do
    {
      'cart_items' => [
        {
          'product_id' => 1,
          'quantity' => 2,
          'price' => 29.99
        },
        {
          'product_id' => 2,
          'quantity' => 1,
          'price' => 49.99
        }
      ],
      'customer_info' => {
        'email' => 'customer@example.com',
        'name' => 'Test Customer'
      }
    }
  end

  describe '#call' do
    context 'with valid cart items' do
      it 'validates cart successfully' do
        result = handler.call(task, sequence, step)

        expect(result).to be_success
        expect(result.result[:validated_items]).to be_an(Array)
        expect(result.result[:validated_items].length).to eq(2)
        expect(result.result[:item_count]).to eq(2)
      end

      it 'calculates correct totals' do
        result = handler.call(task, sequence, step)

        # Widget A: 2 * $29.99 = $59.98
        # Gadget B: 1 * $49.99 = $49.99
        # Subtotal: $109.97
        expected_subtotal = 109.97

        expect(result.result[:subtotal]).to eq(expected_subtotal)

        # Tax: 8% of subtotal = $8.80
        expected_tax = (expected_subtotal * 0.08).round(2)
        expect(result.result[:tax]).to eq(expected_tax)

        # Shipping: 3 items * 0.5 lbs = 1.5 lbs → $5.99
        expect(result.result[:shipping]).to eq(5.99)

        # Total: $109.97 + $8.80 + $5.99 = $124.76
        expected_total = expected_subtotal + expected_tax + 5.99
        expect(result.result[:total]).to eq(expected_total)
      end

      it 'includes validated item details' do
        result = handler.call(task, sequence, step)

        first_item = result.result[:validated_items][0]
        expect(first_item[:product_id]).to eq(1)
        expect(first_item[:name]).to eq('Widget A')
        expect(first_item[:price]).to eq(29.99)
        expect(first_item[:quantity]).to eq(2)
        expect(first_item[:line_total]).to eq(59.98)

        second_item = result.result[:validated_items][1]
        expect(second_item[:product_id]).to eq(2)
        expect(second_item[:name]).to eq('Gadget B')
        expect(second_item[:price]).to eq(49.99)
        expect(second_item[:quantity]).to eq(1)
        expect(second_item[:line_total]).to eq(49.99)
      end

      it 'includes execution metadata' do
        result = handler.call(task, sequence, step)

        expect(result.metadata[:operation]).to eq('validate_cart')
        expect(result.metadata[:execution_hints]).to be_present
        expect(result.metadata[:execution_hints][:items_validated]).to eq(2)
        expect(result.metadata[:execution_hints][:total_amount]).to be_present
      end

      it 'includes timestamp' do
        result = handler.call(task, sequence, step)

        expect(result.result[:validated_at]).to be_present
        expect(result.result[:validated_at]).to match(/\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z/)
      end
    end

    context 'with single item' do
      let(:task_context) do
        {
          'cart_items' => [
            { 'product_id' => 1, 'quantity' => 1 }
          ]
        }
      end

      it 'validates single item cart' do
        result = handler.call(task, sequence, step)

        expect(result).to be_success
        expect(result.result[:item_count]).to eq(1)
        expect(result.result[:subtotal]).to eq(29.99)
      end

      it 'calculates shipping for light weight' do
        result = handler.call(task, sequence, step)

        # 1 item * 0.5 lbs = 0.5 lbs → $5.99
        expect(result.result[:shipping]).to eq(5.99)
      end
    end

    context 'with heavy cart' do
      let(:task_context) do
        {
          'cart_items' => [
            { 'product_id' => 1, 'quantity' => 30 } # 30 items * 0.5 lbs = 15 lbs
          ]
        }
      end

      it 'calculates higher shipping for heavy cart' do
        result = handler.call(task, sequence, step)

        # 15 lbs → $14.99
        expect(result.result[:shipping]).to eq(14.99)
      end
    end

    context 'with missing cart items' do
      let(:task_context) { {} }

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Cart items are required')
          expect(error.error_code).to eq('MISSING_CART_ITEMS')
        end
      end
    end

    context 'with empty cart' do
      let(:task_context) { { 'cart_items' => [] } }

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Cart items are required')
        end
      end
    end

    context 'with missing product_id' do
      let(:task_context) do
        {
          'cart_items' => [
            { 'quantity' => 2 } # Missing product_id
          ]
        }
      end

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Product ID is required')
          expect(error.error_code).to eq('MISSING_PRODUCT_ID')
        end
      end
    end

    context 'with invalid quantity' do
      let(:task_context) do
        {
          'cart_items' => [
            { 'product_id' => 1, 'quantity' => 0 }
          ]
        }
      end

      it 'raises permanent error for zero quantity' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Valid quantity is required')
          expect(error.error_code).to eq('INVALID_QUANTITY')
        end
      end
    end

    context 'with invalid quantity (negative)' do
      let(:task_context) do
        {
          'cart_items' => [
            { 'product_id' => 1, 'quantity' => -1 }
          ]
        }
      end

      it 'raises permanent error for negative quantity' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Valid quantity is required')
        end
      end
    end

    context 'with non-existent product' do
      let(:task_context) do
        {
          'cart_items' => [
            { 'product_id' => 999999, 'quantity' => 1 }
          ]
        }
      end

      it 'returns nil for non-existent product' do
        # Note: Our mock Product.find_by returns nil for unknown IDs > 3
        # The handler will raise PRODUCT_NOT_FOUND error
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('not found')
          expect(error.error_code).to eq('PRODUCT_NOT_FOUND')
        end
      end
    end

    context 'with symbolized keys in context' do
      let(:task_context) do
        {
          cart_items: [
            { product_id: 1, quantity: 2 }
          ]
        }
      end

      it 'handles symbolized keys correctly' do
        result = handler.call(task, sequence, step)

        expect(result).to be_success
        expect(result.result[:item_count]).to eq(1)
      end
    end

    context 'with mixed string and symbol keys' do
      let(:task_context) do
        {
          'cart_items' => [
            { product_id: 1, 'quantity' => 2 }
          ]
        }
      end

      it 'normalizes keys correctly' do
        result = handler.call(task, sequence, step)

        expect(result).to be_success
        expect(result.result[:item_count]).to eq(1)
      end
    end
  end

  describe 'shipping calculation' do
    context 'with weight 0-2 lbs' do
      let(:task_context) do
        {
          'cart_items' => [
            { 'product_id' => 1, 'quantity' => 1 } # 0.5 lbs
          ]
        }
      end

      it 'charges $5.99 for light weight' do
        result = handler.call(task, sequence, step)
        expect(result.result[:shipping]).to eq(5.99)
      end
    end

    context 'with weight 2-10 lbs' do
      let(:task_context) do
        {
          'cart_items' => [
            { 'product_id' => 1, 'quantity' => 10 } # 5 lbs
          ]
        }
      end

      it 'charges $9.99 for medium weight' do
        result = handler.call(task, sequence, step)
        expect(result.result[:shipping]).to eq(9.99)
      end
    end

    context 'with weight > 10 lbs' do
      let(:task_context) do
        {
          'cart_items' => [
            { 'product_id' => 1, 'quantity' => 25 } # 12.5 lbs
          ]
        }
      end

      it 'charges $14.99 for heavy weight' do
        result = handler.call(task, sequence, step)
        expect(result.result[:shipping]).to eq(14.99)
      end
    end
  end
end
