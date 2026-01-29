# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::Tracing do
  before do
    # Stub all FFI log methods
    allow(TaskerCore::FFI).to receive(:log_error)
    allow(TaskerCore::FFI).to receive(:log_warn)
    allow(TaskerCore::FFI).to receive(:log_info)
    allow(TaskerCore::FFI).to receive(:log_debug)
    allow(TaskerCore::FFI).to receive(:log_trace)
  end

  describe '.error' do
    it 'calls FFI.log_error with message and normalized fields' do
      described_class.error('Something failed', { task_uuid: 'abc-123' })

      expect(TaskerCore::FFI).to have_received(:log_error).with('Something failed', { 'task_uuid' => 'abc-123' })
    end

    it 'handles empty fields' do
      described_class.error('No fields')

      expect(TaskerCore::FFI).to have_received(:log_error).with('No fields', {})
    end

    context 'when FFI raises an error' do
      before do
        allow(TaskerCore::FFI).to receive(:log_error).and_raise(StandardError, 'FFI unavailable')
      end

      it 'falls back to calling warn via FFI.log_warn' do
        # The rescue calls `warn` which resolves to Tracing.warn -> FFI.log_warn
        described_class.error('Test message', { key: 'val' })

        expect(TaskerCore::FFI).to have_received(:log_warn).at_least(:once)
      end
    end
  end

  describe '.warn' do
    it 'calls FFI.log_warn with message and normalized fields' do
      described_class.warn('Warning message', { step_uuid: 'step-1' })

      expect(TaskerCore::FFI).to have_received(:log_warn).with('Warning message', { 'step_uuid' => 'step-1' })
    end

    context 'when FFI.log_warn raises an error' do
      it 'raises SystemStackError due to recursive warn call' do
        allow(TaskerCore::FFI).to receive(:log_warn).and_raise(StandardError, 'FFI unavailable')

        # The rescue calls `warn` which is Tracing.warn itself — infinite recursion
        expect { described_class.warn('Test', {}) }.to raise_error(SystemStackError)
      end
    end
  end

  describe '.info' do
    it 'calls FFI.log_info with message and normalized fields' do
      described_class.info('Info message', { duration_ms: 42 })

      expect(TaskerCore::FFI).to have_received(:log_info).with('Info message', { 'duration_ms' => '42' })
    end

    context 'when FFI raises an error' do
      before do
        allow(TaskerCore::FFI).to receive(:log_info).and_raise(StandardError, 'FFI unavailable')
      end

      it 'falls back to calling warn via FFI.log_warn' do
        described_class.info('Test', {})

        expect(TaskerCore::FFI).to have_received(:log_warn).at_least(:once)
      end
    end
  end

  describe '.debug' do
    it 'calls FFI.log_debug with message and normalized fields' do
      described_class.debug('Debug details', { verbose: true })

      expect(TaskerCore::FFI).to have_received(:log_debug).with('Debug details', { 'verbose' => 'true' })
    end

    context 'when FFI raises an error' do
      before do
        allow(TaskerCore::FFI).to receive(:log_debug).and_raise(StandardError, 'FFI unavailable')
      end

      it 'falls back to calling warn via FFI.log_warn' do
        described_class.debug('Test', {})

        expect(TaskerCore::FFI).to have_received(:log_warn).at_least(:once)
      end
    end
  end

  describe '.trace' do
    it 'calls FFI.log_trace with message and normalized fields' do
      described_class.trace('Trace entry', { operation: :validate })

      expect(TaskerCore::FFI).to have_received(:log_trace).with('Trace entry', { 'operation' => 'validate' })
    end

    context 'when FFI raises an error' do
      before do
        allow(TaskerCore::FFI).to receive(:log_trace).and_raise(StandardError, 'FFI unavailable')
      end

      it 'falls back to calling warn via FFI.log_warn' do
        described_class.trace('Test', {})

        expect(TaskerCore::FFI).to have_received(:log_warn).at_least(:once)
      end
    end
  end

  describe 'field normalization' do
    it 'converts symbol keys to strings' do
      described_class.info('msg', { task_uuid: 'abc' })

      expect(TaskerCore::FFI).to have_received(:log_info).with('msg', { 'task_uuid' => 'abc' })
    end

    it 'normalizes nil values to string' do
      described_class.info('msg', { value: nil })

      expect(TaskerCore::FFI).to have_received(:log_info).with('msg', { 'value' => 'nil' })
    end

    it 'passes strings through unchanged' do
      described_class.info('msg', { name: 'hello' })

      expect(TaskerCore::FFI).to have_received(:log_info).with('msg', { 'name' => 'hello' })
    end

    it 'converts symbols to strings' do
      described_class.info('msg', { status: :active })

      expect(TaskerCore::FFI).to have_received(:log_info).with('msg', { 'status' => 'active' })
    end

    it 'converts numeric values to strings' do
      described_class.info('msg', { count: 42, rate: 3.14 })

      expect(TaskerCore::FFI).to have_received(:log_info).with('msg', { 'count' => '42', 'rate' => '3.14' })
    end

    it 'converts boolean values to strings' do
      described_class.info('msg', { enabled: true, disabled: false })

      expect(TaskerCore::FFI).to have_received(:log_info).with('msg', { 'enabled' => 'true', 'disabled' => 'false' })
    end

    it 'formats exceptions as class and message' do
      error = RuntimeError.new('something broke')
      described_class.info('msg', { error: error })

      expect(TaskerCore::FFI).to have_received(:log_info).with('msg', { 'error' => 'RuntimeError: something broke' })
    end

    it 'falls back to to_s for unknown types' do
      obj = Object.new
      allow(obj).to receive(:to_s).and_return('custom_object')
      described_class.info('msg', { obj: obj })

      expect(TaskerCore::FFI).to have_received(:log_info).with('msg', { 'obj' => 'custom_object' })
    end

    it 'handles nil fields gracefully' do
      described_class.info('msg', nil)

      expect(TaskerCore::FFI).to have_received(:log_info).with('msg', {})
    end

    it 'handles serialization errors gracefully' do
      bad_obj = Object.new
      allow(bad_obj).to receive(:to_s).and_raise(StandardError, 'cannot serialize')
      described_class.info('msg', { bad: bad_obj })

      expect(TaskerCore::FFI).to have_received(:log_info).with('msg', { 'bad' => '<serialization_error>' })
    end
  end
end
