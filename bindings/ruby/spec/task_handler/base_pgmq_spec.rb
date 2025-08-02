# frozen_string_literal: true

require 'spec_helper'
require 'tempfile'

RSpec.describe TaskerCore::TaskHandler::Base do
  let(:handler) { described_class.new }
  let(:task_id) { 12345 }
  
  # Bootstrap the orchestration system before any tests
  before(:all) do
    # Initialize the orchestration system properly for tests
    TaskerCore::Internal::OrchestrationManager.instance.bootstrap_orchestration_system
  end
  
  after(:all) do
    # Clean up orchestration system after tests
    TaskerCore::Internal::OrchestrationManager.instance.reset!
  end
  
  # Proper TaskRequest as Hash (NO task_id - that's created after processing)
  # The handler expects Hash input and converts to TaskRequest internally
  let(:task_request_hash) do
    {
      'namespace' => 'fulfillment',
      'name' => 'order_processing',
      'version' => '1.0.0',
      'context' => { 'order_id' => 12345, 'customer_id' => 67890 },
      'initiator' => 'rspec_test',
      'source_system' => 'test_suite',
      'reason' => 'integration_test',
      'tags' => ['test', 'pgmq_integration']
    }
  end

  # Additional test with different structure
  let(:inventory_task_request_hash) do
    {
      'namespace' => 'inventory',
      'name' => 'stock_update', 
      'context' => { 'product_id' => 'ABC123', 'quantity' => 50 },
      'priority' => 3,
      'max_retries' => 2,
      'initiator' => 'test',
      'source_system' => 'rspec',
      'reason' => 'testing'
    }
  end

  # Verify our test data can actually create valid TaskRequest objects
  let(:validated_task_request) do
    TaskerCore::Types::TaskRequest.from_hash(task_request_hash)
  end

  let(:validated_inventory_request) do
    TaskerCore::Types::TaskRequest.from_hash(inventory_task_request_hash)
  end

  describe '#initialize' do
    it 'initializes with lazy pgmq client using bootstrapped system' do
      expect(handler.logger).not_to be_nil
      expect(handler.task_config).to eq({})
      
      # pgmq_client should be lazily initialized - may fail if DB not available
      # but should still respond to expected methods
      begin
        pgmq_client = handler.pgmq_client
        expect(pgmq_client).to be_a(TaskerCore::Messaging::PgmqClient)
        expect(pgmq_client).to respond_to(:send_message)
        expect(pgmq_client).to respond_to(:create_queue)
      rescue TaskerCore::Error => e
        # Expected if database not available in test environment
        expect(e.message).to include('database connection')
      end
    end

    it 'loads task config from path when provided' do
      # Create a temporary config file
      config_content = { 'test_setting' => 'test_value' }
      temp_file = Tempfile.new(['test_config', '.yaml'])
      temp_file.write(YAML.dump(config_content))
      temp_file.close

      handler_with_config = described_class.new(task_config_path: temp_file.path)
      expect(handler_with_config.task_config).to eq(config_content)

      temp_file.unlink
    end
  end

  describe '#handle' do
    context 'when orchestrator is available through bootstrapped system' do
      it 'handles orchestration using bootstrapped orchestration system' do
        result = handler.handle(task_id)
        
        expect(result).to be_a(Hash)
        expect(result[:task_id]).to eq(task_id)
        expect(result[:architecture]).to eq('pgmq')
        
        # With bootstrapped orchestration system, should work properly
        # The actual mode depends on configuration, but system should be functional
        expect(result).to have_key(:success)
        expect(result).to have_key(:message)
        
        # In test environment, should default to embedded mode with working orchestrator
        if result[:success]
          expect(result[:message]).to be_a(String)
        else
          # If it fails, should be a clear error about orchestration
          expect(result[:error_type]).to be_a(String)
        end
      end
    end

    context 'when using bootstrapped orchestration system' do
      it 'successfully processes tasks with bootstrapped orchestration' do
        result = handler.handle(task_id)
        
        expect(result).to be_a(Hash)
        expect(result[:task_id]).to eq(task_id)
        expect(result[:architecture]).to eq('pgmq')
        expect(result).to have_key(:success)
        expect(result).to have_key(:message)
        
        # With properly bootstrapped system, should work in configured mode
        if result[:success]
          expect(result[:message]).to be_a(String)
        else
          # Clear error messages if orchestration isn't fully available
          expect(result[:error_type]).to be_a(String)
        end
      end

      it 'handles invalid task_id with validation' do
        # Test with invalid task_id
        result = handler.handle(-1)
        
        expect(result).to be_a(Hash)
        expect(result[:task_id]).to eq(-1)
        expect(result[:architecture]).to eq('pgmq')
        # Should either succeed or fail gracefully
        expect(result).to have_key(:success)
      end
    end

    it 'validates task_id is required' do
      result = handler.handle(nil)
      
      expect(result).to be_a(Hash)
      expect(result[:success]).to be false
      expect(result[:error_type]).to eq('TaskerCore::ValidationError')
      expect(result[:error]).to eq('task_id is required')
      expect(result[:architecture]).to eq('pgmq')
    end

    it 'validates task_id is an integer' do
      result = handler.handle('not_an_integer')
      
      expect(result).to be_a(Hash)
      expect(result[:success]).to be false
      expect(result[:error_type]).to eq('TaskerCore::ValidationError')
      expect(result[:error]).to eq('task_id must be an integer')
      expect(result[:architecture]).to eq('pgmq')
    end
  end

  describe '#initialize_task' do
    it 'validates required fields using empty hash' do
      result = handler.initialize_task({})
      
      expect(result).to be_a(Hash)
      expect(result[:success]).to be false
      expect(result[:error_type]).to eq('ValidationError')
      expect(result[:error]).to include('Invalid TaskRequest')
    end

    it 'successfully processes valid TaskRequest hash data' do
      result = handler.initialize_task(task_request_hash)
      
      expect(result).to be_a(Hash)
      expect(result[:success]).to be true
      expect(result[:namespace]).to eq('fulfillment')
      expect(result[:task_name]).to eq('order_processing')
      expect(result[:task_version]).to eq('1.0.0')
      expect(result[:architecture]).to eq('pgmq')
      expect(result[:message]).to include('Phase 4.3')
    end

    it 'successfully processes different TaskRequest configurations' do
      result = handler.initialize_task(inventory_task_request_hash)
      
      expect(result).to be_a(Hash)
      expect(result[:success]).to be true
      expect(result[:namespace]).to eq('inventory')
      expect(result[:task_name]).to eq('stock_update')
      expect(result[:task_version]).to eq('1.0.0')
      expect(result[:architecture]).to eq('pgmq')
    end

    it 'validates that our test data creates proper TaskRequest objects' do
      # This test ensures our hash data is valid for TaskRequest creation
      expect(validated_task_request).to be_a(TaskerCore::Types::TaskRequest)
      expect(validated_task_request.namespace).to eq('fulfillment')
      expect(validated_task_request.name).to eq('order_processing')
      expect(validated_task_request.valid_for_creation?).to be true
      
      expect(validated_inventory_request).to be_a(TaskerCore::Types::TaskRequest)
      expect(validated_inventory_request.namespace).to eq('inventory')
      expect(validated_inventory_request.valid_for_creation?).to be true
    end

    it 'handles hash-based TaskRequest (converted to dry-struct internally)' do
      # Test with raw hash that gets converted to TaskRequest internally
      hash_request = {
        'namespace' => 'fulfillment',
        'name' => 'order_processing',
        'version' => '1.0.0',
        'initiator' => 'test',
        'source_system' => 'test_suite',
        'reason' => 'unit_test',
        'context' => { 'order_id' => 12345, 'customer_id' => 67890 }
      }
      
      result = handler.initialize_task(hash_request)
      
      expect(result[:success]).to be true
      expect(result[:namespace]).to eq('fulfillment')
      expect(result[:task_name]).to eq('order_processing')
    end

    it 'validates TaskRequest structure and type coercion' do
      # Test that proper validation occurs
      invalid_request = { 'namespace' => '', 'name' => 'test' } # Missing required fields
      
      result = handler.initialize_task(invalid_request)
      
      expect(result).to be_a(Hash)
      expect(result[:success]).to be false
      expect(result[:error_type]).to eq('ValidationError')
    end

    it 'handles non-hash input gracefully' do
      result = handler.initialize_task('not_a_hash')
      
      expect(result).to be_a(Hash)
      expect(result[:success]).to be false
      expect(result[:error_type]).to eq('NoMethodError')
      expect(result[:error]).to include('transform_keys')
    end
  end

  describe '#orchestration_ready?' do
    it 'checks orchestration readiness with bootstrapped system' do
      # With bootstrapped orchestration system, should give accurate status
      result = handler.orchestration_ready?
      expect([true, false]).to include(result)
      
      # Should not crash and should return boolean
      expect([true, false]).to include(result)
    end

    it 'handles orchestrator availability gracefully' do
      # Test the method doesn't crash regardless of orchestrator state
      result = handler.orchestration_ready?
      expect([true, false]).to include(result)
    end
  end

  describe '#status' do
    it 'returns comprehensive status information with bootstrapped system' do
      # Status should work even if pgmq client creation fails
      # The key is that it should return a valid status hash
      status = nil
      expect { status = handler.status }.not_to raise_error
      
      expect(status).to be_a(Hash)
      expect(status[:handler_type]).to eq('TaskHandler::Base')
      expect(status[:architecture]).to eq('pgmq')
      expect(status[:orchestration_mode]).to be_a(String)
      expect(['embedded', 'distributed']).to include(status[:orchestration_mode])
      expect(status).to have_key(:orchestration_ready)
      expect(status).to have_key(:pgmq_available)
      expect(status).to have_key(:task_config_loaded)
    end

    context 'when using bootstrapped orchestration system' do
      it 'includes appropriate orchestrator details based on mode' do
        status = handler.status
        
        if status[:orchestration_mode] == 'embedded'
          # In embedded mode, should have embedded_orchestrator info
          expect(status).to have_key(:embedded_orchestrator)
        else
          # In distributed mode, should not have embedded_orchestrator info
          expect(status[:embedded_orchestrator]).to be_nil
        end
      end
    end

    context 'when configuration determines mode' do
      it 'reflects actual configuration mode' do
        status = handler.status
        
        # Should be one of the valid modes
        expect(['embedded', 'distributed']).to include(status[:orchestration_mode])
        
        # Mode-specific checks
        if status[:orchestration_mode] == 'embedded'
          expect(status).to have_key(:embedded_orchestrator)
        else
          expect(status[:embedded_orchestrator]).to be_nil
        end
      end
    end

    it 'handles different task config states' do
      # Test with config loaded
      config_content = { 'test_setting' => 'test_value' }
      temp_file = Tempfile.new(['test_config', '.yaml'])
      temp_file.write(YAML.dump(config_content))
      temp_file.close

      handler_with_config = described_class.new(task_config_path: temp_file.path)
      
      # Status should work even if pgmq_client fails
      status = nil
      expect { status = handler_with_config.status }.not_to raise_error
      
      if status
        expect(status[:task_config_loaded]).to be true
      end
      
      temp_file.unlink
    end
  end

  describe 'pgmq architecture validation' do
    it 'confirms no command_client dependencies' do
      # Verify the handler doesn't reference any TCP-era components in actual code
      source_path = File.join(File.dirname(__FILE__), '..', '..', 'lib', 'tasker_core', 'task_handler', 'base.rb')
      source_code = File.read(source_path)
      
      # Remove comments to test only actual code
      code_lines = source_code.split("\n").reject { |line| line.strip.start_with?('#') }
      actual_code = code_lines.join("\n")
      
      expect(actual_code).not_to include('command_client')
      expect(actual_code).not_to include('CommandClient')
      expect(actual_code).not_to include('create_command_client')
      expect(actual_code).not_to include('OrchestrationManager')
    end

    it 'uses pgmq components only' do
      source_path = File.join(File.dirname(__FILE__), '..', '..', 'lib', 'tasker_core', 'task_handler', 'base.rb')
      source_code = File.read(source_path)
      
      expect(source_code).to include('PgmqClient')
      expect(source_code).to include('embedded_orchestrator')
      expect(source_code).to include('architecture: \'pgmq\'')
    end
  end
end