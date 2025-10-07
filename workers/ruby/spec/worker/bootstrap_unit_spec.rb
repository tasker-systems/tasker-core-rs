# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::Worker::Bootstrap do
  let(:bootstrap) { described_class.instance }

  before do
    # Reset singleton state
    bootstrap.instance_variable_set(:@status, :initialized)
    bootstrap.instance_variable_set(:@rust_handle, nil)
    bootstrap.instance_variable_set(:@shutdown_handlers, [])
    bootstrap.instance_variable_set(:@step_subscriber, nil)

    # Mock FFI module
    allow(TaskerCore::FFI).to receive_messages(bootstrap_worker: {
                                                 'status' => 'started',
                                                 'worker_id' => 'test-worker-123',
                                                 'handle_id' => 'test-handle-456'
                                               }, worker_status: {
                                                 'running' => true,
                                                 'worker_core_status' => 'active'
                                               })
    allow(TaskerCore::FFI).to receive(:stop_worker)
    allow(TaskerCore::FFI).to receive(:transition_to_graceful_shutdown)

    # Mock Ruby components
    allow(TaskerCore::Worker::EventBridge).to receive(:instance).and_return(
      double('EventBridge', active?: true, stop!: true, subscribe_to_step_execution: true)
    )
    allow(TaskerCore::Registry::HandlerRegistry).to receive(:instance).and_return(
      double('HandlerRegistry', handlers: { 'test' => 'handler' }, registered_handlers: [])
    )
    allow(TaskerCore::Worker::StepExecutionSubscriber).to receive(:new).and_return(
      double('StepSubscriber', active?: true, stop!: true, call: true)
    )
  end

  describe '#initialize' do
    it 'initializes with correct default state' do
      # Since it's a singleton, we check the instance variables directly
      expect(bootstrap.instance_variable_get(:@status)).to eq(:initialized)
      expect(bootstrap.instance_variable_get(:@rust_handle)).to be_nil
      expect(bootstrap.instance_variable_get(:@shutdown_handlers)).to eq([])
    end
  end

  describe '#start!' do
    it 'initializes all components in correct order' do
      expect(bootstrap).to receive(:initialize_ruby_components!).ordered
      expect(bootstrap).to receive(:bootstrap_rust_foundation!).ordered
      expect(bootstrap).to receive(:start_event_processing!).ordered
      expect(bootstrap).to receive(:register_shutdown_handlers!).ordered

      result = bootstrap.start!

      expect(result).to eq(bootstrap)
      expect(bootstrap.instance_variable_get(:@status)).to eq(:running)
    end

    it 'merges provided config with defaults' do
      config = { worker_id: 'custom-worker', enable_web_api: true }

      bootstrap.start!(config)

      stored_config = bootstrap.instance_variable_get(:@config)
      expect(stored_config[:worker_id]).to eq('custom-worker')
      expect(stored_config[:enable_web_api]).to be(true)
      expect(stored_config[:event_driven_enabled]).to be(true) # default
    end
  end

  describe '#running?' do
    it 'returns false when not started' do
      expect(bootstrap.running?).to be false
    end

    it 'returns true when started and Rust worker running' do
      bootstrap.instance_variable_set(:@status, :running)
      bootstrap.instance_variable_set(:@rust_handle, { 'handle_id' => 'test' })

      expect(bootstrap.running?).to be true
    end

    it 'returns false if status is running but Rust worker not running' do
      bootstrap.instance_variable_set(:@status, :running)
      bootstrap.instance_variable_set(:@rust_handle, { 'handle_id' => 'test' })
      allow(TaskerCore::FFI).to receive(:worker_status).and_return({ 'running' => false })

      expect(bootstrap.running?).to be false
    end
  end

  describe '#rust_handle_running?' do
    it 'returns false when no handle stored' do
      expect(bootstrap.rust_handle_running?).to be false
    end

    it 'returns false when handle exists but no handle_id' do
      bootstrap.instance_variable_set(:@rust_handle, { 'other_key' => 'value' })

      expect(bootstrap.rust_handle_running?).to be false
    end

    it 'returns true when handle exists with valid ID and Rust worker running' do
      bootstrap.instance_variable_set(:@rust_handle, { 'handle_id' => 'test-123' })

      expect(bootstrap.rust_handle_running?).to be true
    end

    it 'handles both string and symbol keys' do
      bootstrap.instance_variable_set(:@rust_handle, { handle_id: 'test-123' })

      expect(bootstrap.rust_handle_running?).to be true
    end
  end

  describe '#status' do
    before do
      bootstrap.instance_variable_set(:@status, :running)
      bootstrap.instance_variable_set(:@rust_handle, {
                                        'handle_id' => 'test-handle',
                                        'worker_id' => 'test-worker'
                                      })
    end

    it 'returns comprehensive status information' do
      status = bootstrap.status

      expect(status[:rust]).to include('running' => true, 'worker_core_status' => 'active')
      expect(status[:ruby]).to include(
        status: :running,
        handle_stored: true,
        handle_id: 'test-handle',
        worker_id: 'test-worker',
        event_bridge_active: true,
        handler_registry_size: 1
      )
    end

    it 'handles FFI errors gracefully' do
      allow(TaskerCore::FFI).to receive(:worker_status).and_raise(StandardError.new('FFI error'))

      status = bootstrap.status

      expect(status[:error]).to eq('FFI error')
      expect(status[:status]).to eq(:running)
    end
  end

  describe '#health_check' do
    context 'when not running' do
      it 'returns unhealthy status' do
        health = bootstrap.health_check

        expect(health[:healthy]).to be false
        expect(health[:status]).to eq(:initialized)
        expect(health[:error]).to eq('not_running')
      end
    end

    context 'when running' do
      before do
        bootstrap.instance_variable_set(:@status, :running)
        bootstrap.instance_variable_set(:@rust_handle, { 'handle_id' => 'test' })
      end

      it 'returns healthy status when all components are working' do
        health = bootstrap.health_check

        expect(health[:healthy]).to be true
        expect(health[:status]).to eq(:running)
        expect(health[:rust][:running]).to be true
        expect(health[:ruby][:event_bridge_active]).to be true
      end

      it 'returns unhealthy when Rust worker not running' do
        # When Rust worker is not running, the running? method returns false,
        # which causes health_check to return early with not_running error
        allow(TaskerCore::FFI).to receive(:worker_status).and_return({ 'running' => false })

        health = bootstrap.health_check

        expect(health[:healthy]).to be false
        expect(health[:error]).to eq('not_running')
        expect(health[:status]).to eq(:running)
      end

      it 'handles health check errors' do
        allow(bootstrap).to receive(:running?).and_return(true)
        allow(TaskerCore::FFI).to receive(:worker_status).and_raise(StandardError.new('Health error'))

        health = bootstrap.health_check

        expect(health[:healthy]).to be false
        expect(health[:error]).to eq('Health error')
      end
    end
  end

  describe '#shutdown!' do
    before do
      bootstrap.instance_variable_set(:@status, :running)
      bootstrap.instance_variable_set(:@rust_handle, { 'handle_id' => 'test' })

      # Mock components
      @event_bridge = double('EventBridge', stop!: true)
      @step_subscriber = double('StepSubscriber', stop!: true)
      bootstrap.instance_variable_set(:@step_subscriber, @step_subscriber)
      allow(TaskerCore::Worker::EventBridge).to receive(:instance).and_return(@event_bridge)
    end

    it 'executes shutdown sequence in correct order' do
      expect(TaskerCore::FFI).to receive(:transition_to_graceful_shutdown).ordered
      expect(@step_subscriber).to receive(:stop!).ordered
      expect(@event_bridge).to receive(:stop!).ordered
      expect(TaskerCore::FFI).to receive(:stop_worker).ordered

      bootstrap.shutdown!

      expect(bootstrap.instance_variable_get(:@status)).to eq(:stopped)
      expect(bootstrap.instance_variable_get(:@rust_handle)).to be_nil
    end

    it 'runs custom shutdown handlers' do
      handler_called = false
      bootstrap.on_shutdown { handler_called = true }

      bootstrap.shutdown!

      expect(handler_called).to be true
    end

    it 'handles FFI errors during shutdown' do
      allow(TaskerCore::FFI).to receive(:transition_to_graceful_shutdown).and_raise(StandardError.new('FFI error'))

      expect { bootstrap.shutdown! }.not_to raise_error
      expect(bootstrap.instance_variable_get(:@status)).to eq(:stopped)
      expect(bootstrap.instance_variable_get(:@rust_handle)).to be_nil
    end

    it 'does nothing if already stopped' do
      bootstrap.instance_variable_set(:@status, :stopped)

      expect(TaskerCore::FFI).not_to receive(:transition_to_graceful_shutdown)

      bootstrap.shutdown!
    end
  end

  describe '#with_rust_handle' do
    it 'bootstraps if not running and executes block' do
      expect(bootstrap).to receive(:bootstrap_rust_foundation!)

      result = nil
      bootstrap.with_rust_handle { result = 'executed' }

      expect(result).to eq('executed')
    end

    it 'does not bootstrap if already running' do
      bootstrap.instance_variable_set(:@rust_handle, { 'handle_id' => 'test' })
      expect(bootstrap).not_to receive(:bootstrap_rust_foundation!)

      result = nil
      bootstrap.with_rust_handle { result = 'executed' }

      expect(result).to eq('executed')
    end
  end

  describe '#on_shutdown' do
    it 'registers shutdown handler' do
      handler_called = false

      bootstrap.on_shutdown { handler_called = true }

      expect(bootstrap.instance_variable_get(:@shutdown_handlers).size).to eq(1)
    end
  end

  describe 'private methods' do
    describe '#default_config' do
      it 'returns sensible defaults' do
        config = bootstrap.send(:default_config)

        expect(config[:worker_id]).to match(/^ruby-worker-/)
        expect(config[:enable_web_api]).to be(false)
        expect(config[:event_driven_enabled]).to be(true)
        expect(config[:deployment_mode]).to eq('Hybrid')
        expect(config[:namespaces]).to be_an(Array)
      end
    end

    describe '#bootstrap_rust_foundation!' do
      it 'stores handle and verifies running status' do
        bootstrap.send(:bootstrap_rust_foundation!)

        rust_handle = bootstrap.instance_variable_get(:@rust_handle)
        expect(rust_handle).to include('handle_id' => 'test-handle-456')
      end

      it 'handles already running worker' do
        allow(TaskerCore::FFI).to receive(:bootstrap_worker).and_return({
                                                                          'status' => 'already_running',
                                                                          'worker_id' => 'existing-worker',
                                                                          'handle_id' => 'existing-handle'
                                                                        })

        expect { bootstrap.send(:bootstrap_rust_foundation!) }.not_to raise_error

        rust_handle = bootstrap.instance_variable_get(:@rust_handle)
        expect(rust_handle['handle_id']).to eq('existing-handle')
      end

      it 'raises error if worker fails to start' do
        allow(TaskerCore::FFI).to receive(:worker_status).and_return({ 'running' => false })

        expect { bootstrap.send(:bootstrap_rust_foundation!) }.to raise_error(/failed to start/)
      end
    end
  end
end
