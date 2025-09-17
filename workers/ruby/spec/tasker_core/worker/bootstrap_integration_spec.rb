# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::Worker::Bootstrap do
  let(:bootstrap) { subject.instance }

  describe 'Bootstrap and lifecycle' do
    it 'successfully bootstraps the worker system' do
      # Start the worker
      bootstrap.start!(
        worker_id: 'test-worker-001',
        event_driven_enabled: true,
        deployment_mode: 'Hybrid'
      )

      # Verify it's running
      expect(bootstrap.running?).to be true

      # Check status
      status = bootstrap.status
      expect(status[:running]).to be true
      expect(status[:environment]).to eq('test')
      expect(status[:event_bridge_active]).to be true

      # Graceful shutdown
      bootstrap.shutdown!
      expect(bootstrap.running?).to be false
    end
  end

  describe 'Event flow integration' do
    before do
      bootstrap.start!
    end

    after do
      bootstrap.shutdown!
    end

    it 'processes step execution events from Rust to Ruby' do
      events_received = []

      # Subscribe to events
      TaskerCore::Worker::EventBridge.instance.subscribe_to_step_execution do |event|
        events_received << event
      end

      # Create a test task that will trigger events
      # (This assumes the Rust worker will process and emit events)
      create_test_task

      # Wait for events
      sleep 1

      expect(events_received).not_to be_empty
      event = events_received.first

      expect(event[:event_id]).to be_present
      expect(event[:task_sequence_step]).to be_a(TaskSequenceStepWrapper)
    end

    it 'sends completion events from Ruby to Rust' do
      completion_sent = false

      # Monitor completion events
      TaskerCore::Worker::EventBridge.instance.subscribe('step.completion.sent') do |_event|
        completion_sent = true
      end

      # Send a completion
      completion_data = {
        event_id: SecureRandom.uuid,
        task_uuid: SecureRandom.uuid,
        step_uuid: SecureRandom.uuid,
        success: true,
        result: { value: 42 },
        metadata: { handler: 'test' }
      }

      TaskerCore::Worker::EventBridge.instance.publish_step_completion(completion_data)

      expect(completion_sent).to be true
    end
  end

  describe 'Error handling' do
    it 'handles bootstrap failures gracefully' do
      # Attempt to start twice
      bootstrap.start!

      expect {
        TaskerCore::FFI.bootstrap_worker({ worker_id: 'duplicate' })
      }.to raise_error(/already running/)

      bootstrap.shutdown!
    end

    it 'handles event processing errors' do
      bootstrap.start!

      # Send invalid completion
      expect {
        TaskerCore::Worker::EventBridge.instance.publish_step_completion({})
      }.to raise_error(ArgumentError, /Missing required fields/)

      bootstrap.shutdown!
    end
  end
end
