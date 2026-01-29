# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::Errors do
  describe 'inheritance hierarchy' do
    it 'has Error inheriting from StandardError' do
      expect(TaskerCore::Errors::Error.superclass).to eq(StandardError)
    end

    it 'has ProceduralError inheriting from Error' do
      expect(TaskerCore::Errors::ProceduralError.superclass).to eq(TaskerCore::Errors::Error)
    end

    it 'has RetryableError inheriting from ProceduralError' do
      expect(TaskerCore::Errors::RetryableError.superclass).to eq(TaskerCore::Errors::ProceduralError)
    end

    it 'has PermanentError inheriting from ProceduralError' do
      expect(TaskerCore::Errors::PermanentError.superclass).to eq(TaskerCore::Errors::ProceduralError)
    end

    it 'has ValidationError inheriting from PermanentError' do
      expect(TaskerCore::Errors::ValidationError.superclass).to eq(TaskerCore::Errors::PermanentError)
    end

    it 'has NotFoundError inheriting from PermanentError' do
      expect(TaskerCore::Errors::NotFoundError.superclass).to eq(TaskerCore::Errors::PermanentError)
    end

    it 'has FFIError inheriting from Error' do
      expect(TaskerCore::Errors::FFIError.superclass).to eq(TaskerCore::Errors::Error)
    end

    it 'has ServerError inheriting from Error' do
      expect(TaskerCore::Errors::ServerError.superclass).to eq(TaskerCore::Errors::Error)
    end
  end

  describe TaskerCore::Errors::RetryableError do
    subject(:error) { described_class.new('Service timeout', retry_after: 30, context: { service: 'billing' }) }

    it 'stores the message' do
      expect(error.message).to eq('Service timeout')
    end

    it 'stores retry_after' do
      expect(error.retry_after).to eq(30)
    end

    it 'stores context' do
      expect(error.context).to eq({ service: 'billing' })
    end

    it 'returns the error_class name' do
      expect(error.error_class).to eq('RetryableError')
    end

    it 'defaults retry_after to nil' do
      err = described_class.new('simple error')
      expect(err.retry_after).to be_nil
    end

    it 'defaults context to empty hash' do
      err = described_class.new('simple error')
      expect(err.context).to eq({})
    end
  end

  describe TaskerCore::Errors::PermanentError do
    subject(:error) { described_class.new('Invalid data', error_code: 'INVALID', context: { field: 'email' }) }

    it 'stores the message' do
      expect(error.message).to eq('Invalid data')
    end

    it 'stores error_code' do
      expect(error.error_code).to eq('INVALID')
    end

    it 'stores context' do
      expect(error.context).to eq({ field: 'email' })
    end

    it 'returns the error_class name' do
      expect(error.error_class).to eq('PermanentError')
    end

    it 'defaults error_code to nil' do
      err = described_class.new('simple error')
      expect(err.error_code).to be_nil
    end

    it 'defaults context to empty hash' do
      err = described_class.new('simple error')
      expect(err.context).to eq({})
    end
  end

  describe TaskerCore::Errors::TimeoutError do
    subject(:error) { described_class.new('Query timeout', timeout_duration: 60, context: { query: 'SELECT' }) }

    it 'stores the message' do
      expect(error.message).to eq('Query timeout')
    end

    it 'stores timeout_duration' do
      expect(error.timeout_duration).to eq(60)
    end

    it 'stores context' do
      expect(error.context).to eq({ query: 'SELECT' })
    end

    it 'returns the error_class name' do
      expect(error.error_class).to eq('TimeoutError')
    end

    it 'defaults timeout_duration to nil' do
      err = described_class.new('simple timeout')
      expect(err.timeout_duration).to be_nil
    end
  end

  describe TaskerCore::Errors::NetworkError do
    subject(:error) do
      described_class.new('Connection refused', status_code: 503, context: { host: 'api.example.com' })
    end

    it 'stores the message' do
      expect(error.message).to eq('Connection refused')
    end

    it 'stores status_code' do
      expect(error.status_code).to eq(503)
    end

    it 'stores context' do
      expect(error.context).to eq({ host: 'api.example.com' })
    end

    it 'returns the error_class name' do
      expect(error.error_class).to eq('NetworkError')
    end

    it 'defaults status_code to nil' do
      err = described_class.new('network error')
      expect(err.status_code).to be_nil
    end
  end

  describe TaskerCore::Errors::ValidationError do
    subject(:error) do
      described_class.new('Email is invalid', field: 'email', error_code: 'INVALID_FORMAT', context: { value: 'bad' })
    end

    it 'stores the message' do
      expect(error.message).to eq('Email is invalid')
    end

    it 'stores the field' do
      expect(error.field).to eq('email')
    end

    it 'inherits error_code from PermanentError' do
      expect(error.error_code).to eq('INVALID_FORMAT')
    end

    it 'inherits context from PermanentError' do
      expect(error.context).to eq({ value: 'bad' })
    end

    it 'returns the error_class name' do
      expect(error.error_class).to eq('ValidationError')
    end

    it 'defaults field to nil' do
      err = described_class.new('validation error')
      expect(err.field).to be_nil
    end
  end

  describe TaskerCore::Errors::NotFoundError do
    subject(:error) do
      described_class.new('Handler missing', resource_type: 'handler', error_code: 'NOT_FOUND',
                                             context: { name: 'foo' })
    end

    it 'stores the message' do
      expect(error.message).to eq('Handler missing')
    end

    it 'stores resource_type' do
      expect(error.resource_type).to eq('handler')
    end

    it 'inherits error_code from PermanentError' do
      expect(error.error_code).to eq('NOT_FOUND')
    end

    it 'inherits context from PermanentError' do
      expect(error.context).to eq({ name: 'foo' })
    end

    it 'returns the error_class name' do
      expect(error.error_class).to eq('NotFoundError')
    end

    it 'defaults resource_type to nil' do
      err = described_class.new('not found')
      expect(err.resource_type).to be_nil
    end
  end

  describe TaskerCore::Errors::FFIError do
    subject(:error) { described_class.new('Bridge failed', operation: 'log_info', context: { module: 'tracing' }) }

    it 'stores the message' do
      expect(error.message).to eq('Bridge failed')
    end

    it 'stores operation' do
      expect(error.operation).to eq('log_info')
    end

    it 'stores context' do
      expect(error.context).to eq({ module: 'tracing' })
    end

    it 'returns the error_class name' do
      expect(error.error_class).to eq('FFIError')
    end

    it 'defaults operation to nil' do
      err = described_class.new('ffi error')
      expect(err.operation).to be_nil
    end
  end

  describe TaskerCore::Errors::ServerError do
    subject(:error) { described_class.new('Startup failed', operation: 'start', context: { port: 8080 }) }

    it 'stores the message' do
      expect(error.message).to eq('Startup failed')
    end

    it 'stores operation' do
      expect(error.operation).to eq('start')
    end

    it 'stores context' do
      expect(error.context).to eq({ port: 8080 })
    end

    it 'returns the error_class name' do
      expect(error.error_class).to eq('ServerError')
    end

    it 'defaults operation to nil' do
      err = described_class.new('server error')
      expect(err.operation).to be_nil
    end
  end
end
