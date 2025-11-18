# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::BatchProcessing::BatchWorkerContext do
  let(:mock_workflow_step) { instance_double(TaskerCore::Models::WorkflowStepWrapper) }

  describe '.from_step_data' do
    it 'extracts context from workflow step wrapper' do
      allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                 'cursor' => {
                                                                   'batch_id' => '001',
                                                                   'start_cursor' => 0,
                                                                   'end_cursor' => 100
                                                                 },
                                                                 'batch_metadata' => {
                                                                   'checkpoint_interval' => 50
                                                                 }
                                                               })

      context = described_class.from_step_data(mock_workflow_step)

      expect(context).to be_a(described_class)
      expect(context.no_op?).to be false
      expect(context.batch_id).to eq('001')
      expect(context.start_cursor).to eq(0)
      expect(context.end_cursor).to eq(100)
      expect(context.checkpoint_interval).to eq(50)
    end

    it 'handles symbol keys correctly' do
      allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                 cursor: {
                                                                   batch_id: '002',
                                                                   start_cursor: 100,
                                                                   end_cursor: 200
                                                                 },
                                                                 batch_metadata: {}
                                                               })

      context = described_class.from_step_data(mock_workflow_step)

      expect(context.batch_id).to eq('002')
      expect(context.start_cursor).to eq(100)
      expect(context.end_cursor).to eq(200)
    end
  end

  describe '#no_op?' do
    context 'when is_no_op is true' do
      it 'returns true for placeholder worker' do
        allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                   'is_no_op' => true
                                                                 })

        context = described_class.from_step_data(mock_workflow_step)

        expect(context.no_op?).to be true
        expect(context.batch_id).to eq('unknown')
        expect(context.start_cursor).to eq(0)
        expect(context.end_cursor).to eq(0)
      end
    end

    context 'when is_no_op is false' do
      it 'returns false for real worker' do
        allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                   'is_no_op' => false,
                                                                   'cursor' => {
                                                                     'batch_id' => '001',
                                                                     'start_cursor' => 0,
                                                                     'end_cursor' => 100
                                                                   }
                                                                 })

        context = described_class.from_step_data(mock_workflow_step)

        expect(context.no_op?).to be false
      end
    end

    context 'when is_no_op is missing' do
      it 'defaults to false' do
        allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                   'cursor' => {
                                                                     'batch_id' => '001',
                                                                     'start_cursor' => 0,
                                                                     'end_cursor' => 100
                                                                   }
                                                                 })

        context = described_class.from_step_data(mock_workflow_step)

        expect(context.no_op?).to be false
      end
    end
  end

  describe '#start_cursor' do
    it 'extracts start_cursor from cursor config' do
      allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                 'cursor' => {
                                                                   'batch_id' => '001',
                                                                   'start_cursor' => 42,
                                                                   'end_cursor' => 100
                                                                 }
                                                               })

      context = described_class.from_step_data(mock_workflow_step)

      expect(context.start_cursor).to eq(42)
    end

    it 'converts string to integer' do
      allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                 'cursor' => {
                                                                   'batch_id' => '001',
                                                                   'start_cursor' => '123',
                                                                   'end_cursor' => '456'
                                                                 }
                                                               })

      context = described_class.from_step_data(mock_workflow_step)

      expect(context.start_cursor).to eq(123)
    end

    it 'defaults to 0 when missing' do
      allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                 'is_no_op' => true
                                                               })

      context = described_class.from_step_data(mock_workflow_step)

      expect(context.start_cursor).to eq(0)
    end
  end

  describe '#end_cursor' do
    it 'extracts end_cursor from cursor config' do
      allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                 'cursor' => {
                                                                   'batch_id' => '001',
                                                                   'start_cursor' => 0,
                                                                   'end_cursor' => 500
                                                                 }
                                                               })

      context = described_class.from_step_data(mock_workflow_step)

      expect(context.end_cursor).to eq(500)
    end

    it 'converts string to integer' do
      allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                 'cursor' => {
                                                                   'batch_id' => '001',
                                                                   'start_cursor' => '0',
                                                                   'end_cursor' => '789'
                                                                 }
                                                               })

      context = described_class.from_step_data(mock_workflow_step)

      expect(context.end_cursor).to eq(789)
    end
  end

  describe '#batch_id' do
    it 'extracts batch_id from cursor config' do
      allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                 'cursor' => {
                                                                   'batch_id' => '007',
                                                                   'start_cursor' => 0,
                                                                   'end_cursor' => 100
                                                                 }
                                                               })

      context = described_class.from_step_data(mock_workflow_step)

      expect(context.batch_id).to eq('007')
    end

    it 'returns "unknown" when missing' do
      allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                 'is_no_op' => true
                                                               })

      context = described_class.from_step_data(mock_workflow_step)

      expect(context.batch_id).to eq('unknown')
    end
  end

  describe '#checkpoint_interval' do
    it 'extracts checkpoint_interval from batch_metadata' do
      allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                 'cursor' => {
                                                                   'batch_id' => '001',
                                                                   'start_cursor' => 0,
                                                                   'end_cursor' => 100
                                                                 },
                                                                 'batch_metadata' => {
                                                                   'checkpoint_interval' => 25
                                                                 }
                                                               })

      context = described_class.from_step_data(mock_workflow_step)

      expect(context.checkpoint_interval).to eq(25)
    end

    it 'defaults to 100 when missing' do
      allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                 'cursor' => {
                                                                   'batch_id' => '001',
                                                                   'start_cursor' => 0,
                                                                   'end_cursor' => 100
                                                                 },
                                                                 'batch_metadata' => {}
                                                               })

      context = described_class.from_step_data(mock_workflow_step)

      expect(context.checkpoint_interval).to eq(100)
    end

    it 'converts string to integer' do
      allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                 'cursor' => {
                                                                   'batch_id' => '001',
                                                                   'start_cursor' => 0,
                                                                   'end_cursor' => 100
                                                                 },
                                                                 'batch_metadata' => {
                                                                   'checkpoint_interval' => '75'
                                                                 }
                                                               })

      context = described_class.from_step_data(mock_workflow_step)

      expect(context.checkpoint_interval).to eq(75)
    end
  end

  describe 'validation' do
    context 'when cursor configuration is invalid' do
      it 'raises ArgumentError when cursor is empty for non-noop worker' do
        allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                   'is_no_op' => false,
                                                                   'cursor' => {}
                                                                 })

        expect do
          described_class.from_step_data(mock_workflow_step)
        end.to raise_error(ArgumentError, 'Missing cursor configuration')
      end

      it 'raises ArgumentError when batch_id is missing' do
        allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                   'cursor' => {
                                                                     'start_cursor' => 0,
                                                                     'end_cursor' => 100
                                                                   }
                                                                 })

        expect do
          described_class.from_step_data(mock_workflow_step)
        end.to raise_error(ArgumentError, 'Missing batch_id')
      end

      it 'raises ArgumentError when start_cursor is missing' do
        allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                   'cursor' => {
                                                                     'batch_id' => '001',
                                                                     'end_cursor' => 100
                                                                   }
                                                                 })

        expect do
          described_class.from_step_data(mock_workflow_step)
        end.to raise_error(ArgumentError, 'Missing start_cursor')
      end

      it 'raises ArgumentError when end_cursor is missing' do
        allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                   'cursor' => {
                                                                     'batch_id' => '001',
                                                                     'start_cursor' => 0
                                                                   }
                                                                 })

        expect do
          described_class.from_step_data(mock_workflow_step)
        end.to raise_error(ArgumentError, 'Missing end_cursor')
      end
    end

    context 'when worker is no-op' do
      it 'does not validate cursor configuration' do
        allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                   'is_no_op' => true
                                                                 })

        expect do
          described_class.from_step_data(mock_workflow_step)
        end.not_to raise_error
      end
    end
  end

  describe 'edge cases' do
    it 'handles nil inputs gracefully for no-op worker' do
      allow(mock_workflow_step).to receive(:inputs).and_return(nil)

      expect do
        described_class.from_step_data(mock_workflow_step)
      end.to raise_error(ArgumentError)
    end

    it 'handles zero values in cursor range' do
      allow(mock_workflow_step).to receive(:inputs).and_return({
                                                                 'cursor' => {
                                                                   'batch_id' => '000',
                                                                   'start_cursor' => 0,
                                                                   'end_cursor' => 0
                                                                 }
                                                               })

      context = described_class.from_step_data(mock_workflow_step)

      expect(context.start_cursor).to eq(0)
      expect(context.end_cursor).to eq(0)
    end
  end
end
