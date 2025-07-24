# frozen_string_literal: true

require 'spec_helper'
require 'json'
require 'timeout'

# Explicitly require ZeroMQ execution module
require_relative '../../../lib/tasker_core/execution'

RSpec.describe 'ZeroMQ Message Flow', type: :integration do
  describe 'Message Protocol Communication' do
    let(:logger) { Logger.new($stdout, level: Logger::DEBUG) }
    
    before do
      # Skip if ZeroMQ not available
      skip "ZeroMQ not available" unless zeromq_available?
    end

    describe 'Direct Message Processing' do
      it 'processes step messages directly without handler infrastructure' do
        # Create a custom ZeroMQ handler that returns predictable results
        handler = create_test_message_handler
        
        # Start the handler
        handler.start
        expect(handler.running?).to be true

        # Wait for handler to be ready
        sleep(0.1)

        # Create a simple step execution request
        step_request = {
          batch_id: SecureRandom.uuid,
          protocol_version: '1.0',
          steps: [
            {
              step_id: 456,
              step_name: 'test_step',
              task_id: 123,
              task_context: { order_id: 'ORD-001' },
              handler_config: {},
              previous_results: {},
              metadata: {}
            }
          ]
        }

        # Send request and wait for response
        response = simulate_rust_step_request(step_request, handler)

        # Validate the response structure
        expect(response).to include(:batch_id, :protocol_version, :results)
        expect(response[:batch_id]).to eq(step_request[:batch_id])
        expect(response[:results]).to be_an(Array)
        expect(response[:results].length).to eq(1)

        result = response[:results].first
        expect(result).to include(:step_id, :status, :output, :metadata)
        expect(result[:step_id]).to eq(456)
        expect(result[:status]).to eq('completed')
        expect(result[:output]).to include(test_result: 'success')

        # Stop the handler
        handler.stop(timeout: 2)
      end

      it 'handles message parsing errors gracefully' do
        handler = create_test_message_handler
        handler.start
        sleep(0.1)

        # Send invalid JSON
        invalid_request = "steps {invalid json}"
        
        context = ZMQ::Context.new
        publisher = context.socket(ZMQ::PUB)
        publisher.bind('inproc://test_steps')
        
        subscriber = context.socket(ZMQ::SUB)
        subscriber.connect('inproc://test_results')
        subscriber.setsockopt(ZMQ::SUBSCRIBE, 'results')
        
        sleep(0.05)
        
        publisher.send_string(invalid_request)
        
        # Should receive error response
        response = nil
        Timeout.timeout(2) do
          message = String.new
          loop do
            rc = subscriber.recv_string(message, ZMQ::DONTWAIT)
            if ZMQ::Util.resultcode_ok?(rc)
              parts = message.split(' ', 2)
              if parts.length == 2 && parts[0] == 'results'
                response = JSON.parse(parts[1], symbolize_names: true)
                break
              end
            end
            sleep(0.01)
          end
        end

        expect(response).to include(:error)
        expect(response[:error][:error_type]).to eq('JSON::ParserError')

        # Cleanup
        subscriber.close
        publisher.close
        context.terminate
        handler.stop(timeout: 2)
      end

      it 'processes multiple steps in sequence' do
        handler = create_test_message_handler
        handler.start
        sleep(0.1)

        # Create request with multiple steps
        step_request = {
          batch_id: SecureRandom.uuid,
          protocol_version: '1.0',
          steps: [
            { step_id: 1001, step_name: 'step_1', task_id: 123 },
            { step_id: 1002, step_name: 'step_2', task_id: 123 },
            { step_id: 1003, step_name: 'step_3', task_id: 123 }
          ]
        }

        response = simulate_rust_step_request(step_request, handler)

        expect(response[:results].length).to eq(3)
        
        step_ids = response[:results].map { |r| r[:step_id] }
        expect(step_ids).to contain_exactly(1001, 1002, 1003)
        
        response[:results].each do |result|
          expect(result[:status]).to eq('completed')
          expect(result[:output]).to include(test_result: 'success')
        end

        handler.stop(timeout: 2)
      end
    end

    private

    def zeromq_available?
      require 'ffi-rzmq'
      true
    rescue LoadError
      false
    end

    # Create a test handler that overrides step processing to return predictable results
    def create_test_message_handler
      Class.new(TaskerCore::Execution::ZeroMQHandler) do
        def initialize
          super(
            step_sub_endpoint: 'inproc://test_steps',
            result_pub_endpoint: 'inproc://test_results',
            logger: Logger.new($stdout, level: Logger::DEBUG)
          )
        end

        private

        # Override step processing to return test results without handler lookup
        def process_single_step(step)
          start_time = Time.now
          step_id = step[:step_id]
          
          # Simulate some processing time
          sleep(0.01)
          
          execution_time_ms = ((Time.now - start_time) * 1000).round

          {
            step_id: step_id,
            status: 'completed',
            output: {
              test_result: 'success',
              step_name: step[:step_name],
              processed_at: Time.now.utc.iso8601
            },
            error: nil,
            metadata: {
              execution_time_ms: execution_time_ms,
              handler_version: 'test-1.0.0',
              retryable: false,
              completed_at: Time.now.utc.iso8601
            }
          }
        rescue => e
          execution_time_ms = ((Time.now - start_time) * 1000).round
          
          {
            step_id: step_id,
            status: 'failed',
            output: nil,
            error: {
              message: e.message,
              error_type: e.class.name,
              retryable: true
            },
            metadata: {
              execution_time_ms: execution_time_ms,
              retryable: true,
              completed_at: Time.now.utc.iso8601
            }
          }
        end
      end.new
    end

    # Simulate Rust sending a step request and receiving the response
    def simulate_rust_step_request(request, handler)
      response = nil
      result_collected = false

      # Set up temporary sockets for this test
      context = ZMQ::Context.new
      
      # Publisher to send steps (simulates Rust)
      step_publisher = context.socket(ZMQ::PUB)
      step_publisher.bind('inproc://test_steps')
      
      # Subscriber to receive results (simulates Rust)
      result_subscriber = context.socket(ZMQ::SUB)
      result_subscriber.connect('inproc://test_results')
      result_subscriber.setsockopt(ZMQ::SUBSCRIBE, 'results')

      # Brief delay to allow sockets to connect
      sleep(0.05)

      # Send the step request
      message = "steps #{request.to_json}"
      step_publisher.send_string(message)

      # Wait for response with timeout
      Timeout.timeout(3) do
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
      # Cleanup on error
      result_subscriber&.close
      step_publisher&.close
      context&.terminate
      raise "No response received from ZeroMQ handler within timeout"
    end
  end
end