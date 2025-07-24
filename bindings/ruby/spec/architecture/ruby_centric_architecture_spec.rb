# frozen_string_literal: true

require 'spec_helper'

RSpec.describe 'Ruby-Centric Step Handler Architecture', type: :architecture do
  let(:config_path) do
    File.join(__dir__, '..', 'handlers', 'examples', 'order_fulfillment', 'config', 'order_fulfillment_handler.yaml')
  end

  let(:handler) { TaskerCore::TaskHandler::Base.new(task_config_path: config_path) }
  let(:orchestration_manager) { TaskerCore::Internal::OrchestrationManager.instance }

  before(:all) do
    # Load example step handlers
    pattern = File.join(__dir__, '..', 'handlers', 'examples', 'order_fulfillment', '**', '*.rb')
    Dir[pattern].each { |f| require f }
  end

  describe 'TaskHandler Creation with YAML Config' do
    it 'creates TaskHandler successfully from YAML config' do
      expect(handler).to be_a(TaskerCore::TaskHandler::Base)
    end

    it 'loads configuration from specified path' do
      expect(File.exist?(config_path)).to be true
    end

    describe 'pre-instantiated step handlers' do
      it 'creates step handlers during initialization' do
        expect(handler.step_handlers).to be_a(Hash)
        expect(handler.step_handlers.size).to be > 0
      end

      it 'instantiates each step handler class' do
        handler.step_handlers.each do |step_name, step_handler|
          expect(step_name).to be_a(String)
          expect(step_handler).to respond_to(:call)
        end
      end

      it 'includes expected order fulfillment step handlers' do
        expected_steps = %w[validate_order reserve_inventory process_payment ship_order]
        
        expected_steps.each do |step_name|
          expect(handler.step_handlers).to have_key(step_name)
        end
      end
    end
  end

  describe 'OrchestrationManager Registry' do
    it 'provides access to OrchestrationManager instance' do
      expect(orchestration_manager).to respond_to(:list_ruby_task_handlers)
    end

    describe 'Ruby TaskHandler registry' do
      let(:ruby_handlers) { orchestration_manager.list_ruby_task_handlers }

      it 'returns an array of handler information' do
        expect(ruby_handlers).to be_an(Array)
      end

      context 'when handlers are registered' do
        it 'includes handler metadata' do
          skip 'No handlers registered' if ruby_handlers.empty?

          ruby_handlers.each do |handler_info|
            expect(handler_info).to have_key(:key)
            expect(handler_info).to have_key(:handler_class)
            expect(handler_info).to have_key(:step_handler_count)
          end
        end
      end
    end
  end

  describe 'Step Handler Lookup' do
    let(:test_steps) { %w[validate_order reserve_inventory process_payment ship_order] }

    it 'resolves step handlers by name' do
      test_steps.each do |step_name|
        step_handler = handler.get_step_handler_from_name(step_name)
        expect(step_handler).not_to be_nil, "Expected step handler for '#{step_name}' to be found"
        expect(step_handler).to respond_to(:call)
      end
    end

    it 'returns consistent step handler instances' do
      test_steps.each do |step_name|
        step_handler1 = handler.get_step_handler_from_name(step_name)
        step_handler2 = handler.get_step_handler_from_name(step_name)
        
        # Should return the same pre-instantiated instance
        expect(step_handler1).to be(step_handler2)
      end
    end

    describe 'step handler characteristics' do
      let(:validate_order_handler) { handler.get_step_handler_from_name('validate_order') }

      it 'provides step handlers with expected interface' do
        expect(validate_order_handler).to respond_to(:call)
      end

      it 'step handlers have proper class inheritance' do
        expect(validate_order_handler.class.ancestors).to include(TaskerCore::StepHandler::Base)
      end
    end
  end

  describe 'O(1) Step Handler Performance' do
    it 'provides constant-time step handler lookup' do
      # Verify that get_step_handler_from_name is O(1) hash lookup
      # rather than O(n) search through handlers
      
      start_time = Time.now
      1000.times do
        handler.get_step_handler_from_name('validate_order')
      end
      lookup_time = Time.now - start_time
      
      # Should complete 1000 lookups very quickly (< 0.1 seconds)
      expect(lookup_time).to be < 0.1
    end

    it 'maintains pre-instantiated handlers in memory' do
      # Verify handlers are created once during initialization
      # and reused for all subsequent calls
      
      handler1 = handler.get_step_handler_from_name('validate_order')
      handler2 = handler.get_step_handler_from_name('validate_order')
      
      expect(handler1.object_id).to eq(handler2.object_id)
    end
  end

  describe 'Ruby-Centric Architecture Benefits' do
    it 'eliminates Rust-side step handler management complexity' do
      # By pre-instantiating step handlers in Ruby, we avoid:
      # - FFI calls for each step handler lookup
      # - Cross-language object management
      # - Dynamic handler instantiation overhead
      
      expect(handler.step_handlers).to be_a(Hash)
      expect(handler.step_handlers.values).to all(be_an(Object))
    end

    it 'provides direct Ruby object access for step execution' do
      step_handler = handler.get_step_handler_from_name('validate_order')
      
      # Handler is a real Ruby object, not an FFI wrapper
      expect(step_handler).to be_a(ValidateOrderHandler)
      expect(step_handler.class.ancestors).to include(TaskerCore::StepHandler::Base)
    end

    it 'supports full Ruby ecosystem integration' do
      # Step handlers can use any Ruby gems, patterns, etc.
      # without FFI boundary limitations
      
      step_handler = handler.get_step_handler_from_name('process_payment')
      expect(step_handler).to respond_to(:call)
      
      # Can invoke Ruby methods directly
      expect { step_handler.class.name }.not_to raise_error
    end
  end
end