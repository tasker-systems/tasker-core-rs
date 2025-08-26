# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::Internal::OrchestrationManager do
  let(:manager) { described_class.instance }

  before do
    manager.reset!
  end

  describe '#bootstrap_orchestration_system' do
    context 'when configuration determines embedded mode' do
      it 'successfully bootstraps embedded orchestration system' do
        # Use real config loading from test config file
        expect { manager.bootstrap_orchestration_system }.not_to raise_error

        expect(manager.initialized?).to be true
        expect(manager.info[:status]).to eq('initialized')
        expect(manager.info[:architecture]).to eq('pgmq')

        # In test environment, should default to embedded mode
        expect(manager.info[:mode]).to eq('embedded')
      end

      it 'does not create distributed handler registry in embedded mode' do
        manager.bootstrap_orchestration_system

        # Embedded mode shouldn't have distributed handler registry
        expect(manager.info[:handler_registry][:available]).to be false
        expect(manager.distributed_handler_registry).to be_nil
      end
    end
  end

  describe '#bootstrap_core_queues' do
    it 'handles database unavailability gracefully' do
      # Temporarily break database connection to test error handling
      original_client = manager.instance_variable_get(:@pgmq_client)
      manager.instance_variable_set(:@pgmq_client, nil)

      # Force pgmq_available? to return false
      allow(manager).to receive(:pgmq_available?).and_return(false)

      expect { manager.send(:bootstrap_core_queues) }.to raise_error(TaskerCore::Errors::OrchestrationError)

      # Restore original client
      manager.instance_variable_set(:@pgmq_client, original_client)
    end
  end

  describe '#info' do
    it 'includes all expected orchestration information' do
      info = manager.info

      expect(info).to have_key(:initialized)
      expect(info).to have_key(:status)
      expect(info).to have_key(:architecture)
      expect(info).to have_key(:mode)
      expect(info).to have_key(:pgmq_available)
      expect(info).to have_key(:embedded_orchestrator_available)
      expect(info).to have_key(:handler_registry)

      expect(info[:architecture]).to eq('pgmq')
    end

    it 'reports accurate pgmq availability based on database connection' do
      info = manager.info

      # Should reflect actual database connectivity in test environment
      expect(info[:pgmq_available]).to be true
    end
  end

  describe '#reset!' do
    it 'resets all orchestration state' do
      # First initialize the manager with actual bootstrapping
      manager.bootstrap_orchestration_system
      expect(manager.initialized?).to be true

      # Then reset and verify state is cleared
      manager.reset!

      expect(manager.initialized?).to be false
      expect(manager.info[:status]).to eq('reset')
      expect(manager.distributed_handler_registry).to be_nil
    end
  end

  describe 'real integration with configuration system' do
    it 'uses actual configuration from test environment' do
      # Test that manager picks up real configuration
      mode = manager.orchestration_mode

      # Should be 'embedded' for test environment per config
      expect(mode).to eq('embedded')

      # Test bootstrap with real config
      manager.bootstrap_orchestration_system
      expect(manager.initialized?).to be true
    end
  end
end
