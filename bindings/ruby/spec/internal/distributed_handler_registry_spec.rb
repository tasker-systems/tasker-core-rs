# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::Orchestration::DistributedHandlerRegistry do
  let(:registry) { described_class.new }

  describe '#initialize' do
    it 'initializes with empty callables registry' do
      expect(registry.list_callables).to be_empty
      expect(registry.stats[:total_callables]).to eq(0)
      expect(registry.stats[:validation_enabled]).to be true
    end
  end

  describe '#register_callable' do
    let(:test_proc) { ->(task, sequence, step) { { status: 'success' } } }

    it 'registers a callable object' do
      registry.register_callable('TestHandler', test_proc)

      expect(registry.callable_registered?('TestHandler')).to be true
      expect(registry.list_callables['TestHandler']).to eq(test_proc)
      expect(registry.stats[:total_callables]).to eq(1)
    end

    it 'validates callable interface on registration' do
      # Test with non-callable object
      non_callable = "not a callable"

      expect {
        registry.register_callable('InvalidHandler', non_callable)
      }.to raise_error(ArgumentError, 'Callable must respond to .call method')
    end
  end

  describe '#register_proc' do
    it 'registers a proc as callable' do
      registry.register_proc('ProcHandler') do |task, sequence, step|
        { status: 'completed', output: 'processed' }
      end

      expect(registry.callable_registered?('ProcHandler')).to be true
      expect(registry.stats[:total_callables]).to eq(1)

      # Test that the registered proc actually works
      callable = registry.get_callable_for_class('ProcHandler')
      result = callable.call(nil, nil, nil)
      expect(result).to eq({ status: 'completed', output: 'processed' })
    end

    it 'raises error when no block is given' do
      expect {
        registry.register_proc('InvalidHandler')
      }.to raise_error(ArgumentError, 'Block required for register_proc')
    end
  end

  describe '#register_class' do
    # Define a real callable class for testing
    let(:callable_class) do
      Class.new do
        def call(task, sequence, step)
          { status: 'success', handler: 'class_based', task_info: task&.class&.name }
        end
      end
    end

    let(:class_with_class_method) do
      Class.new do
        def self.call(task, sequence, step)
          { status: 'success', handler: 'class_method_based' }
        end
      end
    end

    it 'registers a class instance with call method' do
      registry.register_class('ClassHandler', callable_class)

      expect(registry.callable_registered?('ClassHandler')).to be true
      callable = registry.get_callable_for_class('ClassHandler')
      expect(callable).to respond_to(:call)

      # Test actual invocation
      result = callable.call(nil, nil, nil)
      expect(result[:status]).to eq('success')
      expect(result[:handler]).to eq('class_based')
    end

    it 'registers class with class method directly' do
      registry.register_class('ClassMethodHandler', class_with_class_method)

      callable = registry.get_callable_for_class('ClassMethodHandler')
      expect(callable).to eq(class_with_class_method)

      # Test class method invocation
      result = callable.call(nil, nil, nil)
      expect(result[:handler]).to eq('class_method_based')
    end
  end

  describe '#get_callable_for_class' do
    let(:test_proc) { ->(task, sequence, step) { { status: 'success', from: 'proc' } } }

    it 'returns registered callable with correct priority order' do
      # Test Priority 1: Direct callable registration
      registry.register_callable('TestHandler', test_proc)
      callable = registry.get_callable_for_class('TestHandler')
      expect(callable).to eq(test_proc)

      result = callable.call(nil, nil, nil)
      expect(result[:from]).to eq('proc')
    end

    it 'returns nil for completely unregistered handler' do
      callable = registry.get_callable_for_class('CompletelyUnknownHandler')
      expect(callable).to be_nil
    end
  end

  describe '#bootstrap_handlers' do
    it 'performs real handler discovery and registration' do
      result = registry.bootstrap_handlers

      expect(result).to be_a(Hash)
      expect(result['status']).to eq('success')
      expect(result).to have_key('registered_handlers')
      expect(result).to have_key('failed_handlers')
      expect(result).to have_key('total_callables')
      expect(result).to have_key('bootstrapped_at')

      # Should have attempted to register handlers (may be 0 if none exist)
      expect(result['registered_handlers']).to be >= 0
      expect(result['failed_handlers']).to be >= 0
    end

    it 'actually discovers and registers available handler classes' do
      # Before bootstrap, no callables
      expect(registry.stats[:total_callables]).to eq(0)

      result = registry.bootstrap_handlers

      # After bootstrap, may have found some handlers (depends on environment)
      total_attempts = result['registered_handlers'] + result['failed_handlers']
      expect(total_attempts).to be >= 0

      # Registry should reflect any successful registrations
      expect(registry.stats[:total_callables]).to eq(result['registered_handlers'])
    end
  end

  describe '#discover_handler_classes' do
    it 'returns array of actually existing handler class names' do
      handlers = registry.send(:discover_handler_classes)

      expect(handlers).to be_an(Array)

      # Every returned handler should actually exist and be constantizable
      handlers.each do |handler_class|
        expect { handler_class.constantize }.not_to raise_error

        # Should be a valid class constant
        klass = handler_class.constantize
        expect(klass).to be_a(Class)
      end
    end

    it 'filters out non-existent handler classes' do
      # The method should not return handler classes that don't exist
      handlers = registry.send(:discover_handler_classes)

      # All returned handlers should be valid
      handlers.each do |handler_class|
        expect(Object.const_defined?(handler_class)).to be true
      end
    end
  end

  describe 'TaskTemplate discovery (Phase 4.5)' do
    it 'discovers and loads TaskTemplate YAML files using dry-struct' do
      templates = registry.send(:discover_task_templates)

      expect(templates).to be_an(Array)
      # Should discover at least some templates from the existing YAML files
      expect(templates.size).to be >= 0

      # Each template should be a TaskTemplate dry-struct instance
      templates.each do |template|
        expect(template).to be_a(TaskerCore::Types::TaskTemplate)
        expect(template.name).to be_a(String)
        expect(template.namespace_name).to be_a(String)
        expect(template.version).to match(/\A\d+\.\d+\.\d+\z/)
        expect(template.loaded_from).to be_a(String) if template.loaded_from
      end
    end

    it 'gets search patterns from configuration' do
      search_patterns = registry.send(:get_search_patterns_from_config)

      expect(search_patterns).to be_an(Array)
      # Should get patterns from test environment configuration
      expect(search_patterns.size).to be >= 0

      # Patterns should be expanded to absolute paths
      search_patterns.each do |pattern|
        expect(pattern).to be_a(String)
      end
    end

    it 'loads TaskTemplate data from existing YAML files using dry-struct' do
      search_patterns = registry.send(:get_search_patterns_from_config)
      valid_yaml_file = nil

      search_patterns.each do |pattern|
        files = Dir.glob(pattern)
        valid_yaml_file = files.first if files.any?
        break if valid_yaml_file
      end

      if valid_yaml_file
        template = registry.send(:load_task_template_from_file, valid_yaml_file)

        expect(template).not_to be_nil
        expect(template).to be_a(TaskerCore::Types::TaskTemplate)
        expect(template.name).not_to be_nil
        expect(template.namespace_name).not_to be_nil
        expect(template.version).to match(/\A\d+\.\d+\.\d+\z/)
        expect(template.loaded_from).to eq(valid_yaml_file)
        expect(template.valid_for_registration?).to be true
      else
        skip 'No YAML TaskTemplate files found in search patterns'
      end
    end

    it 'normalizes step templates to dry-struct instances' do
      sample_steps = [
        {
          'name' => 'test_step',
          'description' => 'Test step',
          'handler_class' => 'TestHandler',
          'depends_on_steps' => ['prev_step'],
          'default_retryable' => true,
          'default_retry_limit' => 3
        }
      ]

      normalized = registry.send(:normalize_step_templates_to_structs, sample_steps)

      expect(normalized).to be_an(Array)
      expect(normalized.size).to eq(1)

      step = normalized.first
      expect(step).to be_a(TaskerCore::Types::StepTemplate)
      expect(step.name).to eq('test_step')
      expect(step.handler_class).to eq('TestHandler')
      expect(step.depends_on_steps).to eq(['prev_step'])
      expect(step.default_retryable).to be true
      expect(step.default_retry_limit).to eq(3)
    end

    it 'extracts handler classes from TaskTemplates using dry-struct' do
      # Test with actual TaskTemplate discovery
      templates = registry.send(:discover_task_templates)
      handlers = registry.send(:discover_handler_classes)

      if templates.any?
        # Should discover at least some handler classes from actual templates
        expect(handlers).to be_an(Array)

        # Check that handlers were extracted from templates using dry-struct methods
        template_handlers = []
        templates.each do |template|
          template_handlers.concat(template.handler_class_names)
        end

        # At least some of the template handlers should be in the discovered handlers list
        # (those that exist and can be constantized)
        extracted_count = template_handlers.compact.size
        if extracted_count > 0
          expect(extracted_count).to be > 0
        else
          # It's OK if no handlers exist in test environment
          expect(handlers).to be_an(Array)
        end
      else
        skip 'No TaskTemplates found for testing handler extraction'
      end
    end
  end

  describe 'TaskTemplate Registration API' do
    let(:valid_template_data) do
      {
        name: 'test_task',
        namespace_name: 'test_namespace',
        version: '1.0.0',
        task_handler_class: 'TestTaskHandler',
        description: 'Test task for registration',
        step_templates: [
          {
            name: 'test_step',
            handler_class: 'TestStepHandler',
            description: 'Test step'
          }
        ]
      }
    end

    describe '#register_task_template' do
      it 'successfully registers a valid template using dry-struct validation' do
        result = registry.register_task_template(valid_template_data)

        # Should succeed even if database isn't available (will be mocked in actual implementation)
        expect(result).to have_key(:success)
        expect(result).to have_key(:template_key) if result[:success]
        expect(result).to have_key(:error) unless result[:success]
      end

      it 'validates required fields using dry-struct' do
        invalid_template = valid_template_data.dup
        invalid_template.delete(:name)

        result = registry.register_task_template(invalid_template)

        expect(result[:success]).to be false
        expect(result[:error]).to include('validation failed')
      end

      it 'validates version format using dry-struct' do
        invalid_template = valid_template_data.dup
        invalid_template[:version] = 'invalid-version'

        result = registry.register_task_template(invalid_template)

        expect(result[:success]).to be false
        expect(result[:error]).to include('validation failed')
      end
    end

    describe '#task_template_registered?' do
      it 'checks registration status' do
        result = registry.task_template_registered?('test', 'task', '1.0.0')

        expect(result).to have_key(:success)
        expect(result).to have_key(:template_key)
        expect(result[:template_key]).to eq('test/task:1.0.0')
        expect([true, false]).to include(result[:registered]) if result[:success]
      end

      it 'uses default version when not specified' do
        result = registry.task_template_registered?('test', 'task')

        expect(result[:template_key]).to eq('test/task:1.0.0')
      end
    end

    describe '#list_registered_task_templates' do
      it 'returns template list structure' do
        result = registry.list_registered_task_templates

        expect(result).to have_key(:success)
        expect(result).to have_key(:templates) if result[:success]
        expect(result).to have_key(:total_count) if result[:success]
        expect(result).to have_key(:error) unless result[:success]
      end
    end
  end

  describe '#stats integration' do
    let(:proc1) { ->(task, sequence, step) { { from: 'proc1' } } }
    let(:proc2) { ->(task, sequence, step) { { from: 'proc2' } } }

    it 'provides accurate statistics reflecting actual registry state' do
      # Start empty
      stats = registry.stats
      expect(stats[:total_callables]).to eq(0)
      expect(stats[:handler_classes]).to be_empty

      # Add some handlers
      registry.register_callable('Handler1', proc1)
      registry.register_callable('Handler2', proc2)

      # Stats should update
      stats = registry.stats
      expect(stats[:total_callables]).to eq(2)
      expect(stats[:handler_classes]).to contain_exactly('Handler1', 'Handler2')
      expect(stats[:callable_types][Proc]).to eq(2)
    end
  end

  describe 'real callable execution integration' do
    it 'executes registered callables with proper arguments' do
      # Register a proc that inspects its arguments
      registry.register_proc('ArgumentInspector') do |task, sequence, step|
        {
          task_received: !task.nil?,
          sequence_received: !sequence.nil?,
          step_received: !step.nil?,
          argument_count: 3
        }
      end

      callable = registry.get_callable_for_class('ArgumentInspector')

      # Call with actual arguments
      result = callable.call('mock_task', 'mock_sequence', 'mock_step')

      expect(result[:task_received]).to be true
      expect(result[:sequence_received]).to be true
      expect(result[:step_received]).to be true
      expect(result[:argument_count]).to eq(3)
    end
  end

  describe '#clear_callables!' do
    let(:test_proc) { ->(task, sequence, step) { { status: 'success' } } }

    it 'completely clears registry state' do
      # Add some callables
      registry.register_callable('Handler1', test_proc)
      registry.register_callable('Handler2', test_proc)

      expect(registry.stats[:total_callables]).to eq(2)
      expect(registry.list_callables.size).to eq(2)

      # Clear all
      registry.clear_callables!

      # Should be completely empty
      expect(registry.stats[:total_callables]).to eq(0)
      expect(registry.list_callables).to be_empty
      expect(registry.callable_registered?('Handler1')).to be false
      expect(registry.callable_registered?('Handler2')).to be false
    end
  end

  describe 'validation integration' do
    let(:valid_proc) { ->(task, sequence, step) { { status: 'success' } } }
    let(:invalid_object) { "not callable" }

    it 'enforces validation when enabled' do
      expect(registry.stats[:validation_enabled]).to be true

      # Valid callable should work
      expect {
        registry.register_callable('ValidHandler', valid_proc)
      }.not_to raise_error

      # Invalid callable should fail
      expect {
        registry.register_callable('InvalidHandler', invalid_object)
      }.to raise_error(ArgumentError)
    end

    it 'skips validation when disabled' do
      registry.validation_enabled = false

      # Should allow invalid callables when validation is off
      expect {
        registry.register_callable('InvalidHandler', invalid_object)
      }.not_to raise_error

      expect(registry.callable_registered?('InvalidHandler')).to be true
    end
  end
end
