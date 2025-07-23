# frozen_string_literal: true

require 'spec_helper'

RSpec.describe 'handle_one_step method', type: :unit do
  let(:config_path) { File.expand_path('../../examples/order_fulfillment/config/order_fulfillment_handler.yaml', __dir__) }
  let(:config) { YAML.load_file(config_path) }
  
  let(:sample_order_context) do
    {
      'customer_info' => { 'id' => 12345, 'email' => 'test@example.com', 'tier' => 'standard' },
      'order_items' => [{ 'product_id' => 101, 'quantity' => 2, 'price' => 29.99 }],
      'payment_info' => { 'method' => 'credit_card', 'token' => 'tok_test_12345', 'amount' => 59.98 },
      'shipping_info' => { 'address' => '123 Main St', 'method' => 'standard' }
    }
  end
  
  let(:task_request) do
    TaskerCore::Types::TaskRequest.build_test(
      namespace: "fulfillment",
      name: "process_order",
      version: "1.0.0",
      context: sample_order_context,
      initiator: "handle_one_step_spec",
      source_system: "rspec",
      reason: "testing handle_one_step method",
      tags: ["test", "handle_one_step", "unit"]
    )
  end

  let(:handler_instance) do
    TaskerCore::Registry.register(
      namespace: config['namespace_name'],
      name: config['name'],
      version: config['version'],
      handler_class: config['task_handler_class'],
      config_schema: config
    )
    
    handler_result = TaskerCore::Registry.find_handler_and_initialize(
      name: "fulfillment/process_order",
      version: "1.0.0",
      config_path: config_path
    )
    
    handler_result['handler_instance']
  end

  let(:task_id) do
    init_result = handler_instance.initialize_task(task_request)
    expect(init_result.success?).to be(true)
    init_result.task_id
  end

  describe '#handle_one_step' do
    it 'has the correct method signature and validation' do
      expect(handler_instance).to respond_to(:handle_one_step)
      expect(handler_instance.method(:handle_one_step).arity).to eq(1)
    end

    context 'when called with non-existent step ID' do
      it 'returns error result with proper structure' do
        result = handler_instance.handle_one_step(99999)
        
        expect(result).to be_a(TaskerCore::TaskHandler::StepHandleResult)
        expect(result.step_id).to eq(99999)
        expect(result.task_id).to be_a(Integer)
        expect(result.success?).to be(false)
        expect(result.error_message).to include("not found")
        expect(result.execution_time_ms).to be >= 0
      end

      it 'provides comprehensive debugging information' do
        result = handler_instance.handle_one_step(99999)
        
        # Verify all StepHandleResult fields are present and properly typed
        expect(result.step_id).to be_a(Integer)
        expect(result.task_id).to be_a(Integer)
        expect(result.step_name).to be_a(String)
        expect(result.status).to be_a(String)
        expect(result.execution_time_ms).to be_a(Integer)
        expect(result.error_message).to be_a(String)
        expect(result.retry_count).to be_a(Integer)
        expect(result.handler_class).to be_a(String)
        expect(result.dependencies_met).to be_in([true, false])
        expect(result.missing_dependencies).to be_an(Array)
        expect(result.dependency_results).to be_a(Hash)
        expect(result.step_state_before).to be_a(String)
        expect(result.step_state_after).to be_a(String)
        expect(result.task_context).to be_a(Hash)
      end
    end

    context 'when called with invalid parameters' do
      it 'validates step_id parameter type' do
        expect {
          handler_instance.handle_one_step("invalid")
        }.to raise_error(TaskerCore::ValidationError, /must be an integer/)
      end

      it 'validates step_id is not nil' do
        expect {
          handler_instance.handle_one_step(nil)
        }.to raise_error(TaskerCore::ValidationError, /is required/)
      end

      it 'handles missing Rust handler gracefully' do
        handler_without_rust = Class.new(TaskerCore::TaskHandler::Base) do
          def initialize
            # Initialize without Rust handler
            @config = {}
            @logger = TaskerCore::Logging::Logger.instance
            @task_config_path = nil
            @task_config = {}
            @rust_handler = nil
          end
        end.new

        expect {
          handler_without_rust.handle_one_step(123)
        }.to raise_error(NotImplementedError, /requires Rust handler/)
      end
    end

    describe 'StepHandleResult convenience methods' do
      let(:result) { handler_instance.handle_one_step(99999) }

      it 'provides success status methods' do
        expect(result.success?).to be_in([true, false])
        expect(result.failed?).to be_in([true, false])
        expect(result.dependencies_not_met?).to be_in([true, false])
        expect(result.retry_eligible?).to be_in([true, false])
      end

      it 'provides hash conversion' do
        hash = result.to_h
        
        expect(hash).to be_a(Hash)
        expect(hash).to have_key('step_id')
        expect(hash).to have_key('task_id')
        expect(hash).to have_key('status')
        expect(hash).to have_key('success')
        expect(hash).to have_key('execution_time_ms')
        expect(hash).to have_key('dependencies_met')
        expect(hash).to have_key('missing_dependencies')
        expect(hash).to have_key('dependency_results')
        expect(hash).to have_key('step_state_before')
        expect(hash).to have_key('step_state_after')
        expect(hash).to have_key('task_context')
      end
    end

    describe 'method capabilities integration' do
      it 'includes handle_one_step in capabilities' do
        capabilities = handler_instance.capabilities
        expect(capabilities).to include('handle_one_step')
      end

      it 'supports handle_one_step capability check' do
        expect(handler_instance.supports_capability?('handle_one_step')).to be(true)
      end
    end

    describe 'FFI integration' do
      it 'successfully delegates to Rust BaseTaskHandler' do
        # This test verifies the FFI boundary is working
        result = handler_instance.handle_one_step(99999)
        
        # If we get a proper StepHandleResult back, FFI is working
        expect(result).to be_a(TaskerCore::TaskHandler::StepHandleResult)
        expect(result.execution_time_ms).to be > 0 # Proves Rust executed
      end

      it 'maintains data integrity across FFI boundary' do
        result = handler_instance.handle_one_step(99999)
        
        # Verify no data corruption in FFI transfer
        expect(result.step_id).to eq(99999) # Input preserved
        expect(result.task_id).to be_a(Integer) # Valid task ID returned
        expect(result.error_message).to include("99999") # Error message contains step ID
      end
    end
  end
end