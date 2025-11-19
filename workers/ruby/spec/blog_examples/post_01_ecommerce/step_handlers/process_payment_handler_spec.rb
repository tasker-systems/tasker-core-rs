# frozen_string_literal: true

require 'spec_helper'
require_relative '../../support/blog_spec_helper'

# Load handler and dependencies
require 'handlers/examples/blog_examples/post_01_ecommerce/step_handlers/process_payment_handler'
require 'blog_examples/support/mock_services/payment_service'

RSpec.describe Ecommerce::StepHandlers::ProcessPaymentHandler do
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
           name: 'process_payment',
           task_uuid: task.task_uuid)
  end

  # Default cart validation result (from previous validate_cart step)
  let(:cart_validation_result) do
    {
      'validated_items' => [
        { 'product_id' => 1, 'name' => 'Widget A', 'price' => 29.99, 'quantity' => 2, 'line_total' => 59.98 },
        { 'product_id' => 2, 'name' => 'Gadget B', 'price' => 49.99, 'quantity' => 1, 'line_total' => 49.99 }
      ],
      'subtotal' => 109.97,
      'tax' => 8.80,
      'shipping' => 5.99,
      'total' => 124.76,
      'item_count' => 2
    }
  end

  # Default valid task context with payment info
  let(:task_context) do
    {
      'payment_info' => {
        'method' => 'credit_card',
        'token' => 'tok_test_12345',
        'amount' => 124.76
      },
      'customer_info' => {
        'email' => 'customer@example.com',
        'name' => 'Test Customer'
      }
    }
  end

  before do
    # Reset mock service before each test
    MockPaymentService.reset!
  end

  describe '#call' do
    context 'with valid payment information' do
      it 'processes payment successfully' do
        result = handler.call(task, sequence, step)

        expect(result).to be_success
        expect(result.result[:payment_id]).to be_present
        expect(result.result[:amount_charged]).to eq(124.76)
        expect(result.result[:currency]).to eq('USD')
        expect(result.result[:status]).to eq('completed')
      end

      it 'includes transaction details' do
        result = handler.call(task, sequence, step)

        expect(result.result[:transaction_id]).to be_present
        expect(result.result[:processed_at]).to be_present
        expect(result.result[:processed_at]).to match(/\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z/)
      end

      it 'includes execution metadata' do
        result = handler.call(task, sequence, step)

        expect(result.metadata[:operation]).to eq('process_payment')
        expect(result.metadata[:execution_hints]).to be_present
        expect(result.metadata[:execution_hints][:payment_gateway]).to eq('mock')
        expect(result.metadata[:execution_hints][:amount_charged]).to eq(124.76)
      end

      it 'includes HTTP headers in metadata' do
        result = handler.call(task, sequence, step)

        headers = result.metadata[:http_headers]
        expect(headers['X-Payment-Gateway']).to eq('MockPaymentService')
        expect(headers['X-Payment-ID']).to be_present
        expect(headers['X-Transaction-ID']).to be_present
      end

      it 'includes input references in metadata' do
        result = handler.call(task, sequence, step)

        input_refs = result.metadata[:input_refs]
        expect(input_refs[:amount]).to eq('sequence.validate_cart.result.total')
        expect(input_refs[:payment_info]).to eq('task.context.payment_info')
      end

      it 'calls MockPaymentService with correct parameters' do
        handler.call(task, sequence, step)

        expect(MockPaymentService.called?(:process_payment)).to be true
        last_call = MockPaymentService.last_call(:process_payment)
        expect(last_call[:args][:amount]).to eq(124.76)
        expect(last_call[:args][:method]).to eq('credit_card')
        expect(last_call[:args][:token]).to eq('tok_test_12345')
        expect(last_call[:args][:currency]).to eq('USD')
      end
    end

    context 'with different payment methods' do
      let(:task_context) do
        {
          'payment_info' => {
            'method' => 'paypal',
            'token' => 'paypal_token_xyz',
            'amount' => 124.76
          }
        }
      end

      it 'processes PayPal payment successfully' do
        result = handler.call(task, sequence, step)

        expect(result).to be_success
        expect(result.result[:amount_charged]).to eq(124.76)
        expect(result.result[:status]).to eq('completed')
      end
    end

    context 'with missing payment method' do
      let(:task_context) do
        {
          'payment_info' => {
            'token' => 'tok_test_12345',
            'amount' => 124.76
          }
        }
      end

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Payment method is required')
          expect(error.error_code).to eq('MISSING_PAYMENT_METHOD')
        end
      end
    end

    context 'with missing payment token' do
      let(:task_context) do
        {
          'payment_info' => {
            'method' => 'credit_card',
            'amount' => 124.76
          }
        }
      end

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Payment token is required')
          expect(error.error_code).to eq('MISSING_PAYMENT_TOKEN')
        end
      end
    end

    context 'with missing cart total' do
      let(:cart_validation_result) { nil }

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Cart total is required')
          expect(error.error_code).to eq('MISSING_CART_TOTAL')
        end
      end
    end

    context 'with missing payment_info' do
      let(:task_context) { { 'customer_info' => { 'email' => 'test@example.com' } } }

      it 'raises permanent error for missing payment method' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Payment method is required')
        end
      end
    end

    context 'with payment amount mismatch' do
      let(:task_context) do
        {
          'payment_info' => {
            'method' => 'credit_card',
            'token' => 'tok_test_12345',
            'amount' => 100.00 # Doesn't match cart total of 124.76
          }
        }
      end

      it 'raises permanent error with mismatch details' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Payment amount mismatch')
          expect(error.message).to include('Expected: $124.76')
          expect(error.message).to include('Provided: $100.0')
          expect(error.error_code).to eq('PAYMENT_AMOUNT_MISMATCH')
          expect(error.context[:expected_amount]).to eq(124.76)
          expect(error.context[:provided_amount]).to eq(100.00)
          expect(error.context[:difference]).to eq(-24.76)
        end
      end
    end

    context 'with floating point rounding tolerance' do
      let(:task_context) do
        {
          'payment_info' => {
            'method' => 'credit_card',
            'token' => 'tok_test_12345',
            'amount' => 124.755 # Within 0.01 tolerance
          }
        }
      end

      it 'accepts amounts within rounding tolerance' do
        result = handler.call(task, sequence, step)
        expect(result).to be_success
      end
    end

    context 'with payment declined by card' do
      before do
        MockPaymentService.stub_response(:process_payment, {
          status: 'card_declined',
          error: 'Card was declined by issuer'
        })
      end

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Payment declined')
          expect(error.message).to include('Card was declined by issuer')
          expect(error.error_code).to eq('PAYMENT_DECLINED')
          expect(error.context[:payment_status]).to eq('card_declined')
        end
      end
    end

    context 'with insufficient funds' do
      before do
        MockPaymentService.stub_response(:process_payment, {
          status: 'insufficient_funds',
          error: 'Insufficient funds in account'
        })
      end

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Payment declined')
          expect(error.message).to include('Insufficient funds')
          expect(error.error_code).to eq('PAYMENT_DECLINED')
          expect(error.context[:payment_status]).to eq('insufficient_funds')
        end
      end
    end

    context 'with invalid card' do
      before do
        MockPaymentService.stub_response(:process_payment, {
          status: 'invalid_card',
          error: 'Card number is invalid'
        })
      end

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Payment declined')
          expect(error.error_code).to eq('PAYMENT_DECLINED')
        end
      end
    end

    context 'with rate limiting' do
      before do
        MockPaymentService.stub_response(:process_payment, {
          status: 'rate_limited',
          error: 'Too many requests'
        })
      end

      it 'raises retryable error with backoff' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::RetryableError) do |error|
          expect(error.message).to include('Payment service rate limited')
          expect(error.retry_after).to eq(30)
          expect(error.context[:payment_status]).to eq('rate_limited')
        end
      end
    end

    context 'with service unavailable' do
      before do
        MockPaymentService.stub_response(:process_payment, {
          status: 'service_unavailable',
          error: 'Payment service temporarily unavailable'
        })
      end

      it 'raises retryable error with shorter backoff' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::RetryableError) do |error|
          expect(error.message).to include('Payment service temporarily unavailable')
          expect(error.retry_after).to eq(15)
          expect(error.context[:payment_status]).to eq('service_unavailable')
        end
      end
    end

    context 'with timeout' do
      before do
        MockPaymentService.stub_response(:process_payment, {
          status: 'timeout',
          error: 'Payment gateway timeout'
        })
      end

      it 'raises retryable error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::RetryableError) do |error|
          expect(error.message).to include('Payment service temporarily unavailable')
          expect(error.retry_after).to eq(15)
        end
      end
    end

    context 'with unknown payment status' do
      before do
        MockPaymentService.stub_response(:process_payment, {
          status: 'unknown_error',
          error: 'Something went wrong'
        })
      end

      it 'raises retryable error for safety' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::RetryableError) do |error|
          expect(error.message).to include('Payment service returned unknown status')
          expect(error.message).to include('Something went wrong')
          expect(error.retry_after).to eq(30)
          expect(error.context[:payment_status]).to eq('unknown_error')
        end
      end
    end

    context 'with network error from payment service' do
      before do
        MockPaymentService.stub_failure(:process_payment,
                                       MockPaymentService::NetworkError,
                                       'Connection timeout')
      end

      it 'raises retryable error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::RetryableError) do |error|
          expect(error.message).to include('Payment service network error')
          expect(error.message).to include('Connection timeout')
          expect(error.retry_after).to eq(30)
          expect(error.context[:error_class]).to eq('MockPaymentService::NetworkError')
        end
      end
    end

    context 'with payment error from payment service' do
      before do
        MockPaymentService.stub_failure(:process_payment,
                                       MockPaymentService::CardDeclinedError,
                                       'Card declined')
      end

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Payment failed')
          expect(error.message).to include('Card declined')
          expect(error.error_code).to eq('PAYMENT_FAILED')
          expect(error.context[:error_class]).to eq('MockPaymentService::CardDeclinedError')
        end
      end
    end

    context 'with symbolized keys in context' do
      let(:task_context) do
        {
          payment_info: {
            method: 'credit_card',
            token: 'tok_test_12345',
            amount: 124.76
          }
        }
      end

      it 'handles symbolized keys correctly' do
        result = handler.call(task, sequence, step)
        expect(result).to be_success
        expect(result.result[:amount_charged]).to eq(124.76)
      end
    end

    context 'with symbolized keys in cart validation result' do
      let(:cart_validation_result) do
        {
          validated_items: [
            { product_id: 1, quantity: 2, line_total: 59.98 }
          ],
          subtotal: 109.97,
          tax: 8.80,
          shipping: 5.99,
          total: 124.76
        }
      end

      it 'handles symbolized cart results correctly' do
        result = handler.call(task, sequence, step)
        expect(result).to be_success
        expect(result.result[:amount_charged]).to eq(124.76)
      end
    end

    context 'with mixed string and symbol keys' do
      let(:task_context) do
        {
          'payment_info' => {
            method: 'credit_card',
            'token' => 'tok_test_12345',
            amount: 124.76
          }
        }
      end

      it 'normalizes keys correctly' do
        result = handler.call(task, sequence, step)
        expect(result).to be_success
      end
    end
  end

  describe 'payment method types' do
    it 'processes credit card payments' do
      result = handler.call(task, sequence, step)
      expect(result.result[:payment_method_type]).to eq('credit_card')
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
