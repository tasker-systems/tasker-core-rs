# frozen_string_literal: true

require 'spec_helper'

RSpec.describe TaskerCore::DomainEvents::SubscriberRegistry do
  let(:registry) { described_class.instance }

  before do
    registry.reset!
  end

  # Test subscriber class
  class TestEventSubscriber < TaskerCore::DomainEvents::BaseSubscriber
    subscribes_to 'test.*'

    attr_reader :handled_events

    def initialize
      super
      @handled_events = []
    end

    def handle(event)
      @handled_events << event
    end
  end

  describe '#register' do
    it 'registers a subscriber class' do
      expect { registry.register(TestEventSubscriber) }.not_to raise_error
      expect(registry.count).to eq(1)
    end

    it 'raises error for non-BaseSubscriber class' do
      expect { registry.register(String) }.to raise_error(ArgumentError)
    end
  end

  describe '#register_instance' do
    it 'registers a subscriber instance' do
      subscriber = TestEventSubscriber.new
      expect { registry.register_instance(subscriber) }.not_to raise_error
      expect(registry.count).to eq(1)
    end

    it 'raises error for non-BaseSubscriber instance' do
      expect { registry.register_instance('not a subscriber') }.to raise_error(ArgumentError)
    end
  end

  describe '#start_all!' do
    it 'starts all registered subscribers' do
      registry.register(TestEventSubscriber)
      registry.start_all!

      expect(registry.started?).to be true
      expect(registry.stats[:active_count]).to eq(1)
    end

    it 'is idempotent' do
      registry.register(TestEventSubscriber)
      registry.start_all!
      registry.start_all! # Should not error

      expect(registry.stats[:subscriber_count]).to eq(1)
    end
  end

  describe '#stop_all!' do
    it 'stops all running subscribers' do
      registry.register(TestEventSubscriber)
      registry.start_all!
      registry.stop_all!

      expect(registry.started?).to be false
      expect(registry.stats[:active_count]).to eq(0)
    end

    it 'is idempotent' do
      registry.register(TestEventSubscriber)
      registry.start_all!
      registry.stop_all!
      registry.stop_all! # Should not error

      expect(registry.started?).to be false
    end
  end

  describe '#stats' do
    it 'returns subscriber statistics' do
      registry.register(TestEventSubscriber)
      stats = registry.stats

      expect(stats).to include(
        started: false,
        subscriber_count: 0, # Not instantiated until start_all!
        active_count: 0
      )
    end

    it 'includes subscriber details after starting' do
      registry.register(TestEventSubscriber)
      registry.start_all!
      stats = registry.stats

      expect(stats[:subscribers].size).to eq(1)
      expect(stats[:subscribers].first).to include(
        active: true,
        patterns: ['test.*']
      )
    end
  end
end
