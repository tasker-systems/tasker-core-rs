# frozen_string_literal: true

require 'spec_helper'
require_relative '../../support/blog_spec_helper'

# Load handler and dependencies
require 'handlers/examples/blog_examples/post_01_ecommerce/step_handlers/send_confirmation_handler'
require 'blog_examples/support/mock_services/email_service'

RSpec.describe Ecommerce::StepHandlers::SendConfirmationHandler do
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
           name: 'send_confirmation',
           task_uuid: task.task_uuid)
  end

  # Default task context
  let(:task_context) do
    {
      'customer_info' => {
        'email' => 'customer@example.com',
        'name' => 'Test Customer'
      }
    }
  end

  # Results from previous steps
  let(:step_results) do
    {
      'create_order' => {
        'order_id' => 1234,
        'order_number' => 'ORD-20251119-TEST',
        'total_amount' => 124.76,
        'estimated_delivery' => 'November 26, 2025'
      },
      'validate_cart' => {
        'validated_items' => [
          { 'product_id' => 1, 'name' => 'Widget A', 'quantity' => 2 },
          { 'product_id' => 2, 'name' => 'Gadget B', 'quantity' => 1 }
        ]
      }
    }
  end

  before do
    # Reset mock service before each test
    MockEmailService.reset!
  end

  describe '#call' do
    context 'with valid email sending' do
      it 'sends email successfully' do
        result = handler.call(task, sequence, step)

        expect(result).to be_success
        expect(result.result[:email_sent]).to be true
        expect(result.result[:recipient]).to eq('customer@example.com')
        expect(result.result[:email_type]).to eq('order_confirmation')
      end

      it 'includes message ID' do
        result = handler.call(task, sequence, step)

        expect(result.result[:message_id]).to be_present
      end

      it 'includes timestamp' do
        result = handler.call(task, sequence, step)

        expect(result.result[:sent_at]).to be_present
        expect(result.result[:sent_at]).to match(/\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}/)
      end

      it 'includes execution metadata' do
        result = handler.call(task, sequence, step)

        expect(result.metadata[:operation]).to eq('send_confirmation_email')
        expect(result.metadata[:execution_hints]).to be_present
        expect(result.metadata[:execution_hints][:recipient]).to eq('customer@example.com')
        expect(result.metadata[:execution_hints][:email_type]).to eq('order_confirmation')
      end

      it 'includes HTTP headers in metadata' do
        result = handler.call(task, sequence, step)

        headers = result.metadata[:http_headers]
        expect(headers['X-Email-Service']).to eq('MockEmailService')
        expect(headers['X-Recipient']).to eq('customer@example.com')
      end

      it 'includes input references in metadata' do
        result = handler.call(task, sequence, step)

        input_refs = result.metadata[:input_refs]
        expect(input_refs[:customer_info]).to eq('task.context.customer_info')
        expect(input_refs[:order_result]).to eq('sequence.create_order.result')
        expect(input_refs[:cart_validation]).to eq('sequence.validate_cart.result')
      end

      it 'calls MockEmailService with correct parameters' do
        handler.call(task, sequence, step)

        expect(MockEmailService.called?(:send_confirmation)).to be true
        last_call = MockEmailService.last_call(:send_confirmation)
        expect(last_call[:args][:to]).to eq('customer@example.com')
        expect(last_call[:args][:customer_name]).to eq('Test Customer')
        expect(last_call[:args][:order_number]).to eq('ORD-20251119-TEST')
        expect(last_call[:args][:order_id]).to eq(1234)
      end

      it 'includes order details in email' do
        handler.call(task, sequence, step)

        last_call = MockEmailService.last_call(:send_confirmation)
        expect(last_call[:args][:total_amount]).to eq(124.76)
        expect(last_call[:args][:estimated_delivery]).to eq('November 26, 2025')
        expect(last_call[:args][:items]).to be_an(Array)
        expect(last_call[:args][:items].length).to eq(2)
      end

      it 'includes order URL' do
        handler.call(task, sequence, step)

        last_call = MockEmailService.last_call(:send_confirmation)
        expect(last_call[:args][:order_url]).to eq('https://example.com/orders/1234')
      end
    end

    context 'with missing customer email' do
      let(:task_context) do
        {
          'customer_info' => {
            'name' => 'Test Customer'
          }
        }
      end

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Customer email is required')
          expect(error.error_code).to eq('MISSING_CUSTOMER_EMAIL')
        end
      end
    end

    context 'with missing customer info' do
      let(:task_context) { {} }

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Customer email is required')
        end
      end
    end

    context 'with missing order result' do
      let(:step_results) do
        {
          'create_order' => nil,
          'validate_cart' => {
            'validated_items' => [{ 'product_id' => 1 }]
          }
        }
      end

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Order results are required')
          expect(error.error_code).to eq('MISSING_ORDER_RESULT')
        end
      end
    end

    context 'with order result missing order_id' do
      let(:step_results) do
        {
          'create_order' => { 'order_number' => 'ORD-123' },
          'validate_cart' => {
            'validated_items' => [{ 'product_id' => 1 }]
          }
        }
      end

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Order results are required')
        end
      end
    end

    context 'with missing cart validation' do
      let(:step_results) do
        {
          'create_order' => { 'order_id' => 1234 },
          'validate_cart' => nil
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
          'create_order' => { 'order_id' => 1234 },
          'validate_cart' => { 'validated_items' => [] }
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

    context 'with rate limited email service' do
      before do
        MockEmailService.stub_response(:send_confirmation, {
          status: 'rate_limited',
          error: 'Too many emails'
        })
      end

      it 'raises retryable error with longer backoff' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::RetryableError) do |error|
          expect(error.message).to include('Email service rate limited')
          expect(error.retry_after).to eq(60)
          expect(error.context[:delivery_status]).to eq('rate_limited')
        end
      end
    end

    context 'with service unavailable' do
      before do
        MockEmailService.stub_response(:send_confirmation, {
          status: 'service_unavailable',
          error: 'Service temporarily down'
        })
      end

      it 'raises retryable error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::RetryableError) do |error|
          expect(error.message).to include('Email service temporarily unavailable')
          expect(error.retry_after).to eq(30)
          expect(error.context[:delivery_status]).to eq('service_unavailable')
        end
      end
    end

    context 'with timeout' do
      before do
        MockEmailService.stub_response(:send_confirmation, {
          status: 'timeout',
          error: 'Request timeout'
        })
      end

      it 'raises retryable error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::RetryableError) do |error|
          expect(error.message).to include('Email service temporarily unavailable')
        end
      end
    end

    context 'with invalid email' do
      before do
        MockEmailService.stub_response(:send_confirmation, {
          status: 'invalid_email',
          error: 'Email address is invalid',
          email: 'not-an-email'
        })
      end

      it 'raises permanent error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::PermanentError) do |error|
          expect(error.message).to include('Invalid email address')
          expect(error.error_code).to eq('INVALID_EMAIL')
        end
      end
    end

    context 'with unknown error status' do
      before do
        MockEmailService.stub_response(:send_confirmation, {
          status: 'unknown_error',
          error: 'Something went wrong'
        })
      end

      it 'raises retryable error for safety' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(TaskerCore::Errors::RetryableError) do |error|
          expect(error.message).to include('Email delivery failed')
          expect(error.message).to include('Something went wrong')
          expect(error.retry_after).to eq(30)
        end
      end
    end

    context 'with email service failure' do
      before do
        MockEmailService.stub_failure(:send_confirmation,
                                     MockEmailService::EmailError,
                                     'Service error')
      end

      it 'raises the service error' do
        expect {
          handler.call(task, sequence, step)
        }.to raise_error(MockEmailService::EmailError) do |error|
          expect(error.message).to include('Service error')
        end
      end
    end

    context 'with symbolized keys in context' do
      let(:task_context) do
        {
          customer_info: {
            email: 'customer@example.com',
            name: 'Test Customer'
          }
        }
      end

      it 'handles symbolized keys correctly' do
        result = handler.call(task, sequence, step)
        expect(result).to be_success
        expect(result.result[:recipient]).to eq('customer@example.com')
      end
    end

    context 'with symbolized keys in step results' do
      let(:step_results) do
        {
          'create_order' => {
            order_id: 1234,
            order_number: 'ORD-123',
            total_amount: 100.0
          },
          'validate_cart' => {
            validated_items: [{ product_id: 1 }]
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

  describe 'deep_symbolize_keys helper' do
    it 'symbolizes nested hash keys' do
      result = handler.call(task, sequence, step)
      # Implicitly tested through successful processing with mixed key types
      expect(result).to be_success
    end
  end
end
