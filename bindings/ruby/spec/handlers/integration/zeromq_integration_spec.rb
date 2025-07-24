# frozen_string_literal: true

require 'spec_helper'
require 'json'
require 'timeout'

RSpec.describe 'ZeroMQ Integration', type: :integration do
  describe 'Rust-Ruby ZeroMQ Step Execution' do
    let(:logger) { Logger.new($stdout, level: Logger::DEBUG) }
    let(:zeromq_handler) { TaskerCore::Execution::ZeroMQHandler.new(logger: logger) }
    let(:task_id) { 123 }
    let(:step_id) { 456 }

    before do
      # Skip if ZeroMQ not available
      skip "ZeroMQ not available" unless zeromq_available?
    end

    after do
      zeromq_handler.stop(timeout: 2) if zeromq_handler.running?
    end

    describe 'Single Step Execution' do
      it 'executes a simple step through ZeroMQ pub-sub' do
        # Start the Ruby ZeroMQ handler
        zeromq_handler.start
        expect(zeromq_handler.running?).to be true

        # Wait for handler to be ready
        sleep(0.1)

        # Create a simple step execution request
        step_request = {
          batch_id: SecureRandom.uuid,
          protocol_version: '1.0',
          steps: [
            {
              step_id: step_id,
              step_name: 'validate_order',
              task_id: task_id,
              task_context: { order_id: 'ORD-001', customer_id: 'CUST-123' },
              handler_config: { timeout: 30 },
              previous_results: {},
              metadata: { created_at: Time.now.utc.iso8601 }
            }
          ]
        }

        # Mock the step handler to return predictable results
        mock_step_handler = double('MockStepHandler')
        allow(mock_step_handler).to receive(:process).and_return({ status: 'validated', order_valid: true })
        allow(mock_step_handler).to receive_message_chain(:class, :const_get).with(:VERSION).and_return('1.0.0')

        # Mock the handler registry to return our mock handler
        mock_task_handler = double('MockTaskHandler')
        allow(mock_task_handler).to receive(:get_step_handler_from_name).with('validate_order').and_return(mock_step_handler)
        
        mock_registry = double('MockRegistry')
        allow(mock_registry).to receive(:get_task_handler_for_task).with(task_id).and_return(mock_task_handler)

        # Inject the mock registry into the handler
        allow(TaskerCore::Internal::OrchestrationManager).to receive(:instance).and_return(mock_registry)

        # Simulate Rust sending a step batch through ZeroMQ
        response = simulate_rust_step_request(step_request)

        # Validate the response
        expect(response).to include(:batch_id, :protocol_version, :results)
        expect(response[:batch_id]).to eq(step_request[:batch_id])
        expect(response[:results]).to be_an(Array)
        expect(response[:results].length).to eq(1)

        result = response[:results].first
        expect(result).to include(:step_id, :status, :output, :metadata)
        expect(result[:step_id]).to eq(step_id)
        expect(result[:status]).to eq('completed')
        expect(result[:output]).to include(status: 'validated', order_valid: true)
        expect(result[:metadata]).to include(:execution_time_ms, :completed_at)
      end

      it 'handles step execution errors gracefully' do
        zeromq_handler.start
        expect(zeromq_handler.running?).to be true

        sleep(0.1)

        # Create a step request that will cause an error
        step_request = {
          batch_id: SecureRandom.uuid,
          protocol_version: '1.0',
          steps: [
            {
              step_id: step_id,
              step_name: 'failing_step',
              task_id: task_id,
              task_context: {},
              handler_config: {},
              previous_results: {},
              metadata: {}
            }
          ]
        }

        # Mock a failing step handler
        mock_step_handler = double('FailingStepHandler')
        allow(mock_step_handler).to receive(:process).and_raise(StandardError.new('Simulated step failure'))

        mock_task_handler = double('MockTaskHandler')
        allow(mock_task_handler).to receive(:get_step_handler_from_name).with('failing_step').and_return(mock_step_handler)
        
        mock_registry = double('MockRegistry')
        allow(mock_registry).to receive(:get_task_handler_for_task).with(task_id).and_return(mock_task_handler)

        allow(TaskerCore::Internal::OrchestrationManager).to receive(:instance).and_return(mock_registry)

        # Send the failing step request
        response = simulate_rust_step_request(step_request)

        # Validate error handling
        expect(response[:results].length).to eq(1)
        result = response[:results].first
        expect(result[:status]).to eq('failed')
        expect(result[:error]).to include(:message, :error_type)
        expect(result[:error][:message]).to eq('Simulated step failure')
        expect(result[:error][:error_type]).to eq('StandardError')
        expect(result[:metadata][:retryable]).to be true
      end

      it 'processes multiple steps in a batch' do
        zeromq_handler.start
        expect(zeromq_handler.running?).to be true

        sleep(0.1)

        # Create a batch with multiple steps
        step_request = {
          batch_id: SecureRandom.uuid,
          protocol_version: '1.0',
          steps: [
            {
              step_id: 1001,
              step_name: 'validate_order',
              task_id: task_id,
              task_context: { order_id: 'ORD-001' },
              handler_config: {},
              previous_results: {},
              metadata: {}
            },
            {
              step_id: 1002,
              step_name: 'check_inventory',
              task_id: task_id,
              task_context: { order_id: 'ORD-001' },
              handler_config: {},
              previous_results: {},
              metadata: {}
            }
          ]
        }

        # Mock multiple step handlers
        validate_handler = double('ValidateHandler')
        allow(validate_handler).to receive(:process).and_return({ validation: 'passed' })
        allow(validate_handler).to receive_message_chain(:class, :const_get).with(:VERSION).and_return('1.0.0')

        inventory_handler = double('InventoryHandler')
        allow(inventory_handler).to receive(:process).and_return({ inventory: 'available' })
        allow(inventory_handler).to receive_message_chain(:class, :const_get).with(:VERSION).and_return('1.0.0')

        mock_task_handler = double('MockTaskHandler')
        allow(mock_task_handler).to receive(:get_step_handler_from_name).with('validate_order').and_return(validate_handler)
        allow(mock_task_handler).to receive(:get_step_handler_from_name).with('check_inventory').and_return(inventory_handler)
        
        mock_registry = double('MockRegistry')
        allow(mock_registry).to receive(:get_task_handler_for_task).with(task_id).and_return(mock_task_handler)

        allow(TaskerCore::Internal::OrchestrationManager).to receive(:instance).and_return(mock_registry)

        # Send the batch request
        response = simulate_rust_step_request(step_request)

        # Validate batch processing
        expect(response[:results].length).to eq(2)
        
        validate_result = response[:results].find { |r| r[:step_id] == 1001 }
        expect(validate_result[:status]).to eq('completed')
        expect(validate_result[:output]).to include(validation: 'passed')

        inventory_result = response[:results].find { |r| r[:step_id] == 1002 }
        expect(inventory_result[:status]).to eq('completed')
        expect(inventory_result[:output]).to include(inventory: 'available')
      end
    end

    describe 'Handler Lifecycle' do
      it 'starts and stops gracefully' do
        expect(zeromq_handler.running?).to be false

        thread = zeromq_handler.start
        expect(thread).to be_a(Thread)
        expect(zeromq_handler.running?).to be true

        stopped = zeromq_handler.stop(timeout: 1)
        expect(stopped).to be true
        expect(zeromq_handler.running?).to be false
      end

      it 'handles forced stop on timeout' do
        zeromq_handler.start
        expect(zeromq_handler.running?).to be true

        # Mock the thread to not respond to join
        allow(zeromq_handler.instance_variable_get(:@thread)).to receive(:join).and_return(nil)

        stopped = zeromq_handler.stop(timeout: 0.1)
        expect(stopped).to be false # Forced stop due to timeout
      end
    end

    private

    # Check if ZeroMQ is available in the environment
    def zeromq_available?
      require 'ffi-rzmq'
      true
    rescue LoadError
      false
    end

    # Simulate Rust sending a step request through ZeroMQ
    # In a real integration test, this would be done by the Rust ZmqPubSubExecutor
    def simulate_rust_step_request(request)
      response = nil
      result_collected = false

      # Set up a temporary subscriber to collect the response
      context = ZMQ::Context.new
      result_subscriber = context.socket(ZMQ::SUB)
      result_subscriber.connect('inproc://results')
      result_subscriber.setsockopt(ZMQ::SUBSCRIBE, 'results')

      # Set up a publisher to send the step request
      step_publisher = context.socket(ZMQ::PUB)
      step_publisher.bind('inproc://steps')

      # Brief delay to allow sockets to connect
      sleep(0.05)

      # Send the step request
      message = "steps #{request.to_json}"
      step_publisher.send_string(message)

      # Wait for response with timeout
      Timeout.timeout(5) do
        until result_collected
          message = String.new
          rc = result_subscriber.recv_string(message, ZMQ::DONTWAIT)
          
          if ZMQ::Util.resultcode_ok?(rc)
            parts = message.split(' ', 2)
            if parts.length == 2 && parts[0] == 'results'
              response = JSON.parse(parts[1], symbolize_names: true)
              result_collected = true
            end
          else
            sleep(0.01) # Brief pause before checking again
          end
        end
      end

      # Cleanup
      result_subscriber.close
      step_publisher.close
      context.terminate

      response
    rescue Timeout::Error
      raise "No response received from ZeroMQ handler within timeout"
    ensure
      # Ensure cleanup even on error
      result_subscriber&.close
      step_publisher&.close
      context&.terminate
    end
  end
end