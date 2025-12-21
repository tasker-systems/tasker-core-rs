# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::DomainEvents::BasePublisher do
  # Custom test publisher for testing transform and filtering hooks
  class CustomTestPublisher < TaskerCore::DomainEvents::BasePublisher
    attr_reader :before_publish_called, :after_publish_called, :transform_called
    attr_reader :last_event_name, :last_payload, :last_metadata

    def initialize
      super
      @before_publish_called = false
      @after_publish_called = false
      @transform_called = false
    end

    def name
      'CustomTestPublisher'
    end

    def transform_payload(step_result, event_declaration, step_context = nil)
      @transform_called = true
      {
        transformed: true,
        original_success: step_result[:success],
        event_name: event_declaration[:name]
      }
    end

    # Correct signature: (event_name, payload, metadata)
    def before_publish(event_name, payload, metadata)
      @before_publish_called = true
      @last_event_name = event_name
      @last_payload = payload
      @last_metadata = metadata
    end

    # Correct signature: (event_name, payload, metadata)
    def after_publish(event_name, payload, metadata)
      @after_publish_called = true
    end
  end

  let(:publisher) { CustomTestPublisher.new }
  let(:step_result) { TaskerCore::TestHelpers::EventFactory.step_result(success: true) }
  let(:event_declaration) { TaskerCore::TestHelpers::EventFactory.event_declaration(name: 'test.event') }
  let(:step_context) { TaskerCore::TestHelpers::EventFactory.step_context }

  describe '#name' do
    it 'returns the publisher name' do
      expect(publisher.name).to eq('CustomTestPublisher')
    end
  end

  describe '#transform_payload' do
    it 'transforms the step result into event payload' do
      payload = publisher.transform_payload(step_result, event_declaration, step_context)

      expect(payload).to include(
        transformed: true,
        original_success: true,
        event_name: 'test.event'
      )
      expect(publisher.transform_called).to be true
    end
  end

  describe '#should_publish?' do
    it 'returns true by default' do
      expect(publisher.should_publish?(step_result, event_declaration, step_context)).to be true
    end
  end

  describe '#additional_metadata' do
    it 'returns empty hash by default' do
      expect(publisher.additional_metadata(step_result, event_declaration, step_context)).to eq({})
    end
  end

  describe '#before_publish' do
    it 'calls the hook with event details' do
      publisher.before_publish('test.event', { data: 'test' }, { publisher: 'Test' })

      expect(publisher.before_publish_called).to be true
      expect(publisher.last_event_name).to eq('test.event')
      expect(publisher.last_payload).to eq({ data: 'test' })
      expect(publisher.last_metadata[:publisher]).to eq('Test')
    end
  end

  describe '#after_publish' do
    it 'calls the hook' do
      publisher.after_publish('test.event', { data: 'test' }, { publisher: 'Test' })

      expect(publisher.after_publish_called).to be true
    end
  end

  describe '#publish (TAS-96 cross-language standard)' do
    let(:publish_context) do
      {
        event_name: 'test.published',
        step_result: step_result,
        event_declaration: event_declaration,
        step_context: step_context
      }
    end

    it 'publishes when should_publish? returns true' do
      result = publisher.publish(publish_context)

      expect(result).to be true
      expect(publisher.transform_called).to be true
      expect(publisher.before_publish_called).to be true
      expect(publisher.after_publish_called).to be true
    end

    it 'passes correct event name to hooks' do
      publisher.publish(publish_context)

      expect(publisher.last_event_name).to eq('test.published')
    end

    it 'passes transformed payload to hooks' do
      publisher.publish(publish_context)

      expect(publisher.last_payload[:transformed]).to be true
    end

    it 'includes publisher metadata' do
      publisher.publish(publish_context)

      expect(publisher.last_metadata[:publisher]).to eq('CustomTestPublisher')
      expect(publisher.last_metadata[:published_at]).to be_a(String)
    end

    context 'when should_publish? returns false' do
      class SkippingPublisher < TaskerCore::DomainEvents::BasePublisher
        def name
          'SkippingPublisher'
        end

        def should_publish?(_step_result, _event_declaration, _step_context = nil)
          false
        end
      end

      let(:skipping_publisher) { SkippingPublisher.new }

      it 'returns false without calling hooks' do
        result = skipping_publisher.publish(publish_context)

        expect(result).to be false
      end
    end

    context 'when step_context has task/step info' do
      let(:step_context_with_info) do
        double('step_context',
               task_uuid: 'task-123',
               step_uuid: 'step-456',
               step_name: 'validate_order',
               namespace_name: 'payments')
      end

      let(:publish_context_with_info) do
        {
          event_name: 'test.published',
          step_result: step_result,
          event_declaration: event_declaration,
          step_context: step_context_with_info
        }
      end

      it 'includes task and step info in metadata' do
        publisher.publish(publish_context_with_info)

        expect(publisher.last_metadata[:task_uuid]).to eq('task-123')
        expect(publisher.last_metadata[:step_uuid]).to eq('step-456')
        expect(publisher.last_metadata[:step_name]).to eq('validate_order')
        expect(publisher.last_metadata[:namespace]).to eq('payments')
      end
    end

    context 'when before_publish raises an error' do
      class FailingPublisher < TaskerCore::DomainEvents::BasePublisher
        attr_reader :error_handled

        def name
          'FailingPublisher'
        end

        def before_publish(_event_name, _payload, _metadata)
          raise StandardError, 'Pre-publish validation failed'
        end

        def on_publish_error(_event_name, _error, _payload)
          @error_handled = true
        end
      end

      let(:failing_publisher) { FailingPublisher.new }

      it 'calls on_publish_error and returns false' do
        result = failing_publisher.publish(publish_context)

        expect(result).to be false
        expect(failing_publisher.error_handled).to be true
      end
    end
  end

  describe 'MockEventPublisher' do
    let(:mock_publisher) { TaskerCore::TestHelpers::MockEventPublisher.new(name: 'MockTest') }

    describe '#events_by_name' do
      it 'filters captured events by name' do
        # Use new signature: before_publish(event_name, payload, metadata)
        mock_publisher.before_publish('test.event', { data: 'test1' }, { publisher: 'MockTest' })
        mock_publisher.before_publish('other.event', { data: 'test2' }, { publisher: 'MockTest' })

        expect(mock_publisher.events_by_name('test.event').size).to eq(1)
        expect(mock_publisher.events_by_name('other.event').size).to eq(1)
      end
    end

    describe '#events_matching' do
      it 'filters events by pattern' do
        mock_publisher.before_publish('test.event', { data: 'test1' }, { publisher: 'MockTest' })
        mock_publisher.before_publish('test.other', { data: 'test2' }, { publisher: 'MockTest' })
        mock_publisher.before_publish('payment.processed', { data: 'pay' }, { publisher: 'MockTest' })

        expect(mock_publisher.events_matching('test.*').size).to eq(2)
        expect(mock_publisher.events_matching('payment.*').size).to eq(1)
      end
    end

    describe '#simulate_failure!' do
      it 'causes before_publish to raise an error' do
        mock_publisher.simulate_failure!

        expect { mock_publisher.before_publish('test.event', {}, {}) }
          .to raise_error(StandardError, 'Simulated failure')
      end
    end

    describe '#clear!' do
      it 'removes all captured events' do
        mock_publisher.before_publish('test.event', { data: 'test' }, { publisher: 'MockTest' })
        expect(mock_publisher.published_events).not_to be_empty

        mock_publisher.clear!
        expect(mock_publisher.published_events).to be_empty
      end
    end

    describe 'integration with publish(ctx)' do
      it 'captures events through the publish flow' do
        ctx = {
          event_name: 'order.completed',
          step_result: step_result,
          event_declaration: event_declaration,
          step_context: step_context
        }

        mock_publisher.publish(ctx)

        expect(mock_publisher.published_events.size).to eq(1)
        expect(mock_publisher.events_by_name('order.completed').size).to eq(1)
      end
    end
  end
end
