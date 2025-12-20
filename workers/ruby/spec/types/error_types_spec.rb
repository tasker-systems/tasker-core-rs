# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::Types::ErrorTypes do
  describe 'cross-language standard error type constants' do
    it 'defines PERMANENT_ERROR with PascalCase value' do
      expect(described_class::PERMANENT_ERROR).to eq('PermanentError')
    end

    it 'defines RETRYABLE_ERROR with PascalCase value' do
      expect(described_class::RETRYABLE_ERROR).to eq('RetryableError')
    end

    it 'defines VALIDATION_ERROR with PascalCase value' do
      expect(described_class::VALIDATION_ERROR).to eq('ValidationError')
    end

    it 'defines UNEXPECTED_ERROR with PascalCase value' do
      expect(described_class::UNEXPECTED_ERROR).to eq('UnexpectedError')
    end

    it 'defines STEP_COMPLETION_ERROR with PascalCase value' do
      expect(described_class::STEP_COMPLETION_ERROR).to eq('StepCompletionError')
    end
  end

  describe 'type validation helpers' do
    it 'validates known error types' do
      expect(described_class.valid?('PermanentError')).to be true
      expect(described_class.valid?('RetryableError')).to be true
      expect(described_class.valid?('ValidationError')).to be true
    end

    it 'rejects unknown error types' do
      expect(described_class.valid?('unknown_type')).to be false
      expect(described_class.valid?('random')).to be false
    end

    it 'identifies typically retryable error types' do
      expect(described_class.typically_retryable?('RetryableError')).to be true
      expect(described_class.typically_retryable?('UnexpectedError')).to be true
      expect(described_class.typically_retryable?('PermanentError')).to be false
    end

    it 'identifies typically permanent error types' do
      expect(described_class.typically_permanent?('PermanentError')).to be true
      expect(described_class.typically_permanent?('ValidationError')).to be true
      expect(described_class.typically_permanent?('RetryableError')).to be false
    end
  end

  describe 'usage in StepHandlerCallResult' do
    it 'can be used to create typed error results' do
      result = TaskerCore::Types::StepHandlerCallResult.error(
        error_type: described_class::PERMANENT_ERROR,
        message: 'Invalid customer ID',
        retryable: false
      )

      expect(result.success).to be false
      expect(result.error_type).to eq('PermanentError')
      expect(result.retryable).to be false
    end

    it 'can be used for retryable errors' do
      result = TaskerCore::Types::StepHandlerCallResult.error(
        error_type: described_class::RETRYABLE_ERROR,
        message: 'Service temporarily unavailable',
        retryable: true
      )

      expect(result.success).to be false
      expect(result.error_type).to eq('RetryableError')
      expect(result.retryable).to be true
    end

    it 'can be used for validation errors' do
      result = TaskerCore::Types::StepHandlerCallResult.error(
        error_type: described_class::VALIDATION_ERROR,
        message: 'Invalid input format',
        retryable: false
      )

      expect(result.success).to be false
      expect(result.error_type).to eq('ValidationError')
    end

    it 'can be used for unexpected errors' do
      result = TaskerCore::Types::StepHandlerCallResult.error(
        error_type: described_class::UNEXPECTED_ERROR,
        message: 'Something unexpected happened',
        retryable: true
      )

      expect(result.success).to be false
      expect(result.error_type).to eq('UnexpectedError')
    end
  end
end
