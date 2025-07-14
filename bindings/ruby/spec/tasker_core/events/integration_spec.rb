# frozen_string_literal: true

require 'spec_helper'

RSpec.describe 'TaskerCore Events Integration' do
  let(:publisher) { TaskerCore::Events::Publisher.instance }
  let(:event_bridge) { TaskerCore::Events::RustEventBridge.new(nil) }

  describe 'FFI Integration with standalone publisher' do
    let(:received_events) { [] }

    before do
      received_events.clear
      publisher.subscribe('task.completed') { |event| received_events << event }
    end

    it 'publishes Rust events through handle_rust_event_publication to standalone publisher' do
      event_payload = {
        task_id: '123',
        task_name: 'test_task',
        execution_duration: 2.5
      }

      # Simulate the FFI call from Rust
      event_bridge.handle_rust_event_publication('task.completed', event_payload.to_json)

      expect(received_events.size).to eq(1)
      event = received_events.first
      expect(event[:task_id]).to eq('123')
      expect(event[:task_name]).to eq('test_task')
      expect(event[:execution_duration]).to eq(2.5)
      expect(event[:timestamp]).to be_a(Time)
    end

    it 'handles JSON parsing errors gracefully' do
      expect do
        event_bridge.handle_rust_event_publication('task.completed', 'invalid_json')
      end.to raise_error(JSON::ParserError)
    end

    it 'uses only TaskerCore publisher, not Rails publisher' do
      # Ensure we're not accidentally trying to use Rails
      expect(defined?(Tasker::Events::Publisher)).to be_falsy

      event_payload = { task_id: '456' }

      expect do
        event_bridge.handle_rust_event_publication('task.completed', event_payload.to_json)
      end.not_to raise_error

      expect(received_events.size).to eq(1)
      expect(received_events.first[:task_id]).to eq('456')
    end
  end

  describe 'Standalone operation without Rails' do
    it 'works completely independently of Rails engine' do
      # Verify TaskerCore publisher works without any Rails dependencies
      expect(publisher).to be_a(TaskerCore::Events::Publisher)
      expect(publisher.registered_event_count).to be > 0
      expect(publisher.event_registered?('task.completed')).to be true
    end

    it 'provides full Rails-compatible API' do
      # Test that our standalone API matches Rails engine patterns
      received_event = nil
      publisher.subscribe('workflow.transition_requested') { |event| received_event = event }

      result = publisher.publish_step_transition(
        'step_123',
        'task_456',
        'in_progress',
        'error',
        { error_message: 'Test error' }
      )

      expect(result).to be true
      expect(received_event).not_to be_nil
      expect(received_event[:entity_type]).to eq('step')
      expect(received_event[:entity_id]).to eq('step_123')
      expect(received_event[:task_id]).to eq('task_456')
      expect(received_event[:error_message]).to eq('Test error')
    end
  end
end
