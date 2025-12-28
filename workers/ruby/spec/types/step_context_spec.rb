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
        expect(context.step_config['retry_on_failure']).to eq(true)
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
