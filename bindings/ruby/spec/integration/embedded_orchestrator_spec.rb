# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::EmbeddedOrchestrator do
  let(:namespaces) { ['fulfillment', 'inventory'] }
  let(:orchestrator) { described_class.new(namespaces) }

  after do
    # Ensure cleanup after each test
    orchestrator.stop if orchestrator.running?
  end

  describe '#initialize' do
    it 'initializes with provided namespaces' do
      expect(orchestrator.namespaces).to eq(namespaces)
      expect(orchestrator.started_at).to be_nil
      expect(orchestrator).not_to be_running
    end

    it 'defaults to standard namespaces if none provided' do
      default_orchestrator = described_class.new
      expect(default_orchestrator.namespaces).to eq(['fulfillment', 'inventory', 'notifications'])
    end
  end

  describe '#start' do
    it 'starts the embedded orchestration system' do
      expect(orchestrator.start).to be true
      expect(orchestrator).to be_running
      expect(orchestrator.started_at).to be_within(1.second).of(Time.now)
    end

    it 'returns false if already running' do
      orchestrator.start
      expect(orchestrator.start).to be false
    end

    it 'raises error on startup failure' do
      # Mock a startup failure by providing invalid namespaces
      allow(TaskerCore).to receive(:start_embedded_orchestration).and_raise('Test failure')
      
      expect {
        orchestrator.start
      }.to raise_error(TaskerCore::OrchestrationError, /Failed to start embedded orchestration/)
    end
  end

  describe '#stop' do
    before do
      orchestrator.start
    end

    it 'stops the running orchestration system' do
      expect(orchestrator.stop).to be true
      expect(orchestrator).not_to be_running
      expect(orchestrator.started_at).to be_nil
    end

    it 'returns false if not running' do
      orchestrator.stop
      expect(orchestrator.stop).to be false
    end
  end

  describe '#running?' do
    it 'returns false when not started' do
      expect(orchestrator).not_to be_running
    end

    it 'returns true when started' do
      orchestrator.start
      expect(orchestrator).to be_running
    end

    it 'returns false after stopped' do
      orchestrator.start
      orchestrator.stop
      expect(orchestrator).not_to be_running
    end
  end

  describe '#status' do
    context 'when not running' do
      it 'returns status with running false' do
        status = orchestrator.status
        expect(status[:running]).to be false
        expect(status[:namespaces]).to eq(namespaces)
        expect(status[:started_at]).to be_nil
        expect(status[:uptime_seconds]).to be_nil
      end
    end

    context 'when running' do
      before do
        orchestrator.start
        sleep 0.1 # Small delay to ensure system is fully started
      end

      it 'returns detailed status information' do
        status = orchestrator.status
        expect(status[:running]).to be true
        expect(status[:namespaces]).to eq(namespaces)
        expect(status[:started_at]).not_to be_nil
        expect(status[:uptime_seconds]).to be >= 0
        expect(status[:database_pool_size]).to be > 0
        expect(status[:database_pool_idle]).to be >= 0
      end
    end
  end

  describe '#enqueue_steps' do
    let(:task_id) { 12345 }

    context 'when not running' do
      it 'raises error' do
        expect {
          orchestrator.enqueue_steps(task_id)
        }.to raise_error(TaskerCore::OrchestrationError, /Orchestration system not running/)
      end
    end

    context 'when running' do
      before do
        orchestrator.start
      end

      it 'enqueues steps for the task' do
        # Mock the FFI call to avoid needing actual task data
        allow(TaskerCore).to receive(:enqueue_task_steps).with(task_id).and_return("Steps enqueued")
        
        result = orchestrator.enqueue_steps(task_id)
        expect(result).to eq("Steps enqueued")
      end

      it 'raises error on enqueueing failure' do
        allow(TaskerCore).to receive(:enqueue_task_steps).and_raise('Enqueueing failed')
        
        expect {
          orchestrator.enqueue_steps(task_id)
        }.to raise_error(TaskerCore::OrchestrationError, /Failed to enqueue steps/)
      end
    end
  end

  describe 'class methods' do
    after do
      TaskerCore.stop_embedded_orchestration!
    end

    describe '.embedded_orchestrator' do
      it 'returns a singleton instance' do
        orchestrator1 = TaskerCore.embedded_orchestrator
        orchestrator2 = TaskerCore.embedded_orchestrator
        expect(orchestrator1).to be(orchestrator2)
      end
    end

    describe '.start_embedded_orchestration!' do
      it 'starts the global orchestrator' do
        result = TaskerCore.start_embedded_orchestration!(['test'])
        expect(result).to be true
        expect(TaskerCore.embedded_orchestration_running?).to be true
      end

      it 'creates new orchestrator with custom namespaces' do
        TaskerCore.start_embedded_orchestration!(['custom'])
        expect(TaskerCore.embedded_orchestrator.namespaces).to eq(['custom'])
      end
    end

    describe '.stop_embedded_orchestration!' do
      it 'stops the global orchestrator' do
        TaskerCore.start_embedded_orchestration!
        result = TaskerCore.stop_embedded_orchestration!
        expect(result).to be true
        expect(TaskerCore.embedded_orchestration_running?).to be false
      end
    end

    describe '.embedded_orchestration_running?' do
      it 'returns false when not running' do
        expect(TaskerCore.embedded_orchestration_running?).to be false
      end

      it 'returns true when running' do
        TaskerCore.start_embedded_orchestration!
        expect(TaskerCore.embedded_orchestration_running?).to be true
      end
    end
  end

  describe 'lifecycle management' do
    it 'handles multiple start/stop cycles' do
      3.times do
        expect(orchestrator.start).to be true
        expect(orchestrator).to be_running
        expect(orchestrator.stop).to be true
        expect(orchestrator).not_to be_running
      end
    end

    it 'provides consistent status across lifecycle' do
      # Not running
      expect(orchestrator.status[:running]).to be false
      
      # Start
      orchestrator.start
      expect(orchestrator.status[:running]).to be true
      
      # Stop
      orchestrator.stop
      expect(orchestrator.status[:running]).to be false
    end
  end
end