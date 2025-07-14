# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::Events::Publisher do
  let(:publisher) { described_class.instance }

  describe 'singleton behavior' do
    it 'returns the same instance' do
      publisher1 = described_class.instance
      publisher2 = described_class.instance

      expect(publisher1).to be(publisher2)
    end
  end

  describe 'event registration' do
    it 'registers events from constants during initialization' do
      expect(publisher.registered_event_count).to be > 0
    end

    it 'registers task events' do
      expect(publisher.event_registered?('task.completed')).to be true
      expect(publisher.event_registered?('task.failed')).to be true
    end

    it 'registers step events' do
      expect(publisher.event_registered?('step.completed')).to be true
      expect(publisher.event_registered?('step.failed')).to be true
    end

    it 'registers workflow events' do
      expect(publisher.event_registered?('workflow.task_started')).to be true
    end

    it 'provides list of registered events' do
      events = publisher.registered_events

      expect(events).to be_an(Array)
      expect(events).to include('task.completed')
      expect(events).to include('step.completed')
      expect(events).to include('workflow.task_started')
    end
  end

  describe 'basic event publishing' do
    let(:received_events) { [] }

    before do
      received_events.clear
      publisher.subscribe('task.completed') { |event| received_events << event }
    end

    it 'publishes and delivers events to subscribers' do
      test_payload = { task_id: '123', execution_duration: 5.2 }
      publisher.publish('task.completed', test_payload)

      expect(received_events.size).to eq(1)
      event = received_events.first
      expect(event[:task_id]).to eq('123')
      expect(event[:execution_duration]).to eq(5.2)
    end

    it 'automatically adds timestamp to events' do
      publisher.publish('task.completed', { task_id: '456' })

      expect(received_events.size).to eq(1)
      event = received_events.first
      expect(event[:timestamp]).to be_a(Time)
    end

    it 'preserves existing timestamp' do
      custom_timestamp = Time.parse('2025-01-01T00:00:00Z')
      publisher.publish('task.completed', {
                          task_id: '789',
                          timestamp: custom_timestamp
                        })

      expect(received_events.size).to eq(1)
      event = received_events.first
      expect(event[:timestamp]).to eq(custom_timestamp)
    end
  end

  describe 'task transition requests' do
    let(:received_events) { [] }

    before do
      received_events.clear
      publisher.subscribe('workflow.transition_requested') { |event| received_events << event }
    end

    it 'sends task transition requests with proper payload structure' do
      result = publisher.publish_task_transition(
        '123', # task_id
        'in_progress',           # from_state
        'complete',              # to_state
        { execution_duration: 3.5 }
      )

      expect(result).to be true
      expect(received_events.size).to eq(1)

      event = received_events.first
      expect(event[:entity_type]).to eq('task')
      expect(event[:entity_id]).to eq('123')
      expect(event[:from_state]).to eq('in_progress')
      expect(event[:to_state]).to eq('complete')
      expect(event[:execution_duration]).to eq(3.5)
      expect(event[:requested_at]).to be_a(String)
    end

    it 'skips same-state transitions' do
      result = publisher.publish_task_transition(
        '456',
        'complete',    # from_state
        'complete',    # to_state (same as from)
        {}
      )

      expect(result).to be false
      expect(received_events).to be_empty
    end
  end

  describe 'step transition requests' do
    let(:received_events) { [] }

    before do
      received_events.clear
      publisher.subscribe('workflow.transition_requested') { |event| received_events << event }
    end

    it 'sends step transition requests with proper payload structure' do
      result = publisher.publish_step_transition(
        '456',                   # step_id
        '123',                   # task_id
        'in_progress',           # from_state
        'error',                 # to_state
        { error_message: 'Test error' }
      )

      expect(result).to be true
      expect(received_events.size).to eq(1)

      event = received_events.first
      expect(event[:entity_type]).to eq('step')
      expect(event[:entity_id]).to eq('456')
      expect(event[:task_id]).to eq('123')
      expect(event[:from_state]).to eq('in_progress')
      expect(event[:to_state]).to eq('error')
      expect(event[:error_message]).to eq('Test error')
      expect(event[:requested_at]).to be_a(String)
    end
  end

  describe 'multiple subscribers' do
    let(:subscriber1_events) { [] }
    let(:subscriber2_events) { [] }

    before do
      subscriber1_events.clear
      subscriber2_events.clear

      publisher.subscribe('workflow.task_started') { |event| subscriber1_events << event }
      publisher.subscribe('workflow.task_started') { |event| subscriber2_events << event }
    end

    it 'delivers events to all subscribers' do
      publisher.publish('workflow.task_started', { task_id: 'multi_test' })

      expect(subscriber1_events.size).to eq(1)
      expect(subscriber2_events.size).to eq(1)

      expect(subscriber1_events.first[:task_id]).to eq('multi_test')
      expect(subscriber2_events.first[:task_id]).to eq('multi_test')
    end
  end

  describe 'error handling' do
    it 'raises error for invalid event name' do
      expect { publisher.publish(nil, {}) }.to raise_error(StandardError)
    end
  end

  describe 'invalid transitions' do
    it 'sends requests for any transition (Rust will validate)' do
      received_events = []
      publisher.subscribe('workflow.transition_requested') { |event| received_events << event }

      result = publisher.publish_task_transition(
        '999',
        'invalid_state',   # from_state
        'another_invalid', # to_state
        {}
      )

      expect(result).to be true # Request is sent regardless
      expect(received_events.size).to eq(1)
      event = received_events.first
      expect(event[:entity_id]).to eq('999')
      expect(event[:from_state]).to eq('invalid_state')
      expect(event[:to_state]).to eq('another_invalid')
    end
  end
end
