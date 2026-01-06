# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::Types::StepHandlerCallResult do
  describe '.success' do
    it 'creates a success result with result and metadata' do
      result = described_class.success(
        result: { customer_id: 123, status: 'validated' },
        metadata: { operation: 'validate', item_count: 5 }
      )

      expect(result).to be_a(TaskerCore::Types::StepHandlerCallResult::Success)
      expect(result.success).to be true
      expect(result.result).to eq({ customer_id: 123, status: 'validated' })
      expect(result.metadata[:operation]).to eq('validate')
      expect(result.metadata[:item_count]).to eq(5)
    end
  end

  describe '.error' do
    it 'creates an error result with proper structure' do
      result = described_class.error(
        error_type: 'PermanentError',
        message: 'Invalid customer ID',
        error_code: 'INVALID_CUSTOMER',
        retryable: false,
        metadata: { context: { customer_id: nil } }
      )

      expect(result).to be_a(TaskerCore::Types::StepHandlerCallResult::Error)
      expect(result.success).to be false
      expect(result.error_type).to eq('PermanentError')
      expect(result.message).to eq('Invalid customer ID')
      expect(result.error_code).to eq('INVALID_CUSTOMER')
      expect(result.retryable).to be false
      expect(result.metadata[:context]).to eq({ customer_id: nil })
    end
  end

  describe '.from_handler_output' do
    context 'with already structured success result' do
      it 'returns the result unchanged' do
        original = described_class.success(result: { data: 'test' })
        result = described_class.from_handler_output(original)

        expect(result).to eq(original)
      end
    end

    context 'with hash that looks like success result' do
      it 'converts to Success instance' do
        hash_output = {
          success: true,
          result: { validated: true },
          metadata: { operation: 'validate' }
        }

        result = described_class.from_handler_output(hash_output)

        expect(result).to be_a(TaskerCore::Types::StepHandlerCallResult::Success)
        expect(result.success).to be true
        expect(result.result).to eq({ validated: true })
        expect(result.metadata[:operation]).to eq('validate')
      end
    end

    context 'with hash that looks like error result' do
      it 'converts to Error instance' do
        hash_output = {
          success: false,
          error_type: 'ValidationError',
          message: 'Invalid data',
          retryable: false
        }

        result = described_class.from_handler_output(hash_output)

        expect(result).to be_a(TaskerCore::Types::StepHandlerCallResult::Error)
        expect(result.success).to be false
        expect(result.error_type).to eq('ValidationError')
        expect(result.message).to eq('Invalid data')
        expect(result.retryable).to be false
      end
    end

    context 'with plain hash (legacy handler)' do
      it 'wraps as success result with wrapped metadata' do
        hash_output = { customer_id: 123, total: 100.50 }

        result = described_class.from_handler_output(hash_output)

        expect(result).to be_a(TaskerCore::Types::StepHandlerCallResult::Success)
        expect(result.success).to be true
        expect(result.result).to eq(hash_output)
        expect(result.metadata[:wrapped]).to be true
        expect(result.metadata[:original_type]).to eq('Hash')
      end
    end

    context 'with non-hash value' do
      it 'wraps as success result' do
        result = described_class.from_handler_output(42)

        expect(result).to be_a(TaskerCore::Types::StepHandlerCallResult::Success)
        expect(result.success).to be true
        expect(result.result).to eq(42)
        expect(result.metadata[:wrapped]).to be true
        expect(result.metadata[:original_type]).to eq('Integer')
      end
    end
  end

  describe '.from_exception' do
    context 'with PermanentError' do
      it 'creates proper error result' do
        exception = TaskerCore::Errors::PermanentError.new(
          'Invalid order total',
          error_code: 'INVALID_TOTAL',
          context: { total: -50 }
        )

        result = described_class.from_exception(exception)

        expect(result).to be_a(TaskerCore::Types::StepHandlerCallResult::Error)
        expect(result.success).to be false
        expect(result.error_type).to eq('PermanentError')
        expect(result.message).to eq('Invalid order total')
        expect(result.error_code).to eq('INVALID_TOTAL')
        expect(result.retryable).to be false
        expect(result.metadata[:context]).to eq({ total: -50 })
        # Field is not part of PermanentError - remove this expectation
      end
    end

    context 'with RetryableError' do
      it 'creates proper error result with retry info' do
        exception = TaskerCore::Errors::RetryableError.new(
          'Service temporarily unavailable',
          retry_after: 30,
          context: { service: 'payment_processor' }
        )

        result = described_class.from_exception(exception)

        expect(result).to be_a(TaskerCore::Types::StepHandlerCallResult::Error)
        expect(result.success).to be false
        expect(result.error_type).to eq('RetryableError')
        expect(result.retryable).to be true
        expect(result.metadata[:context]).to eq({ service: 'payment_processor' })
      end
    end

    context 'with generic exception' do
      it 'creates UnexpectedError result' do
        exception = StandardError.new('Something went wrong')
        exception.set_backtrace(%w[line1 line2 line3])

        result = described_class.from_exception(exception)

        expect(result).to be_a(TaskerCore::Types::StepHandlerCallResult::Error)
        expect(result.success).to be false
        expect(result.error_type).to eq('UnexpectedError')
        expect(result.message).to eq('Something went wrong')
        expect(result.retryable).to be true
        expect(result.metadata[:exception_class]).to eq('StandardError')
        expect(result.metadata[:stack_trace]).to include('line1')
      end
    end
  end

  describe 'input_refs functionality' do
    it 'tracks data references instead of duplicating data' do
      result = described_class.success(
        result: { validated_items: [{ id: 1 }, { id: 2 }] },
        metadata: {
          operation: 'validate_order',
          item_count: 2,
          input_refs: {
            customer_id: 'task.context.customer_info.id',
            order_items: 'task.context.order_items',
            previous_validation: 'sequence.validate_customer.result'
          }
        }
      )

      expect(result.metadata[:input_refs]).to eq({
                                                   customer_id: 'task.context.customer_info.id',
                                                   order_items: 'task.context.order_items',
                                                   previous_validation: 'sequence.validate_customer.result'
                                                 })

      # Verify we're not duplicating the actual data
      expect(result.metadata).not_to have_key(:customer_info)
      expect(result.metadata).not_to have_key(:order_items)
    end
  end

  # TAS-125: Checkpoint Yield Tests
  describe '.checkpoint_yield' do
    it 'creates a checkpoint yield result with required fields' do
      result = described_class.checkpoint_yield(
        cursor: 5000,
        items_processed: 5000
      )

      expect(result).to be_a(TaskerCore::Types::StepHandlerCallResult::CheckpointYield)
      expect(result.checkpoint?).to be true
      expect(result.success?).to be false
      expect(result.cursor).to eq(5000)
      expect(result.items_processed).to eq(5000)
      expect(result.accumulated_results).to be_nil
    end

    it 'creates a checkpoint yield result with accumulated results' do
      result = described_class.checkpoint_yield(
        cursor: 3000,
        items_processed: 3000,
        accumulated_results: { total_amount: 150_000.50, processed_count: 3000 }
      )

      expect(result.cursor).to eq(3000)
      expect(result.items_processed).to eq(3000)
      expect(result.accumulated_results).to eq({ total_amount: 150_000.50, processed_count: 3000 })
    end

    it 'supports string cursor (pagination token)' do
      result = described_class.checkpoint_yield(
        cursor: 'eyJsYXN0X2lkIjoiOTk5In0=',
        items_processed: 100
      )

      expect(result.cursor).to eq('eyJsYXN0X2lkIjoiOTk5In0=')
    end

    it 'supports hash cursor (complex pagination)' do
      complex_cursor = { page: 5, partition: 'A', last_timestamp: '2024-01-15T10:00:00Z' }
      result = described_class.checkpoint_yield(
        cursor: complex_cursor,
        items_processed: 250
      )

      expect(result.cursor).to eq(complex_cursor)
      expect(result.cursor[:page]).to eq(5)
    end

    it 'converts to checkpoint data for FFI transport' do
      event_id = SecureRandom.uuid
      step_uuid = SecureRandom.uuid
      result = described_class.checkpoint_yield(
        cursor: 7500,
        items_processed: 7500,
        accumulated_results: { running_total: 375_000 }
      )

      checkpoint_data = result.to_checkpoint_data(event_id: event_id, step_uuid: step_uuid)

      expect(checkpoint_data[:event_id]).to eq(event_id)
      expect(checkpoint_data[:step_uuid]).to eq(step_uuid)
      expect(checkpoint_data[:cursor]).to eq(7500)
      expect(checkpoint_data[:items_processed]).to eq(7500)
      expect(checkpoint_data[:accumulated_results]).to eq({ running_total: 375_000 })
    end
  end

  describe '.from_handler_output with checkpoint yield' do
    it 'recognizes CheckpointYield result type' do
      original = described_class.checkpoint_yield(
        cursor: 1000,
        items_processed: 1000
      )

      result = described_class.from_handler_output(original)

      expect(result).to eq(original)
      expect(result.checkpoint?).to be true
    end
  end

  # TAS-125: Checkpoint Yield Error Scenario Tests
  describe '.checkpoint_yield error scenarios' do
    it 'handles zero items processed (edge case)' do
      result = described_class.checkpoint_yield(
        cursor: 0,
        items_processed: 0
      )

      expect(result.checkpoint?).to be true
      expect(result.cursor).to eq(0)
      expect(result.items_processed).to eq(0)
    end

    it 'handles nil cursor' do
      result = described_class.checkpoint_yield(
        cursor: nil,
        items_processed: 0
      )

      expect(result.checkpoint?).to be true
      expect(result.cursor).to be_nil
    end

    it 'handles empty hash cursor' do
      result = described_class.checkpoint_yield(
        cursor: {},
        items_processed: 100
      )

      expect(result.checkpoint?).to be true
      expect(result.cursor).to eq({})
    end

    it 'handles empty string cursor' do
      result = described_class.checkpoint_yield(
        cursor: '',
        items_processed: 0
      )

      expect(result.checkpoint?).to be true
      expect(result.cursor).to eq('')
    end

    it 'handles very large items_processed value' do
      large_count = 10_000_000_000
      result = described_class.checkpoint_yield(
        cursor: large_count,
        items_processed: large_count
      )

      expect(result.checkpoint?).to be true
      expect(result.items_processed).to eq(large_count)
    end

    it 'handles deeply nested accumulated results' do
      nested_results = {
        level1: {
          level2: {
            level3: {
              level4: {
                value: 12_345,
                items: [1, 2, 3, 4, 5]
              }
            }
          }
        }
      }
      result = described_class.checkpoint_yield(
        cursor: 5000,
        items_processed: 5000,
        accumulated_results: nested_results
      )

      expect(result.checkpoint?).to be true
      expect(result.accumulated_results[:level1][:level2][:level3][:level4][:value]).to eq(12_345)
    end

    it 'handles special characters in string cursor' do
      special_cursor = 'cursor_with_émojis_🎉_and_中文'
      result = described_class.checkpoint_yield(
        cursor: special_cursor,
        items_processed: 100
      )

      expect(result.checkpoint?).to be true
      expect(result.cursor).to eq(special_cursor)
    end

    it 'handles array cursor (edge case)' do
      array_cursor = [1, 2, 3, 'last_id']
      result = described_class.checkpoint_yield(
        cursor: array_cursor,
        items_processed: 100
      )

      expect(result.checkpoint?).to be true
      expect(result.cursor).to eq(array_cursor)
    end

    it 'handles boolean cursor (edge case)' do
      result = described_class.checkpoint_yield(
        cursor: true,
        items_processed: 100
      )

      expect(result.checkpoint?).to be true
      expect(result.cursor).to be(true)
    end
  end

  describe 'to_checkpoint_data edge cases' do
    it 'handles nil accumulated_results in checkpoint data' do
      event_id = SecureRandom.uuid
      step_uuid = SecureRandom.uuid
      result = described_class.checkpoint_yield(
        cursor: 1000,
        items_processed: 1000,
        accumulated_results: nil
      )

      checkpoint_data = result.to_checkpoint_data(event_id: event_id, step_uuid: step_uuid)

      expect(checkpoint_data[:event_id]).to eq(event_id)
      expect(checkpoint_data[:step_uuid]).to eq(step_uuid)
      expect(checkpoint_data[:accumulated_results]).to be_nil
    end

    it 'handles empty hash accumulated_results in checkpoint data' do
      event_id = SecureRandom.uuid
      step_uuid = SecureRandom.uuid
      result = described_class.checkpoint_yield(
        cursor: 1000,
        items_processed: 1000,
        accumulated_results: {}
      )

      checkpoint_data = result.to_checkpoint_data(event_id: event_id, step_uuid: step_uuid)

      expect(checkpoint_data[:accumulated_results]).to eq({})
    end
  end
end
