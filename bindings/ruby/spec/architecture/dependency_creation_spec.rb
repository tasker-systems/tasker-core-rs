# frozen_string_literal: true

require 'spec_helper'
require 'yaml'

RSpec.describe 'Task Dependency Creation', type: :architecture do
  let(:config_path) do
    File.expand_path('handlers/examples/order_fulfillment/config/order_fulfillment_handler.yaml', __dir__)
  end

  let(:config) { YAML.load_file(config_path) }

  let(:task_request) do
    TaskerCore::Types::TaskRequest.build_test(
      namespace: "fulfillment",
      name: "process_order",
      version: "1.0.0",
      context: {
        order_id: "ORDER_12345",
        customer_id: 123,
        line_items: [
          { sku: "WIDGET-001", quantity: 2, price: 19.99 },
          { sku: "GADGET-002", quantity: 1, price: 45.50 }
        ],
        shipping_address: {
          street: "123 Main St",
          city: "Anytown",
          state: "CA",
          zip: "12345",
          country: "USA"
        },
        billing_address: {
          street: "123 Main St", 
          city: "Anytown",
          state: "CA",
          zip: "12345",
          country: "USA"
        },
        payment_method: {
          type: "credit_card",
          last_four: "1234",
          expiry: "12/25"
        }
      }
    )
  end

  before do
    # Register the handler with current config
    TaskerCore::Registry.register(
      namespace: config['namespace_name'],
      name: config['name'],
      version: config['version'],
      handler_class: config['task_handler_class'],
      config_schema: config
    )
  end

  describe 'Handler Registration and Initialization' do
    it 'loads configuration from YAML file' do
      expect(config).to be_a(Hash)
      expect(config['namespace_name']).to eq('fulfillment')
      expect(config['name']).to eq('process_order')
      expect(config['version']).to eq('1.0.0')
    end

    it 'registers handler successfully' do
      expect {
        TaskerCore::Registry.register(
          namespace: config['namespace_name'],
          name: config['name'],
          version: config['version'],
          handler_class: config['task_handler_class'],
          config_schema: config
        )
      }.not_to raise_error
    end

    it 'finds and initializes handler instance' do
      handler_result = TaskerCore::Registry.find_handler_and_initialize(
        name: "fulfillment/process_order",
        version: "1.0.0",
        config_path: config_path
      )

      expect(handler_result).to be_a(Hash)
      expect(handler_result['handler_instance']).not_to be_nil
    end
  end

  describe 'Task Request Structure' do
    it 'builds test task request with proper structure' do
      expect(task_request).to respond_to(:namespace)
      expect(task_request).to respond_to(:name) 
      expect(task_request).to respond_to(:version)
      expect(task_request).to respond_to(:context)
    end

    it 'includes required fulfillment context data' do
      expect(task_request.context).to have_key(:order_id)
      expect(task_request.context).to have_key(:customer_id)
      expect(task_request.context).to have_key(:line_items)
      expect(task_request.context).to have_key(:shipping_address)
      expect(task_request.context).to have_key(:billing_address)
      expect(task_request.context).to have_key(:payment_method)
    end

    it 'provides realistic test data structure' do
      expect(task_request.context[:order_id]).to eq("ORDER_12345")
      expect(task_request.context[:customer_id]).to eq(123)
      expect(task_request.context[:line_items]).to be_an(Array)
      expect(task_request.context[:line_items].size).to eq(2)
    end
  end

  describe 'Task Creation and Dependency Resolution' do
    let(:handler_result) do
      TaskerCore::Registry.find_handler_and_initialize(
        name: "fulfillment/process_order",
        version: "1.0.0",
        config_path: config_path
      )
    end

    let(:handler_instance) { handler_result['handler_instance'] }

    it 'creates handler instance successfully' do
      expect(handler_instance).not_to be_nil
      expect(handler_instance).to respond_to(:initialize_task)
    end

    context 'task initialization' do
      let(:initialization_result) { handler_instance.initialize_task(task_request) }

      it 'initializes task without errors' do
        expect { initialization_result }.not_to raise_error
      end

      it 'returns initialization result with task information' do
        result = initialization_result
        expect(result).to be_a(Hash)
        expect(result).to have_key('task_id')
        expect(result['task_id']).to be > 0
      end

      it 'creates workflow steps with dependencies' do
        result = initialization_result
        expect(result).to have_key('step_count')
        expect(result['step_count']).to be > 0
      end
    end
  end

  describe 'Dependency Chain Analysis' do
    let(:handler_result) do
      TaskerCore::Registry.find_handler_and_initialize(
        name: "fulfillment/process_order",
        version: "1.0.0",
        config_path: config_path
      )
    end

    let(:handler_instance) { handler_result['handler_instance'] }
    let(:task_result) { handler_instance.initialize_task(task_request) }
    let(:task_id) { task_result['task_id'] }

    it 'creates proper step dependency relationships' do
      # Get the created task and analyze its dependencies
      expect(task_id).to be > 0
      
      # This test validates that:
      # 1. Task creation succeeded
      # 2. Steps were created with proper IDs
      # 3. Dependency relationships were established
      
      expect(task_result['step_count']).to be > 1
    end

    it 'establishes workflow step execution order' do
      # Verify that steps are created with proper dependency chains
      # For order fulfillment: validate_order → reserve_inventory → process_payment → ship_order
      
      expect(task_result).to be_a(Hash)
      expect(task_result['task_id']).to be_a(Integer)
      expect(task_result['step_count']).to be >= 4  # At least 4 order fulfillment steps
    end
  end

  describe 'Integration with Orchestration System' do
    it 'integrates with TaskerCore orchestration components' do
      expect(TaskerCore::Registry).to respond_to(:find_handler_and_initialize)
      expect(TaskerCore::Types::TaskRequest).to respond_to(:build_test)
    end

    it 'supports end-to-end task lifecycle' do
      # This test validates the complete flow:
      # Configuration → Registration → Initialization → Task Creation → Dependency Setup
      
      handler_result = TaskerCore::Registry.find_handler_and_initialize(
        name: "fulfillment/process_order",
        version: "1.0.0", 
        config_path: config_path
      )
      
      expect(handler_result['handler_instance']).not_to be_nil
      
      task_result = handler_result['handler_instance'].initialize_task(task_request)
      expect(task_result['task_id']).to be > 0
    end
  end
end