# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::Messaging::QueueWorker, type: :unit do
  let(:namespace) { 'test_namespace' }
  let(:mock_pgmq_client) { instance_double(TaskerCore::Messaging::PgmqClient) }
  let(:mock_sql_functions) { instance_double(TaskerCore::Database::SqlFunctions) }
  let(:mock_logger) { instance_double(TaskerCore::Logging::Logger) }

  let(:worker) do
    described_class.new(
      namespace,
      pgmq_client: mock_pgmq_client,
      sql_functions: mock_sql_functions,
      logger: mock_logger,
      poll_interval: 0.1
    )
  end

  let(:step_result) do
    TaskerCore::Types::StepResult.success(
      step_id: 12_345,
      task_id: 67_890,
      result_data: { processed: true, value: 42 },
      execution_time_ms: 150
    )
  end

  before do
    # Mock queue creation for namespace
    allow(mock_pgmq_client).to receive(:create_queue).with("#{namespace}_queue")
    allow(mock_logger).to receive(:debug)
    allow(mock_logger).to receive(:info)
    allow(mock_logger).to receive(:warn)
    allow(mock_logger).to receive(:error)
  end

  describe '#send_result_to_orchestration' do
    context 'when orchestration queue send succeeds' do
      before do
        allow(mock_pgmq_client).to receive(:create_queue).with('orchestration_step_results')
        allow(mock_pgmq_client).to receive(:send_message).and_return(98_765)
      end

      it 'sends result to orchestration_step_results queue' do
        result_msg_id = worker.send(:send_result_to_orchestration, step_result)

        expect(result_msg_id).to eq(98_765)
        expect(mock_pgmq_client).to have_received(:create_queue).with('orchestration_step_results')
        expect(mock_pgmq_client).to have_received(:send_message) do |queue_name, message|
          expect(queue_name).to eq('orchestration_step_results')
          expect(message[:step_id]).to eq(12_345)
          expect(message[:task_id]).to eq(67_890)
          expect(message[:namespace]).to eq(namespace)
          expect(message[:status]).to eq('success')
          expect(message[:result_data]).to eq({ processed: true, value: 42 })
          expect(message[:execution_time_ms]).to eq(150)
          expect(message[:processed_at]).to be_a(String)
          expect(message[:worker_id]).to include("#{namespace}_worker_")
        end
      end

      it 'logs successful orchestration send' do
        worker.send(:send_result_to_orchestration, step_result)

        expect(mock_logger).to have_received(:debug).with(
          '📤 QUEUE_WORKER: Sending result to orchestration - step_id: 12345, status: success'
        )
        expect(mock_logger).to have_received(:debug).with(
          '✅ QUEUE_WORKER: Result sent to orchestration - msg_id: 98765'
        )
      end
    end

    context 'when orchestration queue send fails' do
      before do
        allow(mock_pgmq_client).to receive(:create_queue).with('orchestration_step_results')
        allow(mock_pgmq_client).to receive(:send_message).and_raise(StandardError.new('Queue unavailable'))
      end

      it 'handles errors gracefully without re-raising' do
        expect do
          result = worker.send(:send_result_to_orchestration, step_result)
          expect(result).to be_nil
        end.not_to raise_error
      end

      it 'logs the error' do
        worker.send(:send_result_to_orchestration, step_result)

        expect(mock_logger).to have_received(:error).with(
          '❌ QUEUE_WORKER: Failed to send result to orchestration: Queue unavailable'
        )
      end
    end
  end
end
