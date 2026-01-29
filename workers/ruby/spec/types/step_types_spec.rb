# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::Types::StepTypes do
  describe TaskerCore::Types::StepTypes::StepExecutionStatus do
    describe '#success?' do
      it 'returns true for success status' do
        status = described_class.new(status: 'success')
        expect(status.success?).to be true
      end

      it 'returns false for non-success status' do
        status = described_class.new(status: 'failed')
        expect(status.success?).to be false
      end
    end

    describe '#completed?' do
      it 'returns true for success status' do
        status = described_class.new(status: 'success')
        expect(status.completed?).to be true
      end

      it 'returns false for in_progress status' do
        status = described_class.new(status: 'in_progress')
        expect(status.completed?).to be false
      end
    end

    describe '#failed?' do
      it 'returns true for failed status' do
        status = described_class.new(status: 'failed')
        expect(status.failed?).to be true
      end

      it 'returns false for success status' do
        status = described_class.new(status: 'success')
        expect(status.failed?).to be false
      end
    end

    describe '#in_progress?' do
      it 'returns true for in_progress status' do
        status = described_class.new(status: 'in_progress')
        expect(status.in_progress?).to be true
      end

      it 'returns false for success status' do
        status = described_class.new(status: 'success')
        expect(status.in_progress?).to be false
      end
    end
  end

  describe TaskerCore::Types::StepTypes::StepExecutionError do
    subject(:error) do
      described_class.new(
        error_type: 'HandlerException',
        message: 'Handler raised an error',
        retryable: true,
        error_code: 'HANDLER_ERR',
        stack_trace: 'line1:1'
      )
    end

    it 'stores all attributes' do
      expect(error.error_type).to eq('HandlerException')
      expect(error.message).to eq('Handler raised an error')
      expect(error.retryable).to be true
      expect(error.error_code).to eq('HANDLER_ERR')
      expect(error.stack_trace).to eq('line1:1')
    end

    describe '#to_h' do
      it 'serializes to a hash with correct structure' do
        hash = error.to_h
        expect(hash[:message]).to eq('Handler raised an error')
        expect(hash[:error_type]).to eq('HandlerException')
        expect(hash[:retryable]).to be true
        expect(hash[:context][:error_code]).to eq('HANDLER_ERR')
        expect(hash[:context][:stack_trace]).to eq('line1:1')
      end

      it 'compacts nil context values' do
        err = described_class.new(
          error_type: 'ProcessingError',
          message: 'failed',
          retryable: false
        )
        hash = err.to_h
        expect(hash[:context]).to eq({})
      end
    end

    it 'defaults retryable to true' do
      err = described_class.new(
        error_type: 'UnexpectedError',
        message: 'unexpected'
      )
      expect(err.retryable).to be true
    end
  end

  describe TaskerCore::Types::StepTypes::StepResult do
    describe '.success' do
      subject(:result) do
        described_class.success(
          step_uuid: 'step-1',
          task_uuid: 'task-1',
          result_data: { validated: true },
          execution_time_ms: 150
        )
      end

      it 'creates a result with success status' do
        expect(result.status.success?).to be true
      end

      it 'stores step and task UUIDs' do
        expect(result.step_uuid).to eq('step-1')
        expect(result.task_uuid).to eq('task-1')
      end

      it 'stores result data' do
        expect(result.result_data).to eq({ validated: true })
      end

      it 'reports success via predicate' do
        expect(result.success?).to be true
        expect(result.failed?).to be false
      end
    end

    describe '.in_progress' do
      subject(:result) do
        described_class.in_progress(
          step_uuid: 'step-2',
          task_uuid: 'task-2',
          result_data: { partial: true }
        )
      end

      it 'creates a result with in_progress status' do
        expect(result.status.in_progress?).to be true
      end

      it 'is not success or failed' do
        expect(result.success?).to be false
        expect(result.failed?).to be false
      end
    end

    describe '.failure' do
      subject(:result) do
        described_class.failure(
          step_uuid: 'step-3',
          task_uuid: 'task-3',
          error: TaskerCore::Types::StepTypes::StepExecutionError.new(
            error_type: 'HandlerException',
            message: 'Something broke',
            retryable: true
          ),
          execution_time_ms: 50
        )
      end

      it 'creates a result with failed status' do
        expect(result.status.failed?).to be true
      end

      it 'reports failed via predicate' do
        expect(result.failed?).to be true
        expect(result.success?).to be false
      end

      it 'stores the error' do
        expect(result.error.message).to eq('Something broke')
        expect(result.error.error_type).to eq('HandlerException')
      end
    end

    describe '#to_h' do
      it 'maps status to Rust enum variants' do
        result = described_class.success(step_uuid: 's', task_uuid: 't')
        hash = result.to_h

        expect(hash[:status]).to eq('Success')
        expect(hash[:step_uuid]).to eq('s')
        expect(hash[:task_uuid]).to eq('t')
      end

      it 'maps failed status to Failed' do
        result = described_class.failure(
          step_uuid: 's',
          task_uuid: 't',
          error: TaskerCore::Types::StepTypes::StepExecutionError.new(
            error_type: 'ProcessingError',
            message: 'err'
          )
        )
        hash = result.to_h

        expect(hash[:status]).to eq('Failed')
      end

      it 'maps in_progress status to InProgress' do
        result = described_class.in_progress(step_uuid: 's', task_uuid: 't')
        hash = result.to_h

        expect(hash[:status]).to eq('InProgress')
      end

      it 'includes metadata with worker info' do
        result = described_class.success(step_uuid: 's', task_uuid: 't')
        hash = result.to_h

        expect(hash[:metadata]).to be_a(Hash)
        expect(hash[:metadata][:worker_id]).to match(/^ruby_worker_/)
        expect(hash[:metadata][:worker_hostname]).to be_a(String)
        expect(hash[:metadata][:started_at]).to be_a(String)
        expect(hash[:metadata][:completed_at]).to be_a(String)
      end
    end
  end

  describe TaskerCore::Types::StepTypes::StepCompletion do
    subject(:completion) do
      described_class.new(
        step_name: 'validate_order',
        status: 'complete',
        results: { validated: true },
        duration_ms: 1500,
        completed_at: Time.now.utc.iso8601,
        error_message: nil
      )
    end

    describe '#valid?' do
      it 'returns true for valid completion' do
        expect(completion.valid?).to be true
      end

      it 'returns false for empty step_name' do
        empty = described_class.new(
          step_name: '',
          status: 'complete',
          results: {},
          duration_ms: nil,
          completed_at: nil,
          error_message: nil
        )
        expect(empty.valid?).to be false
      end
    end

    describe '#completed?' do
      it 'returns true for complete status' do
        expect(completion.completed?).to be true
      end

      it 'returns false for failed status' do
        failed = described_class.new(
          step_name: 'step',
          status: 'failed',
          results: {},
          duration_ms: nil,
          completed_at: nil,
          error_message: 'err'
        )
        expect(failed.completed?).to be false
      end
    end

    describe '#failed?' do
      it 'returns true for failed status' do
        failed = described_class.new(
          step_name: 'step',
          status: 'failed',
          results: {},
          duration_ms: nil,
          completed_at: nil,
          error_message: 'err'
        )
        expect(failed.failed?).to be true
      end

      it 'returns false for complete status' do
        expect(completion.failed?).to be false
      end
    end

    describe '#pending?' do
      it 'returns true for pending status' do
        pending_step = described_class.new(
          step_name: 'step',
          status: 'pending',
          results: {},
          duration_ms: nil,
          completed_at: nil,
          error_message: nil
        )
        expect(pending_step.pending?).to be true
      end

      it 'returns false for complete status' do
        expect(completion.pending?).to be false
      end
    end

    describe '#duration_seconds' do
      it 'converts milliseconds to seconds' do
        expect(completion.duration_seconds).to eq(1.5)
      end

      it 'returns nil when duration_ms is nil' do
        no_duration = described_class.new(
          step_name: 'step',
          status: 'complete',
          results: {},
          duration_ms: nil,
          completed_at: nil,
          error_message: nil
        )
        expect(no_duration.duration_seconds).to be_nil
      end
    end

    describe '#to_s' do
      it 'includes step_name, status, and duration' do
        str = completion.to_s
        expect(str).to include('validate_order')
        expect(str).to include('complete')
        expect(str).to include('1.5')
      end
    end
  end
end
