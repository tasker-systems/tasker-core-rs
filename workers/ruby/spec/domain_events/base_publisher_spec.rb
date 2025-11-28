# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::DomainEvents::BasePublisher do
  # Custom test publisher
  class CustomTestPublisher < TaskerCore::DomainEvents::BasePublisher
    attr_reader :before_publish_called, :after_publish_called, :transform_called

    def initialize
      super
      @before_publish_called = false
      @after_publish_called = false
      @transform_called = false
    end

    def name
      'CustomTestPublisher'
    end

    def transform_payload(step_result, event_declaration, step_context)
      @transform_called = true
      {
        transformed: true,
        original_success: step_result[:success],
        event_name: event_declaration[:name]
      }
    end

    def before_publish(step_result, event_declaration, step_context)
      @before_publish_called = true
      true
    end

    def after_publish(step_result, event_declaration, step_context)
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
    it 'calls the hook and returns true' do
      result = publisher.before_publish(step_result, event_declaration, step_context)

      expect(result).to be true
      expect(publisher.before_publish_called).to be true
    end
  end

  describe '#after_publish' do
    it 'calls the hook' do
      publisher.after_publish(step_result, event_declaration, step_context)

      expect(publisher.after_publish_called).to be true
    end
  end

  describe 'MockEventPublisher' do
    let(:mock_publisher) { TaskerCore::TestHelpers::MockEventPublisher.new(name: 'MockTest') }

    describe '#events_by_name' do
      it 'filters captured events by name' do
        mock_publisher.before_publish(step_result, event_declaration, step_context)

        other_event = TaskerCore::TestHelpers::EventFactory.event_declaration(name: 'other.event')
        mock_publisher.before_publish(step_result, other_event, step_context)

        expect(mock_publisher.events_by_name('test.event')).to have(1).item
        expect(mock_publisher.events_by_name('other.event')).to have(1).item
      end
    end

    describe '#events_matching' do
      it 'filters events by pattern' do
        mock_publisher.before_publish(step_result, event_declaration, step_context)

        other_event = TaskerCore::TestHelpers::EventFactory.event_declaration(name: 'test.other')
        mock_publisher.before_publish(step_result, other_event, step_context)

        different_event = TaskerCore::TestHelpers::EventFactory.event_declaration(name: 'payment.processed')
        mock_publisher.before_publish(step_result, different_event, step_context)

        expect(mock_publisher.events_matching('test.*')).to have(2).items
        expect(mock_publisher.events_matching('payment.*')).to have(1).item
      end
    end

    describe '#simulate_failure!' do
      it 'causes before_publish to raise an error' do
        mock_publisher.simulate_failure!

        expect { mock_publisher.before_publish(step_result, event_declaration, step_context) }
          .to raise_error(StandardError, 'Simulated failure')
      end
    end

    describe '#clear!' do
      it 'removes all captured events' do
        mock_publisher.before_publish(step_result, event_declaration, step_context)
        expect(mock_publisher.published_events).not_to be_empty

        mock_publisher.clear!
        expect(mock_publisher.published_events).to be_empty
      end
    end
  end
end
