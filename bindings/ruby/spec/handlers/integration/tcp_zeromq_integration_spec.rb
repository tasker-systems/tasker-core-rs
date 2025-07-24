# frozen_string_literal: true

require 'spec_helper'

RSpec.describe 'TCP ZeroMQ Integration', type: :integration do
  let(:manager) { TaskerCore::Internal::OrchestrationManager.instance }
  let(:handle) { manager.orchestration_handle }

  before do
    # Enable debug logging to see ZeroMQ communication
    ENV['RUST_LOG'] = 'debug'
  end

  describe 'FFI Bootstrap Architecture' do
    it 'creates OrchestrationManager with handle-based architecture' do
      expect(manager).to be_initialized
      expect(handle).not_to be_nil
    end

    it 'successfully creates orchestration handle' do
      expect(handle).to be_a(TaskerCore::OrchestrationHandle)
    end
  end

  describe 'ZeroMQ Configuration' do
    context 'when ZeroMQ is enabled' do
      before do
        skip 'ZeroMQ not enabled' unless handle.is_zeromq_enabled
      end

      it 'provides ZeroMQ configuration' do
        zeromq_config = handle.zeromq_config
        expect(zeromq_config).to be_a(Hash)
        expect(zeromq_config).to have_key('batch_endpoint')
        expect(zeromq_config).to have_key('result_endpoint')
      end

      it 'provides ZMQ context information' do
        context_info = handle.zmq_context
        expect(context_info).to be_a(Hash)
        expect(context_info['communication_mode']).to eq('tcp')
      end
    end

    context 'when ZeroMQ is disabled' do
      before do
        skip 'ZeroMQ is enabled' if handle.is_zeromq_enabled
      end

      it 'reports ZeroMQ as disabled' do
        expect(handle.is_zeromq_enabled).to be false
      end
    end
  end

  describe 'ZeroMQ Integration Status' do
    it 'provides integration status information' do
      status = manager.zeromq_integration_status
      expect(status).to be_a(Hash)
      expect(status).to have_key(:enabled)
    end

    context 'when integration is available' do
      let(:status) { manager.zeromq_integration_status }

      before do
        skip 'ZeroMQ integration not available' unless status[:enabled]
      end

      it 'includes configuration details' do
        expect(status[:zeromq_config]).to be_a(Hash)
      end
    end
  end

  describe 'ZeroMQ Integration Lifecycle', :zeromq do
    let(:status) { manager.zeromq_integration_status }

    before do
      skip 'ZeroMQ integration not available' unless status[:enabled]
    end

    after do
      manager.stop_zeromq_integration if manager.zeromq_integration_status[:running]
    end

    it 'successfully starts ZeroMQ integration' do
      integration_started = manager.start_zeromq_integration
      expect(integration_started).to be true
    end

    context 'when integration is started' do
      before do
        manager.start_zeromq_integration
        sleep(0.5) # Allow initialization time
      end

      it 'creates and runs BatchStepExecutionOrchestrator' do
        orchestrator = manager.batch_step_orchestrator
        expect(orchestrator).not_to be_nil

        stats = orchestrator.stats
        expect(stats).to be_a(Hash)
        expect(stats[:running]).to be true
        expect(stats[:max_workers]).to be > 0
      end

      describe 'batch communication' do
        let(:test_batch) do
          {
            batch_id: "test_batch_#{Time.now.to_i}",
            protocol_version: "2.0",
            created_at: Time.now.utc.iso8601,
            steps: [
              {
                step_id: 1001,
                task_id: 500,
                step_name: "test_step",
                handler_class: "TestHandler",
                handler_config: { timeout_seconds: 30 },
                task_context: { test: "data" },
                previous_results: {},
                metadata: { sequence: 1, total_steps: 1 }
              }
            ]
          }
        end

        it 'successfully publishes batch messages' do
          publish_result = handle.publish_batch(test_batch)
          expect(publish_result).to be true
        end

        it 'can receive result messages' do
          results = handle.receive_results
          expect(results).to be_an(Array)
        end
      end

      it 'gracefully stops integration' do
        expect { manager.stop_zeromq_integration }.not_to raise_error
      end
    end
  end

  describe 'Cross-language Architecture Validation' do
    it 'demonstrates proper FFI bootstrap sequencing' do
      # Ruby VM → Rust dylib → Socket setup sequence
      expect(manager).to be_initialized
      expect(handle).not_to be_nil
      
      if handle.is_zeromq_enabled
        expect(handle.zeromq_config).to include('batch_endpoint', 'result_endpoint')
        expect(handle.zmq_context['communication_mode']).to eq('tcp')
      end
    end

    it 'validates TCP-based communication architecture' do
      skip 'ZeroMQ not enabled' unless handle.is_zeromq_enabled
      
      config = handle.zeromq_config
      expect(config['batch_endpoint']).to start_with('tcp://')
      expect(config['result_endpoint']).to start_with('tcp://')
    end
  end
end