# frozen_string_literal: true

require 'spec_helper'
require 'ostruct'

RSpec.describe TaskerCore::StepHandler::Base do
  class TestStepHandler < TaskerCore::StepHandler::Base
    def process(task, _sequence, step)
      {
        status: 'completed',
        result: 'step_result',
        task_id: task.respond_to?(:task_id) ? task.task_id : task['task_id'],
        step_id: step.respond_to?(:workflow_step_id) ? step.workflow_step_id : step['workflow_step_id']
      }
    end

    def process_results(_step, process_output, _initial_results = nil)
      process_output.merge({
                             enhanced: true,
                             timestamp: Time.now.iso8601,
                             retry_count: 0
                           })
    end
  end

  let(:handler) { TestStepHandler.new }
  let(:mock_task) { OpenStruct.new(task_id: 123, context: { 'test' => true }) }
  let(:mock_sequence) { { 'total_steps' => 3, 'current_position' => 1 } }
  let(:mock_step) { OpenStruct.new(workflow_step_id: 456, inputs: { 'test_input' => 'value' }) }

  describe '#initialize' do
    it 'initializes with default configuration' do
      expect(handler).to be_a(TestStepHandler)
      expect(handler.handler_name).to eq('test_step_handler')
    end

    it 'initializes with custom configuration' do
      custom_handler = TestStepHandler.new(
        config: { 'timeout' => 60 },
        logger: Logger.new(IO::NULL)
      )
      expect(custom_handler.config).to include('timeout' => 60)
    end
  end

  describe '#process' do
    it 'processes step with Rails-compatible signature' do
      result = handler.process(mock_task, mock_sequence, mock_step)

      expect(result).to be_a(Hash)
      expect(result[:status]).to eq('completed')
      expect(result[:result]).to eq('step_result')
      expect(result[:task_id]).to eq(123)
      expect(result[:step_id]).to eq(456)
    end
  end

  describe '#process_results' do
    it 'processes results with Rails-compatible signature' do
      process_output = { status: 'completed', result: 'test_result' }
      result = handler.process_results(mock_step, process_output, nil)

      expect(result).to be_a(Hash)
      expect(result[:status]).to eq('completed')
      expect(result[:enhanced]).to be true
      expect(result[:timestamp]).to be_a(String)
      expect(result[:retry_count]).to eq(0)
    end
  end

  describe '#process_with_context' do
    it 'processes step with Rust-compatible JSON context' do
      context_json = JSON.generate({
                                     step_id: 456,
                                     task_id: 123,
                                     step_name: 'test_step',
                                     input_data: { 'test' => 'data' },
                                     previous_steps: [],
                                     step_config: {},
                                     attempt_number: 1,
                                     max_retry_attempts: 3,
                                     timeout_seconds: 30,
                                     is_retryable: true,
                                     environment: 'test',
                                     metadata: {}
                                   })

      # Mock the FFI call with correct response format
      allow(TaskerCore).to receive(:call_ruby_process_method).and_return({
                                                                           'status' => 'completed',
                                                                           'result' => 'step_result'
                                                                         })

      result_json = handler.process_with_context(context_json)
      result = JSON.parse(result_json)

      expect(result).to be_a(Hash)
      expect(result['status']).to eq('completed')
      expect(result['result']).to eq('step_result')
    end

    it 'handles errors in process_with_context' do
      invalid_context = 'invalid_json'

      result_json = handler.process_with_context(invalid_context)
      result = JSON.parse(result_json)

      expect(result).to be_a(Hash)
      expect(result['error']).to be_a(Hash)
      # The error message comes from Rust FFI and mentions "StepExecutionContext"
      expect(result['error']['message']).to include('StepExecutionContext')
    end
  end

  describe '#process_results_with_context' do
    it 'processes results with Rust-compatible JSON context' do
      context_json = JSON.generate({
                                     step_id: 456,
                                     task_id: 123,
                                     step_name: 'test_step'
                                   })

      result_json = JSON.generate({
                                    output_data: { status: 'completed', result: 'step_result' }
                                  })

      response_json = handler.process_results_with_context(context_json, result_json)
      response = JSON.parse(response_json)

      expect(response).to be_a(Hash)
      expect(response['status']).to eq('success')
      expect(response['transformed_result']).to be_a(Hash)
      expect(response['transformed_result']['enhanced']).to be true
    end

    it 'handles errors in process_results_with_context' do
      invalid_context = 'invalid_json'
      valid_result = JSON.generate({ output_data: {} })

      response_json = handler.process_results_with_context(invalid_context, valid_result)
      response = JSON.parse(response_json)

      expect(response).to be_a(Hash)
      expect(response['status']).to eq('error')
      expect(response['error']).to be_a(Hash)
      # JSON parsing error should include "unexpected" or similar parsing error terms
      expect(response['error']['message']).to match(/unexpected|invalid|parse/i)
    end
  end

  describe '#metadata' do
    it 'returns handler metadata' do
      metadata = handler.metadata

      expect(metadata).to be_a(Hash)
      expect(metadata[:handler_name]).to eq('test_step_handler')
      expect(metadata[:handler_class]).to eq('TestStepHandler')
      expect(metadata[:version]).to be_a(String)
      expect(metadata[:capabilities]).to be_an(Array)
      expect(metadata[:capabilities]).to include('process')
      expect(metadata[:capabilities]).to include('process_results')
      expect(metadata[:ruby_version]).to eq(RUBY_VERSION)
    end
  end

  describe '#capabilities' do
    it 'returns handler capabilities' do
      capabilities = handler.capabilities

      expect(capabilities).to be_an(Array)
      expect(capabilities).to include('process')
      expect(capabilities).to include('process_results')
    end
  end

  describe '#config_schema' do
    it 'returns configuration schema' do
      schema = handler.config_schema

      expect(schema).to be_a(Hash)
      expect(schema[:type]).to eq('object')
      expect(schema[:properties]).to be_a(Hash)
      expect(schema[:properties][:timeout]).to be_a(Hash)
      expect(schema[:properties][:retries]).to be_a(Hash)
      expect(schema[:properties][:log_level]).to be_a(Hash)
    end
  end

  describe '#extract_step_name_from_class' do
    it 'extracts step name from class name' do
      step_name = handler.extract_step_name_from_class
      expect(step_name).to eq('test_step')
    end
  end

  # NOTE: Step handlers do not register themselves with the registry
  # They are discovered through task configuration by task handlers

  describe 'error handling' do
    class ErrorTestStepHandler < TaskerCore::StepHandler::Base
      def process(_task, _sequence, _step)
        raise StandardError, 'Test error'
      end
    end

    let(:error_handler) { ErrorTestStepHandler.new }

    it 'handles unexpected errors in process method' do
      context_json = JSON.generate({
                                     step_id: 456,
                                     task_id: 123,
                                     step_name: 'error_test_step'
                                   })

      # Mock the FFI call to raise an error
      allow(TaskerCore).to receive(:call_ruby_process_method).and_raise(StandardError.new('Test error'))

      result_json = error_handler.process_with_context(context_json)
      result = JSON.parse(result_json)

      expect(result).to be_a(Hash)
      expect(result['error']).to be_a(Hash)
      expect(result['error']['message']).to include('Test error')
      expect(result['error']['type']).to eq('StandardError')
    end
  end
end
