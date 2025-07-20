# frozen_string_literal: true

require 'spec_helper'

RSpec.describe 'TaskerCore::Registry find_handler functionality', :integration do
  describe 'handler registration and lookup' do
    let(:handle) { TaskerCore::Internal::OrchestrationManager.instance.orchestration_handle }
    
    let(:handler_data) do
      {
        namespace: 'payments',
        name: 'process_card',
        version: '1.0.0',
        handler_class: 'PaymentCardHandler',
        config_schema: { 'amount' => 'number', 'currency' => 'string' }
      }
    end
    
    let(:task_request) do
      {
        namespace: 'payments',
        name: 'process_card',
        version: '1.0.0'
      }
    end

    before do
      # Ensure clean state for each test
      handle.validate_or_refresh
    end

    it 'successfully registers and finds handlers using namespace/name/version pattern' do
      # Register a handler
      result = TaskerCore::Registry.register(handler_data)
      expect(result).to be_truthy
      
      # Find the handler using the new find_handler functionality
      found_handler = TaskerCore::Registry.find(task_request)
      
      expect(found_handler).not_to be_nil
      expect(found_handler).to be_a(Hash)
      expect(found_handler['found']).to be true
      expect(found_handler['namespace']).to eq('payments')
      expect(found_handler['name']).to eq('process_card')
      expect(found_handler['version']).to eq('1.0.0')
      expect(found_handler['handler_class']).to eq('PaymentCardHandler')
      expect(found_handler['config_schema']).to eq({ 'amount' => 'number', 'currency' => 'string' })
      expect(found_handler['registered_at']).to be_a(String)
    end

    it 'returns nil for non-existent handlers' do
      non_existent_request = {
        namespace: 'nonexistent',
        name: 'missing_handler',
        version: '1.0.0'
      }
      
      found_handler = TaskerCore::Registry.find(non_existent_request)
      expect(found_handler).to be_nil
    end

    it 'handles default namespace and version correctly' do
      default_handler_data = {
        name: 'default_handler',
        handler_class: 'DefaultHandler'
      }
      
      # Register without explicit namespace/version
      TaskerCore::Registry.register(default_handler_data)
      
      # Find with default values
      default_request = {
        name: 'default_handler'
      }
      
      found_handler = TaskerCore::Registry.find(default_request)
      expect(found_handler).not_to be_nil
      expect(found_handler['namespace']).to eq('default')
      expect(found_handler['version']).to eq('0.1.0')  # Default version from FFI wrapper
    end

    it 'maintains handler persistence across multiple lookups' do
      # Register handler
      TaskerCore::Registry.register(handler_data)
      
      # Perform multiple lookups
      3.times do
        found_handler = TaskerCore::Registry.find(task_request)
        expect(found_handler).not_to be_nil
        expect(found_handler['found']).to be true
        expect(found_handler['name']).to eq('process_card')
      end
    end

    it 'validates find_handler FFI integration works end-to-end' do
      # Register through Registry domain
      TaskerCore::Registry.register(handler_data)
      
      # Find through direct handle access to verify FFI layer
      direct_result = handle.find_handler(task_request)
      expect(direct_result).to be_a(Hash)
      expect(direct_result['found']).to be true
      
      # Find through Registry domain to verify domain layer
      domain_result = TaskerCore::Registry.find(task_request)
      expect(domain_result).to eq(direct_result)
    end
  end
end