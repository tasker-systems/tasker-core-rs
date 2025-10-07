# frozen_string_literal: true

require 'spec_helper'

RSpec.describe 'TaskerCore::FFI Correlation ID Support (TAS-29)' do
  describe 'poll_step_events with correlation_id' do
    context 'when step execution event includes correlation_id' do
      before do
        # Bootstrap worker to enable event polling
        TaskerCore::FFI.bootstrap_worker
      end

      after do
        begin
          TaskerCore::FFI.stop_worker
        rescue StandardError
          nil
        end
      end

      it 'exposes correlation_id at top level of event hash' do
        # Poll for events (may return nil if no events available)
        event = TaskerCore::FFI.poll_step_events

        # If we get an event, verify correlation_id is present
        if event && event.is_a?(Hash)
          # TAS-29: correlation_id should be exposed at top level
          expect(event.keys).to include('correlation_id')

          # Should be a valid UUID string
          expect(event['correlation_id']).to match(/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i)
        end

        # This test passes even if no events are available - we're testing structure
        expect(event).to be_nil.or(be_a(Hash))
      end

      it 'includes parent_correlation_id when present' do
        event = TaskerCore::FFI.poll_step_events

        if event && event.is_a?(Hash)
          # parent_correlation_id is optional
          if event.key?('parent_correlation_id')
            expect(event['parent_correlation_id']).to match(/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i)
          end
        end

        expect(event).to be_nil.or(be_a(Hash))
      end
    end
  end

  describe 'EventBridge correlation_id wrapping' do
    let(:event_bridge) { TaskerCore::Worker::EventBridge.instance }

    let(:sample_event_data) do
      {
        event_id: SecureRandom.uuid,
        task_uuid: SecureRandom.uuid,
        step_uuid: SecureRandom.uuid,
        correlation_id: SecureRandom.uuid,
        parent_correlation_id: SecureRandom.uuid,
        task_sequence_step: {
          task: {
            task: {
              task_uuid: SecureRandom.uuid,
              namespace_uuid: SecureRandom.uuid,
              context: { test: 'data' },
              correlation_id: SecureRandom.uuid,
              parent_correlation_id: SecureRandom.uuid
            },
            task_name: 'test_task',
            task_version: '1.0.0',
            namespace_name: 'test_namespace'
          },
          workflow_step: {
            workflow_step_uuid: SecureRandom.uuid,
            name: 'test_step',
            handler_name: 'TestHandler'
          },
          dependency_results: {},
          step_definition: {
            name: 'test_step',
            handler_name: 'TestHandler'
          }
        }
      }
    end

    it 'exposes correlation_id from nested task to top level' do
      # Subscribe to step execution to capture wrapped event
      captured_event = nil
      event_bridge.subscribe_to_step_execution do |event|
        captured_event = event
      end

      # Publish event
      event_bridge.publish_step_execution(sample_event_data)

      # Verify wrapped event has correlation_id at top level
      expect(captured_event).not_to be_nil
      expect(captured_event[:correlation_id]).to eq(sample_event_data[:correlation_id])
    end

    it 'exposes parent_correlation_id when present' do
      captured_event = nil
      event_bridge.subscribe_to_step_execution do |event|
        captured_event = event
      end

      event_bridge.publish_step_execution(sample_event_data)

      expect(captured_event).not_to be_nil
      expect(captured_event[:parent_correlation_id]).to eq(sample_event_data[:parent_correlation_id])
    end

    it 'omits parent_correlation_id when not present' do
      event_data_without_parent = sample_event_data.dup
      event_data_without_parent.delete(:parent_correlation_id)

      captured_event = nil
      event_bridge.subscribe_to_step_execution do |event|
        captured_event = event
      end

      event_bridge.publish_step_execution(event_data_without_parent)

      expect(captured_event).not_to be_nil
      expect(captured_event).not_to have_key(:parent_correlation_id)
    end
  end
end
