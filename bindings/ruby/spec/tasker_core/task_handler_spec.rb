# frozen_string_literal: true

require 'spec_helper'
require 'ostruct'

RSpec.describe TaskerCore::TaskHandler::Base do
  include TaskerCore::TestHelpers

  let(:task_config) do
    {
      'name' => 'test_task',
      'task_handler_class' => 'TestTaskHandler',
      'namespace_name' => 'default',
      'version' => '1.0.0',
      'named_steps' => ['test_step'],
      'step_templates' => [
        {
          'name' => 'test_step',
          'handler_class' => 'TestStepHandler',
          'handler_config' => { 'timeout' => 30 }
        }
      ]
    }
  end
  let(:handler) { TestTaskHandler.new(task_config: task_config) }
  # Create real task and workflow step objects using our factory system
  let(:real_task) do
    create_test_task(
      'name' => 'integration_test_task',
      'context' => { 'test' => true },
      'namespace_name' => 'default'
    )
  end
  let(:real_workflow_step) do
    create_test_workflow_step(
      'task_id' => real_task['task_id'],
      'name' => 'test_step',
      'state' => 'pending'
    )
  end
  let(:mock_task) { { 'task_id' => 123, 'context' => { 'test' => true } } }

  class TestTaskHandler < TaskerCore::TaskHandler::Base
    def initialize(config: {}, logger: nil, task_config: nil)
      puts "DEBUG: task_config = #{task_config.inspect}" if task_config
      super(config: config, logger: logger, task_config_path: nil, task_config: task_config)
    end
  end

  class TestStepHandler < TaskerCore::StepHandler::Base
    def process(_task, _sequence, _step)
      {
        status: 'completed',
        result: 'step_result'
      }
    end
  end

  describe '#initialize' do
    it 'initializes with task configuration' do
      expect(handler).to be_a(TestTaskHandler)
      expect(handler.task_config).to eq(task_config)
    end

    it 'initializes with custom configuration' do
      custom_handler = TestTaskHandler.new(
        config: { 'timeout' => 60 },
        logger: Logger.new(IO::NULL),
        task_config: task_config
      )
      expect(custom_handler.config).to include('timeout' => 60)
    end
  end

  describe '#handle with real integration' do
    it 'handles real task through Rust integration layer', :database do
      # This test uses a real task created through our factory system
      # It will test the actual Rust integration layer
      task_id = real_task['task_id']

      # Create a task object that responds to task_id like Rails models
      task_object = OpenStruct.new(task_id: task_id)

      result = handler.handle(task_object)

      expect(result).to be_a(Hash)
      expect(result).to have_key('status')
      expect(result['task_id']).to eq(task_id)
    end
  end

  describe '#handler_name' do
    it 'returns handler name from config' do
      expect(handler.handler_name).to eq('test_task')
    end
  end

  describe '#find_step_template' do
    it 'finds step template by name' do
      step_template = handler.find_step_template('test_step')
      expect(step_template).to be_a(Hash)
      expect(step_template['name']).to eq('test_step')
      expect(step_template['handler_class']).to eq('TestStepHandler')
    end

    it 'returns nil for non-existent step' do
      step_template = handler.find_step_template('non_existent_step')
      expect(step_template).to be_nil
    end
  end

  describe '#get_step_handler' do
    it 'creates step handler from template' do
      mock_step = { 'name' => 'test_step' }
      step_handler = handler.get_step_handler(mock_step)

      expect(step_handler).to be_a(TestStepHandler)
      expect(step_handler.config).to include('timeout' => 30)
    end

    it 'returns nil for non-existent step' do
      mock_step = { 'name' => 'non_existent_step' }
      step_handler = handler.get_step_handler(mock_step)

      expect(step_handler).to be_nil
    end

    it 'creates step handler from real workflow step', :database do
      # Use real WorkflowStep object
      step_object = OpenStruct.new(name: 'test_step')
      step_handler = handler.get_step_handler(step_object)

      expect(step_handler).to be_a(TestStepHandler)
      expect(step_handler.config).to include('timeout' => 30)
      expect(step_handler.rust_handler).to be_a(TaskerCore::RubyStepHandler)
    end
  end

  describe '#handle_one_step' do
    it 'delegates to step handler' do
      mock_step = { 'name' => 'test_step' }
      mock_sequence = { 'total_steps' => 1 }

      result = handler.handle_one_step(mock_task, mock_sequence, mock_step)

      expect(result).to be_a(Hash)
      expect(result[:status]).to eq('completed')
      expect(result[:result]).to eq('step_result')
    end

    it 'handles step handler not found' do
      mock_step = { 'name' => 'non_existent_step' }
      mock_sequence = { 'total_steps' => 1 }

      result = handler.handle_one_step(mock_task, mock_sequence, mock_step)

      expect(result).to be_a(Hash)
      expect(result[:error]).to eq('Step handler not found')
    end

    it 'processes real workflow step through integration layer', :database do
      # Create real objects for integration testing
      task_object = OpenStruct.new(
        task_id: real_task['task_id'],
        context: real_task['context']
      )

      step_object = OpenStruct.new(
        id: real_workflow_step['workflow_step_id'],
        name: real_workflow_step['name'],
        task_id: real_workflow_step['task_id']
      )

      sequence = { 'total_steps' => 1, 'current_position' => 0 }

      result = handler.handle_one_step(task_object, sequence, step_object)

      expect(result).to be_a(Hash)
      expect(result[:status]).to eq('completed')
      expect(result[:result]).to eq('step_result')
    end
  end

  describe '#metadata' do
    it 'returns handler metadata' do
      metadata = handler.metadata

      expect(metadata).to be_a(Hash)
      expect(metadata[:handler_name]).to eq('test_task')
      expect(metadata[:handler_class]).to eq('TestTaskHandler')
      expect(metadata[:task_config]).to eq(task_config)
      expect(metadata[:version]).to eq('1.0.0')
      expect(metadata[:namespace]).to eq('default')
      expect(metadata[:capabilities]).to be_an(Array)
      expect(metadata[:capabilities]).to include('handle')
      expect(metadata[:step_templates]).to be_an(Array)
      expect(metadata[:ruby_version]).to eq(RUBY_VERSION)
    end
  end

  describe '#capabilities' do
    it 'returns handler capabilities' do
      capabilities = handler.capabilities

      expect(capabilities).to be_an(Array)
      expect(capabilities).to include('handle')
      expect(capabilities).to include('handle_one_step')
    end
  end

  describe '#config_schema' do
    it 'returns configuration schema from task config' do
      schema = handler.config_schema

      expect(schema).to be_a(Hash)
      expect(schema[:type]).to eq('object')
      expect(schema[:properties]).to be_a(Hash)
      expect(schema[:properties][:timeout]).to be_a(Hash)
      expect(schema[:properties][:retries]).to be_a(Hash)
      expect(schema[:properties][:log_level]).to be_a(Hash)
    end
  end

  describe 'registration' do
    it 'checks if handler is registered' do
      # Mock the registration check
      allow(TaskerCore).to receive(:contains_handler).and_return({ 'exists' => true })

      expect(handler.registered?).to be true
    end
  end

  describe '.list_registered_handlers' do
    it 'lists all registered handlers' do
      # Mock the handler list
      allow(TaskerCore).to receive(:list_handlers).and_return({
                                                                'handlers' => [{
                                                                  'name' => 'test_task',
                                                                  'handler_class' => 'TestTaskHandler',
                                                                  'namespace' => 'default',
                                                                  'version' => '1.0.0'
                                                                }],
                                                                'count' => 1
                                                              })

      handlers = TestTaskHandler.list_registered_handlers
      expect(handlers).to be_a(Hash)
      expect(handlers['count']).to eq(1)
      expect(handlers['handlers']).to be_an(Array)
    end
  end
end
