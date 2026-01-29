# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::Types::StepContext do
  # Mock the step_data (TaskSequenceStepWrapper) and its components
  subject(:context) { described_class.new(step_data) }

  let(:task_wrapper) do
    double('TaskWrapper',
           task_uuid: 'task-uuid-123',
           context: { 'customer_id' => 42, 'order_amount' => 100.0 })
  end

  let(:workflow_step_wrapper) do
    double('WorkflowStepWrapper',
           workflow_step_uuid: 'step-uuid-456',
           name: 'validate_order',
           inputs: { 'field1' => 'value1', 'field2' => 'value2' },
           attempts: 2,
           max_attempts: 5)
  end

  let(:dependency_results_wrapper) do
    mock = double('DependencyResultsWrapper')
    allow(mock).to receive(:get_results).with('previous_step').and_return({ result: 'previous_data' })
    allow(mock).to receive(:get_results).with('nonexistent').and_return(nil)
    mock
  end

  let(:handler_config) do
    {
      'timeout' => 30,
      'retry_on_failure' => true
    }
  end

  let(:handler_definition) do
    double('HandlerDefinition',
           callable: 'ValidateOrderHandler',
           initialization: handler_config)
  end

  let(:step_definition_wrapper) do
    double('StepDefinitionWrapper',
           handler: handler_definition)
  end

  let(:step_data) do
    mock = double('TaskSequenceStepWrapper',
                  task: task_wrapper,
                  workflow_step: workflow_step_wrapper,
                  dependency_results: dependency_results_wrapper,
                  step_definition: step_definition_wrapper)
    # Make the mock respond correctly to is_a? check
    allow(mock).to receive(:is_a?).with(TaskerCore::Models::TaskSequenceStepWrapper).and_return(true)
    mock
  end

  describe '#initialize' do
    it 'creates a context from step data' do
      expect(context).to be_a(described_class)
    end

    it 'extracts handler_name from step_definition' do
      expect(context.handler_name).to eq('ValidateOrderHandler')
    end
  end

  describe 'cross-language standard fields' do
    describe '#task_uuid' do
      it 'returns the task UUID' do
        expect(context.task_uuid).to eq('task-uuid-123')
      end
    end

    describe '#step_uuid' do
      it 'returns the workflow step UUID' do
        expect(context.step_uuid).to eq('step-uuid-456')
      end
    end

    describe '#input_data' do
      it 'returns the step inputs' do
        expect(context.input_data).to eq({ 'field1' => 'value1', 'field2' => 'value2' })
      end
    end

    describe '#step_inputs' do
      it 'is an alias for input_data' do
        expect(context.step_inputs).to eq(context.input_data)
      end
    end

    describe '#step_config' do
      it 'returns the handler initialization config' do
        expect(context.step_config['timeout']).to eq(30)
        expect(context.step_config['retry_on_failure']).to be(true)
      end

      context 'when handler initialization is nil' do
        let(:handler_definition) do
          double('HandlerDefinition',
                 callable: 'ValidateOrderHandler',
                 initialization: nil)
        end

        it 'returns an empty hash with indifferent access' do
          expect(context.step_config).to be_a(Hash)
          expect(context.step_config).to be_empty
        end
      end
    end

    describe '#retry_count' do
      it 'returns the current attempt count' do
        expect(context.retry_count).to eq(2)
      end

      context 'when attempts is nil' do
        let(:workflow_step_wrapper) do
          double('WorkflowStepWrapper',
                 workflow_step_uuid: 'step-uuid-456',
                 name: 'validate_order',
                 inputs: {},
                 attempts: nil,
                 max_attempts: 5)
        end

        it 'returns 0' do
          expect(context.retry_count).to eq(0)
        end
      end
    end

    describe '#max_retries' do
      it 'returns the maximum attempt count' do
        expect(context.max_retries).to eq(5)
      end

      context 'when max_attempts is nil' do
        let(:workflow_step_wrapper) do
          double('WorkflowStepWrapper',
                 workflow_step_uuid: 'step-uuid-456',
                 name: 'validate_order',
                 inputs: {},
                 attempts: 0,
                 max_attempts: nil)
        end

        it 'returns 3 as default' do
          expect(context.max_retries).to eq(3)
        end
      end
    end
  end

  describe 'Ruby-specific accessors' do
    describe '#task' do
      it 'returns the task wrapper' do
        expect(context.task).to eq(task_wrapper)
      end
    end

    describe '#workflow_step' do
      it 'returns the workflow step wrapper' do
        expect(context.workflow_step).to eq(workflow_step_wrapper)
      end
    end

    describe '#step_definition' do
      it 'returns the step definition wrapper' do
        expect(context.step_definition).to eq(step_definition_wrapper)
      end
    end

    describe '#dependency_results' do
      it 'returns the dependency results wrapper' do
        expect(context.dependency_results).to eq(dependency_results_wrapper)
      end
    end
  end

  describe 'convenience methods' do
    describe '#get_task_field' do
      it 'retrieves a field from task context by string key' do
        expect(context.get_task_field('customer_id')).to eq(42)
        expect(context.get_task_field('order_amount')).to eq(100.0)
      end

      it 'retrieves a field from task context by symbol key' do
        expect(context.get_task_field(:customer_id)).to eq(42)
      end

      it 'returns nil for nonexistent keys' do
        expect(context.get_task_field('nonexistent')).to be_nil
      end
    end

    describe '#get_dependency_result' do
      it 'retrieves results from a previous step' do
        expect(context.get_dependency_result('previous_step')).to eq({ result: 'previous_data' })
      end

      it 'returns nil for nonexistent steps' do
        expect(context.get_dependency_result('nonexistent')).to be_nil
      end
    end

    describe '#step_name' do
      it 'returns the workflow step name' do
        expect(context.step_name).to eq('validate_order')
      end
    end
  end

  describe 'namespace_name' do
    context 'when task responds to namespace_name' do
      let(:task_wrapper) do
        double('TaskWrapper',
               task_uuid: 'task-uuid-123',
               context: {},
               namespace_name: 'payments')
      end

      it 'returns the namespace name' do
        expect(context.namespace_name).to eq('payments')
      end
    end

    context 'when task does not respond to namespace_name' do
      it 'returns nil' do
        expect(context.namespace_name).to be_nil
      end
    end
  end

  describe 'checkpoint accessors' do
    context 'when checkpoint data exists' do
      let(:workflow_step_wrapper) do
        double('WorkflowStepWrapper',
               workflow_step_uuid: 'step-uuid-456',
               name: 'validate_order',
               inputs: { 'field1' => 'value1' },
               attempts: 0,
               max_attempts: 3,
               checkpoint: {
                 'cursor' => 500,
                 'items_processed' => 500,
                 'accumulated_results' => { 'total' => 1000 }
               })
      end

      it 'returns the raw checkpoint' do
        expect(context.checkpoint).to eq({
                                           'cursor' => 500,
                                           'items_processed' => 500,
                                           'accumulated_results' => { 'total' => 1000 }
                                         })
      end

      it 'returns the checkpoint cursor' do
        expect(context.checkpoint_cursor).to eq(500)
      end

      it 'returns items processed' do
        expect(context.checkpoint_items_processed).to eq(500)
      end

      it 'returns accumulated results' do
        expect(context.accumulated_results).to eq({ 'total' => 1000 })
      end

      it 'reports checkpoint exists' do
        expect(context.has_checkpoint?).to be true
      end
    end

    context 'when no checkpoint data exists' do
      let(:workflow_step_wrapper) do
        double('WorkflowStepWrapper',
               workflow_step_uuid: 'step-uuid-456',
               name: 'validate_order',
               inputs: {},
               attempts: 0,
               max_attempts: 3,
               checkpoint: nil)
      end

      it 'returns nil checkpoint' do
        expect(context.checkpoint).to be_nil
      end

      it 'returns nil cursor' do
        expect(context.checkpoint_cursor).to be_nil
      end

      it 'returns 0 items processed' do
        expect(context.checkpoint_items_processed).to eq(0)
      end

      it 'returns nil accumulated results' do
        expect(context.accumulated_results).to be_nil
      end

      it 'reports no checkpoint' do
        expect(context.has_checkpoint?).to be false
      end
    end
  end

  describe 'retry helpers' do
    describe '#is_retry?' do
      context 'when attempts > 0' do
        it 'returns true' do
          expect(context.is_retry?).to be true
        end
      end

      context 'when attempts is 0' do
        let(:workflow_step_wrapper) do
          double('WorkflowStepWrapper',
                 workflow_step_uuid: 'step-uuid-456',
                 name: 'validate_order',
                 inputs: {},
                 attempts: 0,
                 max_attempts: 5)
        end

        it 'returns false' do
          expect(context.is_retry?).to be false
        end
      end
    end

    describe '#is_last_retry?' do
      context 'when attempts >= max_attempts - 1' do
        let(:workflow_step_wrapper) do
          double('WorkflowStepWrapper',
                 workflow_step_uuid: 'step-uuid-456',
                 name: 'validate_order',
                 inputs: {},
                 attempts: 4,
                 max_attempts: 5)
        end

        it 'returns true' do
          expect(context.is_last_retry?).to be true
        end
      end

      context 'when attempts < max_attempts - 1' do
        let(:workflow_step_wrapper) do
          double('WorkflowStepWrapper',
                 workflow_step_uuid: 'step-uuid-456',
                 name: 'validate_order',
                 inputs: {},
                 attempts: 1,
                 max_attempts: 5)
        end

        it 'returns false' do
          expect(context.is_last_retry?).to be false
        end
      end
    end
  end

  describe 'additional accessors' do
    describe '#retryable?' do
      let(:workflow_step_wrapper) do
        double('WorkflowStepWrapper',
               workflow_step_uuid: 'step-uuid-456',
               name: 'validate_order',
               inputs: {},
               attempts: 0,
               max_attempts: 3,
               retryable: true)
      end

      it 'delegates to workflow_step' do
        expect(context.retryable?).to be true
      end
    end

    describe '#context' do
      it 'returns the task context' do
        expect(context.context).to eq({ 'customer_id' => 42, 'order_amount' => 100.0 })
      end
    end
  end

  describe 'debug methods' do
    describe '#to_s' do
      it 'includes task_uuid, step_uuid, and step_name' do
        str = context.to_s
        expect(str).to include('task-uuid-123')
        expect(str).to include('step-uuid-456')
        expect(str).to include('validate_order')
      end
    end

    describe '#inspect' do
      it 'includes all identifying fields' do
        str = context.inspect
        expect(str).to include('task-uuid-123')
        expect(str).to include('step-uuid-456')
        expect(str).to include('validate_order')
        expect(str).to include('ValidateOrderHandler')
      end
    end
  end

  describe '#get_input_or' do
    it 'returns the value when present' do
      expect(context.get_input_or('customer_id', 0)).to eq(42)
    end

    it 'returns the default when value is nil' do
      expect(context.get_input_or('nonexistent', 'default_value')).to eq('default_value')
    end
  end

  describe '#get_config' do
    it 'returns config value by key' do
      expect(context.get_config('timeout')).to eq(30)
    end

    it 'returns nil for missing key' do
      expect(context.get_config('nonexistent')).to be_nil
    end
  end

  describe '#get_dependency_field' do
    let(:dependency_results_wrapper) do
      mock = double('DependencyResultsWrapper')
      allow(mock).to receive(:get_results).with('previous_step').and_return({
                                                                              'data' => { 'items' => %w[a b c] }
                                                                            })
      allow(mock).to receive(:get_results).with('nonexistent').and_return(nil)
      mock
    end

    it 'extracts nested fields from dependency results' do
      expect(context.get_dependency_field('previous_step', 'data', 'items')).to eq(%w[a b c])
    end

    it 'returns nil when dependency does not exist' do
      expect(context.get_dependency_field('nonexistent', 'data')).to be_nil
    end
  end

  describe 'integration with handler pattern' do
    class TestContextHandler < TaskerCore::StepHandler::Base
      def call(context)
        # Access cross-language standard fields
        task_id = context.task_uuid
        step_id = context.step_uuid
        inputs = context.input_data

        # Access Ruby-specific accessors
        amount = context.get_task_field('order_amount')
        previous = context.get_dependency_result('previous_step')

        TaskerCore::Types::StepHandlerCallResult.success(
          result: {
            task_id: task_id,
            step_id: step_id,
            inputs: inputs,
            amount: amount,
            previous: previous
          }
        )
      end
    end

    it 'works seamlessly with handler pattern' do
      handler = TestContextHandler.new
      result = handler.call(context)

      expect(result.success).to be true
      expect(result.result[:task_id]).to eq('task-uuid-123')
      expect(result.result[:step_id]).to eq('step-uuid-456')
      expect(result.result[:inputs]).to eq({ 'field1' => 'value1', 'field2' => 'value2' })
      expect(result.result[:amount]).to eq(100.0)
      expect(result.result[:previous]).to eq({ result: 'previous_data' })
    end
  end
end
