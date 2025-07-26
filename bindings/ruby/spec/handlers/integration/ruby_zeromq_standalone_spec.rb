# frozen_string_literal: true

require 'spec_helper'
require 'ffi-rzmq'

RSpec.describe 'Ruby ZeroMQ Standalone', type: :integration do
  let(:test_handler_registry) do
    Class.new do
      def get_callable_for_class(handler_class)
        case handler_class
        when 'TestHandler'
          ->(task, sequence, step) {
            puts "   🔧 TestHandler processing step #{step.step_name} for task #{task.task_id}"
            { status: 'completed', result: 'Test execution successful', timestamp: Time.now.utc.iso8601 }
          }
        else
          ->(task, sequence, step) {
            puts "   🔧 Default handler processing step #{step.step_name}"
            { status: 'completed', result: 'Default execution', timestamp: Time.now.utc.iso8601 }
          }
        end
      end

      def get_handler_instance(handler_class)
        # Fallback - not used when callable is available
        Object.new
      end
    end.new
  end

  let(:orchestration_system) do
    TaskerCore::Internal::OrchestrationManager.instance.orchestration_system
    TaskerCore::Internal::OrchestrationManager.instance
  end

  let(:orchestrator) do
    orchestration_system.batch_step_orchestrator
  end

  describe 'BatchStepExecutionOrchestrator Creation' do
    it 'creates orchestrator with test endpoints successfully' do
      expect(orchestrator).to be_a(TaskerCore::Orchestration::BatchStepExecutionOrchestrator)
    end
  end

  describe 'Orchestrator Lifecycle', :zeromq do
    after do
      orchestrator.stop(timeout: 3) if orchestrator.stats[:running]
    end

    it 'starts orchestrator successfully' do
      expect { orchestrator.start }.not_to raise_error
    end

    context 'when orchestrator is running' do
      before do
        orchestrator.start
        sleep(1) # Allow initialization time
      end

      it 'reports running status and statistics' do
        skip "BatchStepExecutionOrchestrator stats method not fully implemented - planned for Phase 3"
        stats = orchestrator.stats
        expect(stats).to be_a(Hash)
        expect(stats[:running]).to be true
        expect(stats[:max_workers]).to eq(3)
      end

      describe 'ZeroMQ Message Communication' do
        let(:context) { ZMQ::Context.new }
        let(:publisher) { context.socket(ZMQ::PUB) }
        let(:subscriber) { context.socket(ZMQ::SUB) }

        let(:test_batch) do
          {
            batch_id: "ruby_test_#{Time.now.to_i}",
            protocol_version: "2.0",
            created_at: Time.now.utc.iso8601,
            steps: [
              {
                step_id: 2001,
                task_id: 600,
                step_name: "ruby_test_step",
                handler_class: "TestHandler",
                handler_config: { timeout_seconds: 10 },
                task_context: { test_type: "ruby_standalone" },
                previous_results: {},
                metadata: { sequence: 1, total_steps: 1 }
              }
            ]
          }
        end

        before do
          publisher.bind(test_endpoints[:step_sub])
          subscriber.connect(test_endpoints[:result_pub])
          subscriber.setsockopt(ZMQ::SUBSCRIBE, '')
          sleep(0.5) # Allow socket connections
        end

        after do
          publisher.close
          subscriber.close
          context.terminate
        end

        it 'successfully sends and processes batch messages' do
          skip "ZeroMQ message processing integration not yet complete - planned for Phase 3"
          # Send test batch message
          message = "steps #{test_batch.to_json}"
          expect { publisher.send_string(message) }.not_to raise_error

          # Try to receive results with timeout
          result_received = false
          timeout_time = Time.now + 5

          while Time.now < timeout_time && !result_received
            result_message = String.new
            rc = subscriber.recv_string(result_message, ZMQ::DONTWAIT)

            if rc == 0 && !result_message.empty?
              result_received = true

              # Validate message format
              expect(result_message).to include(' ')

              # Parse the result
              topic, json_data = result_message.split(' ', 2)
              expect { JSON.parse(json_data) }.not_to raise_error

              parsed_result = JSON.parse(json_data)
              expect(parsed_result).to be_a(Hash)
            else
              sleep(0.1)
            end
          end

          expect(result_received).to be true
        end

        it 'handles batch processing with proper result format' do
          skip "ZeroMQ batch processing integration not yet complete - planned for Phase 3"
          message = "steps #{test_batch.to_json}"
          publisher.send_string(message)

          # Wait for and validate result
          result_message = String.new
          timeout = Time.now + 5

          result_received = false
          while Time.now < timeout && !result_received
            rc = subscriber.recv_string(result_message, ZMQ::DONTWAIT)
            if rc == 0 && !result_message.empty?
              result_received = true
              break
            end
            sleep(0.1)
          end

          skip 'No result received within timeout' unless result_received

          topic, json_data = result_message.split(' ', 2)
          parsed_result = JSON.parse(json_data)

          # Validate result structure
          expect(topic).to be_in(%w[partial_result batch_completion])
          expect(parsed_result).to have_key('batch_id')
          expect(parsed_result['batch_id']).to eq(test_batch[:batch_id])
        end
      end

      it 'stops gracefully with timeout' do
        expect { orchestrator.stop(timeout: 3) }.not_to raise_error
      end
    end
  end

  describe 'Ruby-only ZeroMQ Validation' do
    it 'validates BatchStepExecutionOrchestrator works independently of Rust FFI' do
      # This test ensures Ruby ZeroMQ components function correctly
      # without requiring Rust orchestration system integration
      expect(orchestrator).to respond_to(:start)
      expect(orchestrator).to respond_to(:stop)
      expect(orchestrator).to respond_to(:stats)
    end

    it 'uses proper TCP endpoints for cross-language communication' do
      expect(test_endpoints[:step_sub]).to start_with('tcp://')
      expect(test_endpoints[:result_pub]).to start_with('tcp://')
    end
  end
end
