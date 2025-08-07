# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::Registry::StepHandlerResolver do
  let(:resolver) { described_class.instance }

  before do
    # Clear any existing callables before each test
    resolver.clear_callables!
  end

  describe '#initialize' do
    it 'initializes with empty callables registry' do
      expect(resolver.list_callables).to be_empty
      expect(resolver.stats[:total_callables]).to eq(0)
      expect(resolver.stats[:validation_enabled]).to be true
    end
  end

  describe '#register_callable' do
    let(:test_proc) { ->(_task, _sequence, _step) { { status: 'success' } } }

    it 'registers a callable object' do
      resolver.register_callable('TestHandler', test_proc)

      expect(resolver.callable_registered?('TestHandler')).to be true
      expect(resolver.get_callable_for_class('TestHandler')).to eq(test_proc)
      expect(resolver.stats[:total_callables]).to eq(1)
    end

    it 'validates callable interface when validation is enabled' do
      invalid_callable = Object.new

      expect { resolver.register_callable('InvalidHandler', invalid_callable) }
        .to raise_error(ArgumentError, /must respond to .call method/)
    end

    it 'skips validation when validation is disabled' do
      resolver.validation_enabled = false
      invalid_callable = Object.new

      expect { resolver.register_callable('InvalidHandler', invalid_callable) }
        .not_to raise_error
    end
  end

  describe '#register_proc' do
    it 'registers a proc using block syntax' do
      resolver.register_proc('ProcHandler') do |_task, _sequence, _step|
        { status: 'completed', output: 'processed' }
      end

      expect(resolver.callable_registered?('ProcHandler')).to be true

      callable = resolver.get_callable_for_class('ProcHandler')
      expect(callable).to be_a(Proc)
    end

    it 'raises error when no block is provided' do
      expect { resolver.register_proc('NoBlockHandler') }
        .to raise_error(ArgumentError, /Block required for register_proc/)
    end
  end

  describe '#register_class' do
    context 'with class that has class method .call' do
      let(:callable_class) do
        Class.new do
          def self.call(_task, _sequence, _step)
            { status: 'success' }
          end
        end
      end

      it 'registers the class directly' do
        resolver.register_class('CallableClass', callable_class)

        expect(resolver.callable_registered?('CallableClass')).to be true
        expect(resolver.get_callable_for_class('CallableClass')).to eq(callable_class)
      end
    end

    context 'with class that has instance method .call' do
      let(:instance_callable_class) do
        Class.new do
          def call(_task, _sequence, _step)
            { status: 'success' }
          end
        end
      end

      it 'registers an instance of the class' do
        resolver.register_class('InstanceCallableClass', instance_callable_class)

        expect(resolver.callable_registered?('InstanceCallableClass')).to be true

        callable = resolver.get_callable_for_class('InstanceCallableClass')
        expect(callable).to be_an_instance_of(instance_callable_class)
      end
    end
  end

  describe '#get_callable_for_class' do
    it 'returns registered callable with highest priority' do
      test_proc = ->(_task, _sequence, _step) { { status: 'proc' } }
      resolver.register_callable('TestHandler', test_proc)

      result = resolver.get_callable_for_class('TestHandler')
      expect(result).to eq(test_proc)
    end

    it 'falls back to class .call method when no callable registered' do
      # Define a class in the global namespace for constantization
      stub_const('TestCallableClass', Class.new do
        def self.call(_task, _sequence, _step)
          { status: 'class_call' }
        end
      end)

      result = resolver.get_callable_for_class('TestCallableClass')
      expect(result).to eq(TestCallableClass)
    end

    it 'falls back to instance .call method when class call method not available' do
      # Define a class in the global namespace for constantization
      stub_const('TestInstanceClass', Class.new do
        def call(_task, _sequence, _step)
          { status: 'instance_call' }
        end
      end)

      result = resolver.get_callable_for_class('TestInstanceClass')
      expect(result).to be_an_instance_of(TestInstanceClass)
      expect(result).to respond_to(:call)
    end

    it 'returns nil when handler class does not exist' do
      result = resolver.get_callable_for_class('NonExistentHandler')
      expect(result).to be_nil
    end
  end

  describe '#unregister_callable' do
    it 'removes a registered callable' do
      test_proc = ->(_task, _sequence, _step) { { status: 'success' } }
      resolver.register_callable('TestHandler', test_proc)

      expect(resolver.callable_registered?('TestHandler')).to be true

      removed = resolver.unregister_callable('TestHandler')
      expect(removed).to eq(test_proc)
      expect(resolver.callable_registered?('TestHandler')).to be false
    end

    it 'returns nil when callable not found' do
      result = resolver.unregister_callable('NonExistentHandler')
      expect(result).to be_nil
    end
  end

  describe '#resolve_step_handler' do
    let(:task_template_registry) { TaskerCore::Registry::TaskTemplateRegistry.instance }
    let(:task) { double('Task', task_id: 1) }
    let(:step) { double('Step', named_step_id: 'test_step') }
    let(:task_template) do
      TaskerCore::Types::TaskTemplate.new(
        name: 'test_task',
        namespace_name: 'test_namespace',
        version: '1.0.0',
        task_handler_class: 'TestTaskHandler',
        step_templates: [
          TaskerCore::Types::StepTemplate.new(
            name: 'test_step',
            handler_class: 'TestStepHandler',
            handler_config: { timeout: 30 }
          )
        ]
      )
    end

    before do
      # Mock the TaskTemplateRegistry to return our test template
      allow(task_template_registry).to receive(:get_task_template).with(task).and_return(task_template)

      # Define the handler class for resolution
      stub_const('TestStepHandler', Class.new do
        def initialize(config = {})
          @config = config
        end

        def call(_task, _sequence, _step)
          { status: 'success', config: @config }
        end
      end)
    end

    it 'resolves step handler successfully' do
      resolved = resolver.resolve_step_handler(task, step)

      expect(resolved).to be_a(TaskerCore::Registry::StepHandlerResolver::ResolvedStepHandler)
      expect(resolved.handler_class_name).to eq('TestStepHandler')
      expect(resolved.handler_class).to eq(TestStepHandler)
      expect(resolved.handler_config).to eq({ timeout: 30 })
      expect(resolved.step_template.name).to eq('test_step')
      expect(resolved.task_template).to eq(task_template)
    end

    it 'returns nil when task template not found' do
      allow(task_template_registry).to receive(:get_task_template).with(task).and_return(nil)

      resolved = resolver.resolve_step_handler(task, step)
      expect(resolved).to be_nil
    end

    it 'returns nil when step template not found' do
      step_with_wrong_name = double('Step', named_step_id: 'non_existent_step')

      resolved = resolver.resolve_step_handler(task, step_with_wrong_name)
      expect(resolved).to be_nil
    end

    it 'returns nil when handler class does not exist' do
      task_template_with_missing_handler = TaskerCore::Types::TaskTemplate.new(
        name: 'test_task',
        namespace_name: 'test_namespace',
        version: '1.0.0',
        task_handler_class: 'TestTaskHandler',
        step_templates: [
          TaskerCore::Types::StepTemplate.new(
            name: 'test_step',
            handler_class: 'NonExistentHandler'
          )
        ]
      )

      allow(task_template_registry).to receive(:get_task_template).with(task).and_return(task_template_with_missing_handler)

      resolved = resolver.resolve_step_handler(task, step)
      expect(resolved).to be_nil
    end
  end

  describe '#create_handler_instance' do
    let(:resolved_handler) do
      TaskerCore::Registry::StepHandlerResolver::ResolvedStepHandler.new(
        handler_class_name: 'TestHandler',
        handler_class: test_handler_class,
        handler_config: { timeout: 30 },
        step_template: double('StepTemplate'),
        task_template: double('TaskTemplate')
      )
    end

    context 'with handler class that accepts config in constructor' do
      let(:test_handler_class) do
        Class.new do
          def initialize(config = {})
            @config = config
          end

          def call(_task, _sequence, _step)
            { status: 'success', config: @config }
          end
        end
      end

      it 'creates instance with configuration' do
        instance = resolver.create_handler_instance(resolved_handler)

        expect(instance).to be_an_instance_of(test_handler_class)
        expect(instance.instance_variable_get(:@config)).to eq({ timeout: 30 })
      end
    end

    context 'with handler class that does not accept config' do
      let(:test_handler_class) do
        Class.new do
          def initialize
            # No config parameter
          end

          def call(_task, _sequence, _step)
            { status: 'success' }
          end
        end
      end

      it 'creates instance without configuration' do
        instance = resolver.create_handler_instance(resolved_handler)

        expect(instance).to be_an_instance_of(test_handler_class)
      end
    end

    context 'with registered callable taking precedence' do
      let(:test_handler_class) { Class.new }
      let(:test_callable) { ->(_task, _sequence, _step) { { status: 'callable' } } }

      it 'returns registered callable instead of creating class instance' do
        resolver.register_callable('TestHandler', test_callable)

        result = resolver.create_handler_instance(resolved_handler)
        expect(result).to eq(test_callable)
      end
    end
  end

  describe '#bootstrap_handlers' do
    before do
      # Mock TaskTemplateRegistry to return test templates
      task_template_registry = TaskerCore::Registry::TaskTemplateRegistry.instance
      test_templates = [
        TaskerCore::Types::TaskTemplate.new(
          name: 'test_task',
          namespace_name: 'test_namespace',
          version: '1.0.0',
          task_handler_class: 'BootstrapTestTaskHandler',
          step_templates: [
            TaskerCore::Types::StepTemplate.new(
              name: 'step1',
              handler_class: 'BootstrapTestStepHandler'
            )
          ]
        )
      ]

      allow(task_template_registry).to receive(:load_task_templates_from_database).and_return(test_templates)

      # Define the handler classes
      stub_const('BootstrapTestTaskHandler', Class.new do
        def call(_task, _sequence, _step)
          { status: 'success' }
        end
      end)

      stub_const('BootstrapTestStepHandler', Class.new do
        def call(_task, _sequence, _step)
          { status: 'success' }
        end
      end)
    end

    it 'discovers and registers handler classes from task templates' do
      result = resolver.bootstrap_handlers

      expect(result['status']).to eq('success')
      expect(result['registered_handlers']).to eq(2)
      expect(result['failed_handlers']).to eq(0)
      expect(resolver.stats[:total_callables]).to eq(2)
    end
  end

  describe '#discover_handler_classes' do
    it 'discovers unique handler classes from task templates' do
      # This method is private, so we test it through bootstrap_handlers
      # The functionality is covered in the bootstrap_handlers test above
    end
  end

  describe 'integration with TaskTemplateRegistry' do
    it 'uses TaskTemplateRegistry for task template retrieval' do
      task_template_registry = TaskerCore::Registry::TaskTemplateRegistry.instance
      task = double('Task', task_id: 1)

      expect(task_template_registry).to receive(:get_task_template).with(task)

      resolver.resolve_step_handler(task, double('Step', named_step_id: 'test'))
    end
  end
end
